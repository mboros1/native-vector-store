#pragma once
#include <string>
#include <vector>
#include <memory>
#include <unordered_map>
#include <sys/mman.h>
#include <fcntl.h>
#include <unistd.h>
#include <cstdint>
#include <omp.h>
#include <simdjson.h>

// VectorStore V2 - Bundle-based implementation
// Works exclusively with pre-built bundles for optimal performance
// No JSON parsing at runtime, direct mmap access to all data

class VectorStoreV2 {
public:
    // Search result structure
    struct SearchResult {
        size_t doc_id;
        double score;
        std::string id;
        std::string text;
        std::string metadata_json;
    };
    
    // Memory-mapped file wrapper
    class MMapFile {
    private:
        void* data_ = nullptr;
        size_t size_ = 0;
        int fd_ = -1;
        
    public:
        MMapFile() = default;
        ~MMapFile() { close(); }
        
        // Move-only
        MMapFile(const MMapFile&) = delete;
        MMapFile& operator=(const MMapFile&) = delete;
        MMapFile(MMapFile&& other) noexcept;
        MMapFile& operator=(MMapFile&& other) noexcept;
        
        bool open(const std::string& path);
        void close();
        
        void* data() const { return data_; }
        size_t size() const { return size_; }
        
        template<typename T>
        const T* as() const { return static_cast<const T*>(data_); }
    };
    
private:
    // Bundle metadata from manifest
    struct BundleInfo {
        std::string format;
        size_t num_docs = 0;
        size_t dim = 0;
        std::string embedding_model;
        std::string dtype;
        double bm25_avgdl = 0;
        double bm25_k1 = 1.2;
        double bm25_b = 0.75;
    } info_;
    
    // Memory-mapped files
    std::string bundle_path_;
    MMapFile vectors_file_;
    MMapFile doclen_file_;
    MMapFile lexicon_file_;
    MMapFile postings_file_;
    MMapFile terms_file_;
    MMapFile meta_idx_file_;
    MMapFile meta_file_;
    
    // Parsed data pointers
    const float* vectors_ = nullptr;
    const uint32_t* doclen_ = nullptr;
    const uint64_t* meta_offsets_ = nullptr;
    
    // BM25 structures
    struct LexiconEntry {
        uint64_t offset;
        uint32_t length;
        uint32_t df;
    };
    const LexiconEntry* lexicon_ = nullptr;
    
    // Term dictionary (term -> term_id)
    std::unordered_map<std::string, size_t> term_to_id_;
    
    // Remove pre-allocated scores buffer - we'll use per-thread heaps instead</    
    // Helper structure for top-k tracking
    struct TopK {
        size_t k;
        std::vector<std::pair<float, size_t>> heap; // min-heap by score
        
        explicit TopK(size_t k = 0) : k(k) { 
            heap.reserve(k + 1);
        }
        
        // Make TopK move-only to prevent copy-construction races
        TopK(const TopK&) = delete;
        TopK& operator=(const TopK&) = delete;
        TopK(TopK&&) = default;
        TopK& operator=(TopK&&) = default;
        
        void push(float score, size_t idx) {
            if (heap.size() < k) {
                heap.emplace_back(score, idx);
                std::push_heap(heap.begin(), heap.end(), cmp);
            } else if (k > 0 && score > heap.front().first) {
                std::pop_heap(heap.begin(), heap.end(), cmp);
                heap.back() = {score, idx};
                std::push_heap(heap.begin(), heap.end(), cmp);
            }
        }
        
        static bool cmp(const std::pair<float, size_t>& a, const std::pair<float, size_t>& b) { 
            return a.first > b.first;
        }
        
        void merge(const TopK& other) {
            if (heap.size() + other.heap.size() <= k) {
                heap.insert(heap.end(), other.heap.begin(), other.heap.end());
                std::make_heap(heap.begin(), heap.end(), cmp);
            } else {
                for (const auto& [score, idx] : other.heap) {
                    push(score, idx);
                }
            }
        }
    };
    
    // State
    bool is_open_ = false;
    
public:
    VectorStoreV2() = default;
    explicit VectorStoreV2(const std::string& bundle_path);
    ~VectorStoreV2() = default;
    
    // Open a bundle (can be called after default construction)
    bool open(const std::string& bundle_path);
    
    // Close the bundle and release resources
    void close();
    
    // Check if store is ready
    bool is_open() const { return is_open_; }
    
    // Search functions
    std::vector<SearchResult> search(const float* query, size_t k) const;
    std::vector<SearchResult> search_bm25(const std::vector<std::string>& query_terms, size_t k) const;
    std::vector<SearchResult> search_hybrid(const float* query_vector, 
                                           const std::vector<std::string>& query_terms,
                                           size_t k = 10,
                                           double vector_weight = 0.7) const;
    
    // Accessors
    size_t size() const { return info_.num_docs; }
    size_t dimensions() const { return info_.dim; }
    const std::string& bundle_path() const { return bundle_path_; }
    const BundleInfo& info() const { return info_; }
    
    // Get document by ID (for compatibility)
    bool get_document(size_t doc_id, SearchResult& result) const;
    
private:
    // Helper functions
    bool load_manifest();
    bool load_term_dictionary();
    bool open_data_files();
    
    // Retrieve document metadata
    bool get_document_metadata(size_t doc_id, std::string& id, std::string& text, std::string& metadata) const;
    
    // BM25 scoring
    double compute_bm25_score(size_t doc_id, const std::unordered_map<std::string, int>& query_tf) const;
    
    // Vector similarity (dot product)
    float compute_similarity(const float* a, const float* b) const;
    
    // Helper to decode delta-encoded postings
    std::vector<std::pair<uint32_t, uint32_t>> decode_posting_list(size_t term_id) const;
};