#include "vector_store_improved.h"
#include <algorithm>
#include <cstring>
#include <cmath>
#include <cassert>
#include <omp.h>
#include <iostream>

// === ArenaAllocator Implementation ===

// ArenaAllocator provides fast memory allocation by handing out slices
// from large pre-allocated chunks. This reduces fragmentation and
// improves cache locality for related data.

ArenaAllocator::ArenaAllocator() 
    : head_(std::make_unique<Chunk>()), 
      current_(head_.get()) {
}

ArenaAllocator::~ArenaAllocator() {
    // Clean up the linked list of chunks
    // head_ is automatically deleted by unique_ptr
    Chunk* chunk = head_->next.load(std::memory_order_acquire);
    while (chunk) {
        Chunk* next = chunk->next.load(std::memory_order_acquire);
        delete chunk;
        chunk = next;
    }
}

void* ArenaAllocator::allocate(size_t size, size_t align) {
    // Validate alignment (must be power of 2)
    assert(align > 0 && (align & (align - 1)) == 0);
    if (align > MAX_ALIGN) {
        return nullptr;  // Alignment too large
    }
    
    // Validate size
    if (size > CHUNK_SIZE) {
        return nullptr;  // Cannot allocate larger than chunk size
    }
    
    // Try to allocate from current chunk
    Chunk* chunk = current_.load(std::memory_order_acquire);
    
    while (true) {
        size_t old_offset = chunk->offset.load(std::memory_order_relaxed);
        
        // Calculate aligned allocation position
        void* ptr = chunk->data + old_offset;
        size_t misalignment = (uintptr_t)ptr & (align - 1);
        size_t padding = misalignment ? (align - misalignment) : 0;
        
        size_t aligned_offset = old_offset + padding;
        size_t new_offset = aligned_offset + size;
        
        // Check if allocation fits in current chunk
        if (new_offset > CHUNK_SIZE) {
            // Need a new chunk
            Chunk* next = chunk->next.load(std::memory_order_acquire);
            if (!next) {
                // Create new chunk (serialize creation with mutex)
                std::lock_guard<std::mutex> lock(chunk_creation_mutex_);
                
                // Double-check after acquiring lock
                next = chunk->next.load(std::memory_order_acquire);
                if (!next) {
                    auto new_chunk = std::make_unique<Chunk>();
                    next = new_chunk.get();
                    chunk->next.store(next, std::memory_order_release);
                    // Release ownership after storing pointer
                    new_chunk.release();
                }
            }
            
            // Move to next chunk and retry
            current_.store(next, std::memory_order_release);
            chunk = next;
            continue;
        }
        
        // Try to claim this allocation slot
        if (chunk->offset.compare_exchange_weak(old_offset, new_offset,
                                               std::memory_order_release,
                                               std::memory_order_relaxed)) {
            return chunk->data + aligned_offset;
        }
        // If CAS failed, another thread allocated - retry
    }
}

// === VectorStore Implementation ===

// VectorStore provides similarity search using cosine similarity.
// It operates in two phases:
// 1. Loading: Documents can be added concurrently
// 2. Serving: Documents are normalized and can be searched

VectorStore::VectorStore(size_t embedding_dimensions) 
    : dimensions_(embedding_dimensions) {
    // Pre-allocate entry storage to avoid reallocation during loading
    entries_.resize(1'000'000);  // Support up to 1M documents
}

// Add document from parsed JSON document object
simdjson::error_code VectorStore::add_document(simdjson::ondemand::document& json_doc) {
    simdjson::ondemand::object obj;
    auto error = json_doc.get_object().get(obj);
    if (error) {
        return error;
    }
    return add_document(obj);
}

