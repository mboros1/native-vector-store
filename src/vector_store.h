#pragma once
#include <atomic>
#include <memory>
#include <cstring>
#include <vector>
#include <string_view>
#include <simdjson.h>
#include <omp.h>

class ArenaAllocator {
    static constexpr size_t CHUNK_SIZE = 1 << 26;  // 64MB chunks
    struct Chunk {
        alignas(64) char data[CHUNK_SIZE];
        std::atomic<size_t> offset{0};
        std::unique_ptr<Chunk> next;
    };
    
    std::unique_ptr<Chunk> head_;
    std::atomic<Chunk*> current_;
    
public:
    ArenaAllocator() : head_(std::make_unique<Chunk>()), 
                       current_(head_.get()) {}
    
    void* allocate(size_t size, size_t align = 64) {
        size = (size + align - 1) & ~(align - 1);  // Round up
        
        Chunk* chunk = current_.load(std::memory_order_acquire);
        while (true) {
            size_t old_offset = chunk->offset.load(std::memory_order_relaxed);
            size_t new_offset = old_offset + size;
            
            if (new_offset > CHUNK_SIZE) {
                // Need new chunk
                Chunk* expected = chunk;
                if (!chunk->next) {
                    auto new_chunk = std::make_unique<Chunk>();
                    Chunk* raw_new = new_chunk.get();
                    chunk->next = std::move(new_chunk);
                    current_.compare_exchange_weak(expected, raw_new);
                }
                chunk = current_.load(std::memory_order_acquire);
                continue;
            }
            
            if (chunk->offset.compare_exchange_weak(old_offset, new_offset,
                                                   std::memory_order_release,
                                                   std::memory_order_relaxed)) {
                return chunk->data + old_offset;
            }
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
    
    std::vector<Entry> entries_;
    std::atomic<size_t> count_{0};
    
public:
    explicit VectorStore(size_t dim) : dim_(dim) {
        entries_.reserve(1'000'000);  // Pre-size to avoid realloc
    }
    
    void add_document(simdjson::ondemand::document& json_doc) {
        // Parse once
        auto id = json_doc["id"].get_string().value_unsafe();
        auto text = json_doc["text"].get_string().value_unsafe();
        auto metadata = json_doc["metadata"];
        auto emb_array = metadata["embedding"].get_array();
        
        // Calculate sizes
        size_t emb_size = dim_ * sizeof(float);
        size_t id_size = id.size() + 1;
        size_t text_size = text.size() + 1;
        
        // Get raw JSON for metadata
        std::string_view raw_json = json_doc.raw_json().value_unsafe();
        size_t meta_size = raw_json.size() + 1;
        
        // Single arena allocation
        char* base = (char*)arena_.allocate(emb_size + id_size + text_size + meta_size);
        
        // Layout: [embedding][id][text][metadata_json]
        float* emb_ptr = (float*)base;
        char* id_ptr = base + emb_size;
        char* text_ptr = id_ptr + id_size;
        char* meta_ptr = text_ptr + text_size;
        
        // Copy embedding for fast access
        size_t i = 0;
        for (auto v : emb_array) {
            emb_ptr[i++] = float(v.get_double().value_unsafe());
        }
        
        // Copy strings
        std::memcpy(id_ptr, id.data(), id.size() + 1);
        std::memcpy(text_ptr, text.data(), text.size() + 1);
        std::memcpy(meta_ptr, raw_json.data(), raw_json.size() + 1);
        
        // Store entry with both document and embedding pointer
        size_t idx = count_.fetch_add(1);
        entries_[idx] = Entry{
            .doc = Document{
                .id = std::string_view(id_ptr, id.size()),
                .text = std::string_view(text_ptr, text.size()),
                .metadata_json = std::string_view(meta_ptr, raw_json.size())
            },
            .embedding = emb_ptr
        };
    }
    
    void normalize_all() {
        size_t n = count_.load(std::memory_order_acquire);
        #pragma omp parallel for
        for (size_t i = 0; i < n; ++i) {
            float* emb = entries_[i].embedding;
            float sum = 0.0f;
            #pragma omp simd reduction(+:sum)
            for (size_t j = 0; j < dim_; ++j) {
                sum += emb[j] * emb[j];
            }
            float inv_norm = 1.0f / std::sqrt(sum);
            #pragma omp simd
            for (size_t j = 0; j < dim_; ++j) {
                emb[j] *= inv_norm;
            }
        }
    }
    
    std::vector<std::pair<float, size_t>> 
    search(const float* query, size_t k) const {
        size_t n = count_.load(std::memory_order_acquire);
        std::vector<float> scores(n);
        
        #pragma omp parallel for
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
};