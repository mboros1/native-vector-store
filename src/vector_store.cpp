#include "vector_store.h"

// ArenaAllocator implementation

ArenaAllocator::ArenaAllocator() : head_(std::make_unique<Chunk>()), 
                                   current_(head_.get()) {}

void* ArenaAllocator::allocate(size_t size, size_t align) {
    // Validate alignment is power of 2 and reasonable
    assert(align > 0 && (align & (align - 1)) == 0);
    if (align > 4096) {
        return nullptr;  // Alignment too large
    }
    
    // Validate size
    if (size > CHUNK_SIZE) {
        return nullptr;  // Cannot allocate larger than chunk size
    }
    
    Chunk* chunk = current_.load(std::memory_order_acquire);
    while (true) {
        size_t old_offset = chunk->offset.load(std::memory_order_relaxed);
        
        // Calculate the pointer that would result from current offset
        void* ptr = chunk->data + old_offset;
        
        // Calculate how much padding we need for alignment
        size_t misalignment = (uintptr_t)ptr & (align - 1);
        size_t padding = misalignment ? (align - misalignment) : 0;
        
        size_t aligned_offset = old_offset + padding;
        size_t new_offset = aligned_offset + size;
        
        if (new_offset > CHUNK_SIZE) {
            // Need new chunk
            Chunk* next = chunk->next.load(std::memory_order_acquire);
            if (!next) {
                // Lock to prevent multiple threads creating chunks
                std::lock_guard<std::mutex> lock(chunk_creation_mutex_);
                // Double-check after acquiring lock
                next = chunk->next.load(std::memory_order_acquire);
                if (!next) {
                    auto new_chunk = std::make_unique<Chunk>();
                    next = new_chunk.get();
                    chunk->next.store(next, std::memory_order_release);
                    // Transfer ownership after setting atomic pointer
                    new_chunk.release();
                }
            }
            // Update current to the new chunk
            current_.store(next, std::memory_order_release);
            chunk = next;
            continue;
        }
        
        if (chunk->offset.compare_exchange_weak(old_offset, new_offset,
                                               std::memory_order_release,
                                               std::memory_order_relaxed)) {
            return chunk->data + aligned_offset;
        }
    }
}

ArenaAllocator::~ArenaAllocator() {
    // Clean up linked chunks
    Chunk* chunk = head_->next.load(std::memory_order_acquire);
    while (chunk) {
        Chunk* next = chunk->next.load(std::memory_order_acquire);
        delete chunk;
        chunk = next;
    }
}


// TopK implementation

TopK::TopK(size_t k) : k(k) { 
    heap.reserve(k + 1); // Reserve k+1 to avoid reallocation during push
}

void TopK::push(float score, size_t idx) {
    if (heap.size() < k) {
        heap.emplace_back(score, idx);
        std::push_heap(heap.begin(), heap.end(), cmp);
    } else if (k > 0 && score > heap.front().first) {
        // Replace the minimum element
        std::pop_heap(heap.begin(), heap.end(), cmp);
        heap.back() = {score, idx};
        std::push_heap(heap.begin(), heap.end(), cmp);
    }
}

bool TopK::cmp(const std::pair<float, size_t>& a, const std::pair<float, size_t>& b) { 
    return a.first > b.first;
}

void TopK::merge(const TopK& other) {
    // More efficient: if we have space, bulk insert then re-heapify
    if (heap.size() + other.heap.size() <= k) {
        heap.insert(heap.end(), other.heap.begin(), other.heap.end());
        std::make_heap(heap.begin(), heap.end(), cmp);
    } else {
        // Otherwise, insert one by one
        for (const auto& [score, idx] : other.heap) {
            push(score, idx);
        }
    }
}

// VectorStore implementation

VectorStore::VectorStore(size_t dim) : dim_(dim) {
    entries_.resize(1'000'000);  // Pre-size with default-constructed entries
    
    // Prepare per-thread arena allocators for zero-contention parallel loading
    int max_threads = omp_get_max_threads();
    thread_arenas_.reserve(max_threads);
    for (int i = 0; i < max_threads; ++i) {
        thread_arenas_.emplace_back(std::make_unique<ArenaAllocator>());
    }
}

VectorStore::BatchState& VectorStore::get_batch_state() {
    thread_local BatchState state;
    return state;
}

VectorStoreError VectorStore::add_document(simdjson::ondemand::document& json_doc) {
    simdjson::ondemand::object obj;
    auto error = json_doc.get_object().get(obj);
    if (error) {
        return map_simdjson_error(error);
    }
    return add_document(obj);
}