// Add document from parsed JSON object
simdjson::error_code VectorStore::add_document(simdjson::ondemand::object& json_obj) {
    // Check we're in loading phase
    if (!ensure_loading_phase()) {
        return simdjson::INCORRECT_TYPE;  // Cannot add after finalization
    }
    
    // Parse document fields
    std::string_view id, text;
    auto error = json_obj["id"].get_string().get(id);
    if (error) return error;
    
    error = json_obj["text"].get_string().get(text);
    if (error) return error;
    
    // Parse embedding from metadata
    std::vector<float> temp_embedding;
    error = parse_embedding(json_obj, temp_embedding);
    if (error) return error;
    
    // Get raw metadata JSON for storage
    simdjson::ondemand::object metadata;
    error = json_obj["metadata"].get_object().get(metadata);
    if (error) return error;
    
    std::string_view metadata_json;
    error = metadata.raw_json().get(metadata_json);
    if (error) return error;
    
    // Allocate memory for all document data in one block
    size_t embedding_bytes = dimensions_ * sizeof(float);
    char* memory = allocate_document_memory(
        embedding_bytes,
        id.size() + 1,
        text.size() + 1, 
        metadata_json.size() + 1
    );
    
    if (!memory) {
        return simdjson::MEMALLOC;
    }
    
    // Copy data into allocated memory
    // Layout: [embedding][id][text][metadata]
    float* emb_ptr = reinterpret_cast<float*>(memory);
    char* id_ptr = memory + embedding_bytes;
    char* text_ptr = id_ptr + (id.size() + 1);
    char* meta_ptr = text_ptr + (text.size() + 1);
    
    // Copy embedding
    std::memcpy(emb_ptr, temp_embedding.data(), embedding_bytes);
    
    // Copy strings with null terminators
    std::memcpy(id_ptr, id.data(), id.size());
    id_ptr[id.size()] = '\0';
    
    std::memcpy(text_ptr, text.data(), text.size());
    text_ptr[text.size()] = '\0';
    
    std::memcpy(meta_ptr, metadata_json.data(), metadata_json.size());
    meta_ptr[metadata_json.size()] = '\0';
    
    // Store entry (atomic increment for thread safety)
    size_t index = count_.fetch_add(1, std::memory_order_relaxed);
    if (index >= entries_.size()) {
        count_.fetch_sub(1, std::memory_order_relaxed);
        return simdjson::CAPACITY;  // Too many documents
    }
    
    entries_[index] = Entry{
        .doc = Document{
            .id = std::string_view(id_ptr, id.size()),
            .text = std::string_view(text_ptr, text.size()),
            .metadata_json = std::string_view(meta_ptr, metadata_json.size())
        },
        .embedding = emb_ptr
    };
    
    return simdjson::SUCCESS;
}

// Transition from loading to serving phase
void VectorStore::finalize() {
    bool expected = false;
    if (is_finalized_.compare_exchange_strong(expected, true,
                                             std::memory_order_seq_cst)) {
        // We're the thread that transitioned to finalized
        normalize_all_embeddings();
    }
}

// Search for k most similar documents
std::vector<std::pair<float, size_t>> VectorStore::search(
    const float* query_embedding, 
    size_t k
) const {
    // Ensure we're in serving phase
    if (!is_finalized_.load(std::memory_order_acquire)) {
        return {};
    }
    
    size_t n = count_.load(std::memory_order_acquire);
    if (n == 0 || k == 0) {
        return {};
    }
    
    k = std::min(k, n);
    
    // Lock to prevent concurrent OpenMP teams
    // (OpenMP doesn't handle nested parallelism well)
    std::unique_lock<std::shared_mutex> lock(search_mutex_);
    
    return parallel_top_k_search(query_embedding, k, n);
}

// Get document by index
const Document& VectorStore::get_document(size_t index) const {
    return entries_[index].doc;
}

// Get number of documents
size_t VectorStore::size() const {
    return count_.load(std::memory_order_acquire);
}

// Check if finalized
bool VectorStore::is_finalized() const {
    return is_finalized_.load(std::memory_order_acquire);
}

// === Private Implementation Methods ===

bool VectorStore::ensure_loading_phase() const {
    return !is_finalized_.load(std::memory_order_acquire);
}

