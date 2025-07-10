#pragma once
#include <atomic>
#include <memory>
#include <cstring>
#include <cmath>
#include <vector>
#include <string_view>
#include <simdjson.h>
#include <omp.h>
#include <mutex>
#include <cassert>

class ArenaAllocator {
    static constexpr size_t CHUNK_SIZE = 1 << 26;  // 64MB chunks
    struct Chunk {
        alignas(64) char data[CHUNK_SIZE];
        std::atomic<size_t> offset{0};
        std::atomic<Chunk*> next{nullptr};
    };
    
    std::unique_ptr<Chunk> head_;
    std::atomic<Chunk*> current_;
    std::mutex chunk_creation_mutex_;
    
public:
    ArenaAllocator() : head_(std::make_unique<Chunk>()), 
                       current_(head_.get()) {}
    
    void* allocate(size_t size, size_t align = 64) {
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
    
    ~ArenaAllocator() {
        // Clean up linked chunks
        Chunk* chunk = head_->next.load(std::memory_order_acquire);
        while (chunk) {
            Chunk* next = chunk->next.load(std::memory_order_acquire);
            delete chunk;
            chunk = next;
        }
    }
};

struct Document {
    std::string_view id;
    std::string_view text;
    std::string_view metadata_json;  // Full JSON including embedding
};

class VectorStore {
    const size_t dim_;
    ArenaAllocator arena_;
    
    struct Entry {
        Document doc;
        float* embedding;  // Extracted pointer for fast access
    };
    
    enum class Phase {
        LOADING,     // Can add documents, no searches allowed
        SERVING      // No document additions, only searches
    };
    
    std::vector<Entry> entries_;
    std::atomic<size_t> count_{0};  // Atomic for parallel loading
    std::atomic<Phase> phase_{Phase::LOADING};
    
public:
    explicit VectorStore(size_t dim) : dim_(dim) {
        entries_.resize(1'000'000);  // Pre-size with default-constructed entries
    }
    
    // Overload for document type (used in test_main.cpp)
    simdjson::error_code add_document(simdjson::ondemand::document& json_doc) {
        simdjson::ondemand::object obj;
        auto error = json_doc.get_object().get(obj);
        if (error) {
            return error;
        }
        return add_document(obj);
    }
    
    simdjson::error_code add_document(simdjson::ondemand::object& json_doc) {
        // Check if we're in loading phase
        if (phase_.load(std::memory_order_acquire) != Phase::LOADING) {
            return simdjson::INCORRECT_TYPE;  // Cannot add documents after finalization
        }
        
        // Parse with error handling
        std::string_view id, text;
        auto error = json_doc["id"].get_string().get(id);
        if (error) return error;
        
        error = json_doc["text"].get_string().get(text);
        if (error) return error;
        
        // Calculate sizes
        size_t emb_size = dim_ * sizeof(float);
        size_t id_size = id.size() + 1;
        size_t text_size = text.size() + 1;
        
        // Allocate temporary buffer for embedding
        std::vector<float> temp_embedding;
        temp_embedding.reserve(dim_);
        
        // Process metadata and embedding first
        simdjson::ondemand::object metadata;
        error = json_doc["metadata"].get_object().get(metadata);
        if (error) return error;
        
        simdjson::ondemand::array emb_array;
        error = metadata["embedding"].get_array().get(emb_array);
        if (error) return error;
        
        // Consume the array before touching anything else  
        size_t i = 0;
        for (auto value_result : emb_array) {
            simdjson::ondemand::value v;
            error = value_result.get(v);
            if (error) return error;
            double val;
            error = v.get_double().get(val);
            if (error) return error;
            
            if (i >= dim_) {
                return simdjson::CAPACITY; // Too many embedding values
            }
            temp_embedding.push_back(float(val));
            i++;
        }
        
        // Verify we got the expected number of embedding values
        if (i != dim_) {
            return simdjson::INCORRECT_TYPE; // Wrong embedding dimension
        }
        
        // Now it is safe to take the raw metadata JSON
        std::string_view raw_json;
        error = metadata.raw_json().get(raw_json);
        if (error) return error;
        size_t meta_size = raw_json.size() + 1;
        
        // Single arena allocation
        char* base = (char*)arena_.allocate(emb_size + id_size + text_size + meta_size);
        if (!base) {
            return simdjson::MEMALLOC;  // Allocation failed
        }
        
        // Layout: [embedding][id][text][metadata_json]
        float* emb_ptr = (float*)base;
        char* id_ptr = base + emb_size;
        char* text_ptr = id_ptr + id_size;
        char* meta_ptr = text_ptr + text_size;
        
        // Copy embedding from temporary buffer
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
            return simdjson::CAPACITY;
        }
        
        // Construct entry directly - no synchronization needed
        entries_[idx] = Entry{
            .doc = Document{
                .id = std::string_view(id_ptr, id.size()),
                .text = std::string_view(text_ptr, text.size()),
                .metadata_json = std::string_view(meta_ptr, raw_json.size())
            },
            .embedding = emb_ptr
        };
        
        return simdjson::SUCCESS;
    }
    
    // Finalize the store: normalize and switch to serving phase
    void finalize() {
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
        
        // Switch to serving phase
        phase_.store(Phase::SERVING, std::memory_order_release);
    }
    
    // Deprecated: use finalize() instead
    void normalize_all() {
        finalize();
    }
    
    std::vector<std::pair<float, size_t>> 
    search(const float* query, size_t k) const {
        // Check if we're in serving phase
        if (phase_.load(std::memory_order_acquire) != Phase::SERVING) {
            return {};  // No searches during loading phase
        }
        
        size_t n = count_.load(std::memory_order_acquire);
        if (n == 0 || k == 0) return {};
        
        k = std::min(k, n);  // Ensure k doesn't exceed count
        std::vector<float> scores(n);
        
        #pragma omp parallel for if(omp_get_level() == 0)
        for (size_t i = 0; i < n; ++i) {
            float score = 0.0f;
            const float* emb = entries_[i].embedding;
            
            #pragma omp simd reduction(+:score)
            for (size_t j = 0; j < dim_; ++j) {
                score += emb[j] * query[j];
            }
            scores[i] = score;
        }
        
        // Top-k selection
        std::vector<std::pair<float, size_t>> idx(n);
        for (size_t i = 0; i < n; ++i) {
            idx[i] = {scores[i], i};
        }
        std::partial_sort(idx.begin(), idx.begin() + k, idx.end(),
                         [](const auto& a, const auto& b) { return a.first > b.first; });
        idx.resize(k);
        return idx;
    }
    
    const Entry& get_entry(size_t idx) const {
        return entries_[idx];
    }
    
    size_t size() const {
        return count_.load(std::memory_order_acquire);
    }
    
    bool is_finalized() const {
        return phase_.load(std::memory_order_acquire) == Phase::SERVING;
    }
};