VectorStoreError VectorStore::add_document(simdjson::ondemand::object& json_doc) {
    // Cannot add documents after finalization
    if (is_finalized_.load(std::memory_order_acquire)) {
        return VectorStoreError::STORE_ALREADY_FINALIZED;
    }
    
    // Parse with error handling
    std::string_view id, text;
    auto error = json_doc["id"].get_string().get(id);
    if (error) {
        if (error == simdjson::NO_SUCH_FIELD) {
            fprintf(stderr, "Missing required field 'id'\n");
        }
        return map_simdjson_error(error);
    }
    
    // Auto-detect text field type on first document, then use that for all subsequent documents
    TextFieldType field_type = text_field_type_.load(std::memory_order_acquire);
    
    if (field_type == TextFieldType::UNKNOWN) {
        // First document - detect field type
        error = json_doc["text"].get_string().get(text);
        if (!error) {
            // Found 'text' field - use it for all documents
            TextFieldType expected = TextFieldType::UNKNOWN;
            text_field_type_.compare_exchange_strong(expected, TextFieldType::TEXT, std::memory_order_release);
        } else if (error == simdjson::NO_SUCH_FIELD) {
            // Try 'content' field
            error = json_doc["content"].get_string().get(text);
            if (!error) {
                // Found 'content' field - use it for all documents
                TextFieldType expected = TextFieldType::UNKNOWN;
                text_field_type_.compare_exchange_strong(expected, TextFieldType::CONTENT, std::memory_order_release);
            } else {
                if (error == simdjson::NO_SUCH_FIELD) {
                    fprintf(stderr, "Missing required field 'text' or 'content'\n");
                }
                return map_simdjson_error(error);
            }
        } else {
            return map_simdjson_error(error);
        }
    } else if (field_type == TextFieldType::TEXT) {
        // Use 'text' field directly
        error = json_doc["text"].get_string().get(text);
        if (error) {
            if (error == simdjson::NO_SUCH_FIELD) {
                fprintf(stderr, "Missing required field 'text' (detected from first document)\n");
            }
            return map_simdjson_error(error);
        }
    } else { // TextFieldType::CONTENT
        // Use 'content' field directly
        error = json_doc["content"].get_string().get(text);
        if (error) {
            if (error == simdjson::NO_SUCH_FIELD) {
                fprintf(stderr, "Missing required field 'content' (detected from first document)\n");
            }
            return map_simdjson_error(error);
        }
    }
    
    // Process metadata and embedding first to get raw JSON before allocation
    simdjson::ondemand::object metadata;
    error = json_doc["metadata"].get_object().get(metadata);
    if (error) {
        if (error == simdjson::NO_SUCH_FIELD) {
            fprintf(stderr, "Missing required field 'metadata'\n");
        }
        return map_simdjson_error(error);
    }
    
    simdjson::ondemand::array emb_array;
    error = metadata["embedding"].get_array().get(emb_array);
    if (error) {
        if (error == simdjson::NO_SUCH_FIELD) {
            fprintf(stderr, "Missing required field 'embedding' inside 'metadata'\n");
        }
        return map_simdjson_error(error);
    }
    
    // Use thread-local temporary buffer for embedding to avoid allocation/free per document
    thread_local std::vector<float> temp_embedding;
    temp_embedding.clear();
    temp_embedding.reserve(dim_);
    
    // Fill embedding into temporary buffer
    size_t i = 0;
    for (auto value_result : emb_array) {
        simdjson::ondemand::value v;
        error = value_result.get(v);
        if (error) return map_simdjson_error(error);
        double val;
        error = v.get_double().get(val);
        if (error) return map_simdjson_error(error);
        
        if (i >= dim_) {
            fprintf(stderr, "Too many embedding values: expected %zu, got at least %zu\n", dim_, i+1);
            return VectorStoreError::DIMENSION_MISMATCH;
        }
        temp_embedding.push_back(float(val));
        i++;
    }
    
    // Verify we got the expected number of embedding values
    if (i != dim_) {
        fprintf(stderr, "Wrong embedding dimension: expected %zu, got %zu\n", dim_, i);
        return VectorStoreError::DIMENSION_MISMATCH;
    }
    
    // Now it is safe to take the raw metadata JSON
    std::string_view raw_json;
    error = metadata.raw_json().get(raw_json);
    if (error) return map_simdjson_error(error);
    
    // Calculate sizes
    size_t emb_size = dim_ * sizeof(float);
    size_t id_size = id.size() + 1;
    size_t text_size = text.size() + 1;
    size_t meta_size = raw_json.size() + 1;
    
    // Use per-thread arena allocator for zero-contention allocation
    // Get thread ID and dispatch to appropriate arena
#ifdef _OPENMP
    int tid = omp_get_thread_num();
#else
    // For non-OpenMP builds, assign each std::thread a small integer ID
    static std::atomic<size_t> counter{0};
    static thread_local size_t tid = counter++;
#endif
    
    // Ensure thread ID is within bounds
    if (tid >= static_cast<int>(thread_arenas_.size())) {
        tid = 0; // Fallback to first arena
    }
    
    char* base = (char*)thread_arenas_[tid]->allocate(emb_size + id_size + text_size + meta_size);
    if (!base) {
        return VectorStoreError::MEMORY_ALLOCATION_FAILED;
    }
    
    // Layout: [embedding][id][text][metadata_json]
    float* emb_ptr = (float*)base;
    char* id_ptr = base + emb_size;
    char* text_ptr = id_ptr + id_size;
    char* meta_ptr = text_ptr + text_size;
    
    // Copy embedding from thread-local buffer (no heap allocation per call)
    std::memcpy(emb_ptr, temp_embedding.data(), emb_size);
    
    // Copy strings (adding null terminator)
    std::memcpy(id_ptr, id.data(), id.size());
    id_ptr[id.size()] = '\0';
    
    std::memcpy(text_ptr, text.data(), text.size());
    text_ptr[text.size()] = '\0';
    
    std::memcpy(meta_ptr, raw_json.data(), raw_json.size());
    meta_ptr[raw_json.size()] = '\0';
    
    // Atomic increment for parallel loading
    size_t idx = count_.fetch_add(1, std::memory_order_relaxed);
    
    // Bounds check
    if (idx >= entries_.size()) {
        count_.fetch_sub(1, std::memory_order_relaxed);
        return VectorStoreError::CAPACITY_EXCEEDED;
    }
    
    // Construct entry directly - no synchronization needed
    // Use traditional initialization for C++17 compatibility
    Document doc;
    doc.id = std::string_view(id_ptr, id.size());
    doc.text = std::string_view(text_ptr, text.size());
    doc.metadata_json = std::string_view(meta_ptr, raw_json.size());
    
    Entry entry;
    entry.doc = doc;
    entry.embedding = emb_ptr;
    
    entries_[idx] = entry;
    
    return VectorStoreError::SUCCESS;
}

