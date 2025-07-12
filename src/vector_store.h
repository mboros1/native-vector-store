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
#include <shared_mutex>
#include <cassert>
#include <algorithm>
#include <functional>

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
    ArenaAllocator();
    void* allocate(size_t size, size_t align = 64);
    ~ArenaAllocator();
};

struct Document {
    std::string_view id;
    std::string_view text;
    std::string_view metadata_json;  // Full JSON including embedding
};

// Per-thread top-k tracker for thread-safe parallel search
struct TopK {
    size_t k;
    std::vector<std::pair<float, size_t>> heap; // min-heap by score
    
    explicit TopK(size_t k = 0);
    
    // Make TopK move-only to prevent copy-construction races
    TopK(const TopK&) = delete;
    TopK& operator=(const TopK&) = delete;
    TopK(TopK&&) = default;
    TopK& operator=(TopK&&) = default;
    
    void push(float score, size_t idx);
    
    // Comparator for min-heap (greater than for min-heap behavior)
    static bool cmp(const std::pair<float, size_t>& a, const std::pair<float, size_t>& b);
    
    void merge(const TopK& other);
};

class VectorStore {
public:
    struct Entry {
        Document doc;
        float* embedding;  // Extracted pointer for fast access
    };

private:
    const size_t dim_;
    ArenaAllocator arena_;
    
    std::vector<Entry> entries_;
    std::atomic<size_t> count_{0};  // Atomic for parallel loading
    std::atomic<bool> is_finalized_{false};  // Simple flag: false = loading, true = serving
    mutable std::shared_mutex search_mutex_;  // Protects against overlapping OpenMP teams
    
public:
    explicit VectorStore(size_t dim);
    
    // Overload for document type (used in test_main.cpp)
    simdjson::error_code add_document(simdjson::ondemand::document& json_doc);
    
    simdjson::error_code add_document(simdjson::ondemand::object& json_doc);
    
    // Finalize the store: normalize and switch to serving phase
    void finalize();
    
    // Deprecated: use finalize() instead
    void normalize_all();
    
    std::vector<std::pair<float, size_t>> 
    search(const float* query, size_t k) const;
    
    const Entry& get_entry(size_t idx) const;
    
    size_t size() const;
    
    bool is_finalized() const;
};