// Parse embedding array from metadata object
simdjson::error_code VectorStore::parse_embedding(
    simdjson::ondemand::object& json_obj,
    std::vector<float>& output_embedding
) {
    output_embedding.clear();
    output_embedding.reserve(dimensions_);
    
    // Navigate to metadata.embedding array
    simdjson::ondemand::object metadata;
    auto error = json_obj["metadata"].get_object().get(metadata);
    if (error) return error;
    
    simdjson::ondemand::array emb_array;
    error = metadata["embedding"].get_array().get(emb_array);
    if (error) return error;
    
    // Parse embedding values
    size_t count = 0;
    for (auto value_result : emb_array) {
        simdjson::ondemand::value v;
        error = value_result.get(v);
        if (error) return error;
        
        double val;
        error = v.get_double().get(val);
        if (error) return error;
        
        if (count >= dimensions_) {
            return simdjson::CAPACITY;  // Too many values
        }
        
        output_embedding.push_back(static_cast<float>(val));
        count++;
    }
    
    // Verify correct dimension count
    if (count != dimensions_) {
        return simdjson::INCORRECT_TYPE;  // Wrong number of dimensions
    }
    
    return simdjson::SUCCESS;
}

// Allocate memory for document components
char* VectorStore::allocate_document_memory(
    size_t embedding_bytes,
    size_t id_bytes,
    size_t text_bytes,
    size_t metadata_bytes
) {
    size_t total = embedding_bytes + id_bytes + text_bytes + metadata_bytes;
    return static_cast<char*>(arena_.allocate(total, alignof(float)));
}

// Normalize all embeddings to unit vectors
void VectorStore::normalize_all_embeddings() {
    size_t n = count_.load(std::memory_order_acquire);
    
    // Parallel normalization using OpenMP
    #pragma omp parallel for
    for (size_t i = 0; i < n; ++i) {
        float* emb = entries_[i].embedding;
        if (!emb) continue;
        
        // Calculate L2 norm
        float sum = 0.0f;
        #pragma omp simd reduction(+:sum)
        for (size_t j = 0; j < dimensions_; ++j) {
            sum += emb[j] * emb[j];
        }
        
        // Normalize to unit vector
        if (sum > 1e-10f) {  // Avoid division by zero
            float inv_norm = 1.0f / std::sqrt(sum);
            #pragma omp simd
            for (size_t j = 0; j < dimensions_; ++j) {
                emb[j] *= inv_norm;
            }
        }
    }
}

// Compute dot product using SIMD
float VectorStore::compute_dot_product(const float* vec1, const float* vec2) const {
    float score = 0.0f;
    #pragma omp simd reduction(+:score)
    for (size_t j = 0; j < dimensions_; ++j) {
        score += vec1[j] * vec2[j];
    }
    return score;
}

// Per-thread top-k heap for parallel search
struct TopKHeap {
    size_t k;
    std::vector<std::pair<float, size_t>> data;
    
    explicit TopKHeap(size_t k) : k(k) {
        data.reserve(k + 1);
    }
    
    void push(float score, size_t index) {
        if (data.size() < k) {
            data.emplace_back(score, index);
            std::push_heap(data.begin(), data.end(), std::greater<>());
        } else if (score > data.front().first) {
            std::pop_heap(data.begin(), data.end(), std::greater<>());
            data.back() = {score, index};
            std::push_heap(data.begin(), data.end(), std::greater<>());
        }
    }
    
    void merge_into(TopKHeap& target) const {
        for (const auto& item : data) {
            target.push(item.first, item.second);
        }
    }
};

// Parallel top-k search implementation
std::vector<std::pair<float, size_t>> VectorStore::parallel_top_k_search(
    const float* query,
    size_t k,
    size_t num_documents
) const {
    // Create per-thread heaps
    const int num_threads = omp_get_max_threads();
    std::vector<TopKHeap> thread_heaps;
    thread_heaps.reserve(num_threads);
    for (int i = 0; i < num_threads; ++i) {
        thread_heaps.emplace_back(k);
    }
    
    // Parallel search
    #pragma omp parallel
    {
        int tid = omp_get_thread_num();
        TopKHeap& local_heap = thread_heaps[tid];
        
        #pragma omp for
        for (size_t i = 0; i < num_documents; ++i) {
            float score = compute_dot_product(entries_[i].embedding, query);
            local_heap.push(score, i);
        }
    }
    
    // Merge thread results
    TopKHeap final_heap(k);
    for (const auto& heap : thread_heaps) {
        heap.merge_into(final_heap);
    }
    
    // Sort final results by score (descending)
    std::sort(final_heap.data.begin(), final_heap.data.end(),
              [](const auto& a, const auto& b) { return a.first > b.first; });
    
    return final_heap.data;
}