void VectorStore::finalize() {
    // If already finalized, do nothing
    if (is_finalized_.load(std::memory_order_acquire)) {
        return;
    }
    
    // Get final count
    size_t final_count = count_.load(std::memory_order_acquire);
    
    // Normalize all embeddings (single-threaded, no races)
    for (size_t i = 0; i < final_count; ++i) {
        float* emb = entries_[i].embedding;
        if (!emb) continue;  // Skip uninitialized entries
        
        float sum = 0.0f;
        #pragma omp simd reduction(+:sum)
        for (size_t j = 0; j < dim_; ++j) {
            sum += emb[j] * emb[j];
        }
        
        if (sum > 1e-10f) {  // Avoid division by zero
            float inv_norm = 1.0f / std::sqrt(sum);
            #pragma omp simd
            for (size_t j = 0; j < dim_; ++j) {
                emb[j] *= inv_norm;
            }
        }
    }
    
    // Ensure all threads see the normalized data
    #pragma omp barrier
    
    // Mark as finalized - this is the ONLY place this flag is set
    is_finalized_.store(true, std::memory_order_seq_cst);
}

void VectorStore::normalize_all() {
    finalize();
}

std::vector<std::pair<float, size_t>> 
VectorStore::search(const float* query, size_t k) const {
    // Exclusive lock: prevent overlapping OpenMP teams
    // Since each search uses all threads via OpenMP, concurrent searches provide no benefit
    std::unique_lock<std::shared_mutex> lock(search_mutex_);

    // Search can ONLY run if finalized
    if (!is_finalized_.load(std::memory_order_acquire)) {
        return {};
    }
    
    size_t n = count_.load(std::memory_order_acquire);
    if (n == 0 || k == 0) return {};
    
    k = std::min(k, n);  // Ensure k doesn't exceed count
    
    
    // Always use per-thread heaps to avoid any shared memory races
    const int num_threads = omp_get_max_threads();
    std::vector<TopK> thread_heaps;
    thread_heaps.reserve(num_threads);
    for (int i = 0; i < num_threads; ++i) {
        thread_heaps.emplace_back(k);  // in-place construction, no copies
    }

     std::vector<std::pair<float,std::size_t>> result;  
    
    #pragma omp parallel
    {
        const int tid = omp_get_thread_num();
        TopK& local_heap = thread_heaps[tid];
        
        #pragma omp for  // default barrier kept - ensures all threads finish before merge
        for (int i = 0; i < static_cast<int>(n); ++i) {
            float score = 0.0f;
            const float* emb = entries_[i].embedding;
            
            #pragma omp simd reduction(+:score)
            for (size_t j = 0; j < dim_; ++j) {
                score += emb[j] * query[j];
            }
            
            local_heap.push(score, i);
        }

        #pragma omp barrier
        #pragma omp single
        {
            TopK final_heap(k);
            for (auto& th : thread_heaps) final_heap.merge(th);
            result = std::move(final_heap.heap);
        }
    }
    
    std::sort(result.begin(), result.end(), 
              [](const auto& a, const auto& b) { return a.first > b.first; });
    
    return result;
}

const VectorStore::Entry& VectorStore::get_entry(size_t idx) const {
    return entries_[idx];
}

size_t VectorStore::size() const {
    return count_.load(std::memory_order_acquire);
}

bool VectorStore::is_finalized() const {
    return is_finalized_.load(std::memory_order_acquire);
}
