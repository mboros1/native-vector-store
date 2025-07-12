#pragma once
#include <atomic>
#include <memory>
#include <vector>
#include <string_view>
#include <simdjson.h>
#include <mutex>
#include <shared_mutex>

// Design note on references vs pointers:
// - Use references (&) when the parameter must not be null and won't change
// - Use pointers (*) only when nullptr is a valid value or ownership transfer
// - For output parameters, prefer return values or use references with clear names

// Document represents a searchable item with its text content and metadata.
// The metadata includes the embedding vector used for similarity search.
struct Document {
    std::string_view id;            // Unique identifier
    std::string_view text;          // Searchable text content
    std::string_view metadata_json; // Raw JSON including embedding vector
};

// ArenaAllocator provides fast, cache-friendly memory allocation.
// It works by allocating large chunks (64MB) and handing out slices.
// This avoids fragmentation and keeps related data close in memory.
class ArenaAllocator {
    static constexpr size_t CHUNK_SIZE = 1 << 26;  // 64MB per chunk
    static constexpr size_t MAX_ALIGN = 4096;      // Max alignment supported
    
    struct Chunk {
        alignas(64) char data[CHUNK_SIZE];  // Cache-line aligned storage
        std::atomic<size_t> offset{0};      // Next free byte in this chunk
        std::atomic<Chunk*> next{nullptr};  // Link to next chunk (if needed)
    };
    
    std::unique_ptr<Chunk> head_;           // First chunk (owned)
    std::atomic<Chunk*> current_;           // Current chunk for allocation
    std::mutex chunk_creation_mutex_;       // Serializes new chunk creation
    
public:
    ArenaAllocator();
    ~ArenaAllocator();
    
    // Allocates memory with specified alignment.
    // Returns nullptr if allocation fails (size too large or out of memory).
    // Thread-safe: multiple threads can allocate concurrently.
    void* allocate(size_t size, size_t align = 64);
};

// VectorStore provides high-performance similarity search over documents.
// It works in two phases:
// 1. Loading phase: Documents can be added (thread-safe)
// 2. Serving phase: Documents can be searched (after finalize() is called)
class VectorStore {
public:
    // Creates a store for embeddings of the specified dimension.
    // Pre-allocates space for up to 1M documents.
    explicit VectorStore(size_t embedding_dimensions);
    
    // === Loading Phase Methods ===
    
    // Adds a document from parsed JSON.
    // The JSON must contain: id, text, and metadata.embedding array.
    // Thread-safe during loading phase only.
    // Returns error if called after finalize().
    simdjson::error_code add_document(simdjson::ondemand::document& json_doc);
    simdjson::error_code add_document(simdjson::ondemand::object& json_obj);
    
    // Transitions from loading to serving phase.
    // This normalizes all embeddings to unit vectors for cosine similarity.
    // After this, no more documents can be added but search becomes available.
    // Safe to call multiple times (subsequent calls are no-ops).
    void finalize();
    
    // === Serving Phase Methods ===
    
    // Searches for the k most similar documents to the query embedding.
    // Uses cosine similarity (dot product of normalized vectors).
    // Returns pairs of (similarity_score, document_index).
    // Thread-safe but serialized - only one search runs at a time.
    // Returns empty vector if called before finalize().
    std::vector<std::pair<float, size_t>> search(
        const float* query_embedding, 
        size_t k
    ) const;
    
    // === Accessors ===
    
    // Gets a document by its index (from search results).
    const Document& get_document(size_t index) const;
    
    // Returns the number of documents loaded.
    size_t size() const;
    
    // Returns true if finalize() has been called.
    bool is_finalized() const;
    
private:
    // Entry combines a document with its embedding vector pointer.
    // The embedding is stored separately for cache-efficient similarity computation.
    struct Entry {
        Document doc;
        float* embedding;  // Points into arena-allocated memory
    };
    
    // === Member Variables ===
    
    const size_t dimensions_;               // Embedding vector size
    ArenaAllocator arena_;                  // Memory pool for all data
    std::vector<Entry> entries_;            // Document storage (pre-sized to 1M)
    std::atomic<size_t> count_{0};          // Number of documents loaded
    std::atomic<bool> is_finalized_{false}; // Phase flag: false=loading, true=serving
    mutable std::shared_mutex search_mutex_; // Serializes search operations
    
    // === Private Methods ===
    
    // Validates that we're in the loading phase.
    // Returns false if we're in serving phase (finalized).
    bool ensure_loading_phase() const;
    
    // Parses and validates an embedding array from JSON.
    // Fills output_embedding vector and returns error on failure.
    simdjson::error_code parse_embedding(
        simdjson::ondemand::array& embedding_array,
        std::vector<float>& output_embedding
    );
    
    // Allocates memory for document components in a single block.
    // Layout: [embedding_vector][id_string][text_string][metadata_json]
    // Returns nullptr if allocation fails.
    char* allocate_document_memory(
        size_t embedding_bytes,
        size_t id_bytes,
        size_t text_bytes,
        size_t metadata_bytes
    );
    
    // Normalizes all embedding vectors to unit length.
    // This converts dot product to cosine similarity.
    // Called once during finalize().
    void normalize_all_embeddings();
    
    // Computes dot product between two vectors using SIMD.
    // Assumes both vectors have dimensions_ elements.
    float compute_dot_product(const float* vec1, const float* vec2) const;
    
    // Performs parallel search using OpenMP.
    // Each thread maintains its own top-k heap to avoid contention.
    // Results are merged after all threads complete.
    std::vector<std::pair<float, size_t>> parallel_top_k_search(
        const float* query,
        size_t k,
        size_t num_documents
    ) const;
};