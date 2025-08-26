/**
 * @file vector_store.h
 * @brief High-performance vector similarity search engine with BM25 hybrid search
 * 
 * This header defines the VectorStore class, a thread-safe, SIMD-optimized vector
 * database designed for Model Context Protocol (MCP) servers and RAG applications.
 * 
 * @author Martin Boros
 * @date 2024
 */

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
#include <unordered_map>
#include <string>
#include <parallel_hashmap/phmap.h>

/**
 * @enum VectorStoreError
 * @brief Error codes for VectorStore operations
 * 
 * Comprehensive error codes including JSON parsing errors mapped from simdjson.
 * Used throughout the API for error handling without exceptions.
 */
enum class VectorStoreError {
    SUCCESS = 0,
    MEMORY_ALLOCATION_FAILED,
    DIMENSION_MISMATCH,
    MISSING_FIELD,
    WRONG_TYPE,
    STORE_NOT_FINALIZED,
    STORE_ALREADY_FINALIZED,
    CAPACITY_EXCEEDED,
    JSON_PARSE_ERROR,
    FILE_IO_ERROR,
    UNKNOWN_ERROR,
    // JSON parsing specific errors (mapped from simdjson)
    JSON_CAPACITY,
    JSON_TAPE_ERROR,
    JSON_DEPTH_ERROR,
    JSON_STRING_ERROR,
    JSON_T_ATOM_ERROR,
    JSON_F_ATOM_ERROR,
    JSON_N_ATOM_ERROR,
    JSON_NUMBER_ERROR,
    JSON_UTF8_ERROR,
    JSON_UNINITIALIZED,
    JSON_EMPTY,
    JSON_UNESCAPED_CHARS,
    JSON_UNCLOSED_STRING,
    JSON_UNSUPPORTED_ARCHITECTURE,
    JSON_INCORRECT_TYPE,
    JSON_NUMBER_OUT_OF_RANGE,
    JSON_INDEX_OUT_OF_BOUNDS,
    JSON_NO_SUCH_FIELD,
    JSON_IO_ERROR,
    JSON_INVALID_JSON_POINTER,
    JSON_INVALID_URI_FRAGMENT,
    JSON_UNEXPECTED_ERROR,
    JSON_PARSER_IN_USE,
    JSON_OUT_OF_ORDER_ITERATION,
    JSON_INSUFFICIENT_PADDING,
    JSON_INCOMPLETE_ARRAY_OR_OBJECT,
    JSON_SCALAR_DOCUMENT_AS_VALUE,
    JSON_OUT_OF_BOUNDS,
    JSON_TRAILING_CONTENT
};

/**
 * @brief Maps simdjson error codes to VectorStoreError
 * @param error The simdjson error code
 * @return Corresponding VectorStoreError code
 */
inline VectorStoreError map_simdjson_error(simdjson::error_code error) {
    using namespace simdjson;
    switch (error) {
        case SUCCESS: return VectorStoreError::SUCCESS;
        case CAPACITY: return VectorStoreError::JSON_CAPACITY;
        case MEMALLOC: return VectorStoreError::MEMORY_ALLOCATION_FAILED;
        case TAPE_ERROR: return VectorStoreError::JSON_TAPE_ERROR;
        case DEPTH_ERROR: return VectorStoreError::JSON_DEPTH_ERROR;
        case STRING_ERROR: return VectorStoreError::JSON_STRING_ERROR;
        case T_ATOM_ERROR: return VectorStoreError::JSON_T_ATOM_ERROR;
        case F_ATOM_ERROR: return VectorStoreError::JSON_F_ATOM_ERROR;
        case N_ATOM_ERROR: return VectorStoreError::JSON_N_ATOM_ERROR;
        case NUMBER_ERROR: return VectorStoreError::JSON_NUMBER_ERROR;
        case UTF8_ERROR: return VectorStoreError::JSON_UTF8_ERROR;
        case UNINITIALIZED: return VectorStoreError::JSON_UNINITIALIZED;
        case EMPTY: return VectorStoreError::JSON_EMPTY;
        case UNESCAPED_CHARS: return VectorStoreError::JSON_UNESCAPED_CHARS;
        case UNCLOSED_STRING: return VectorStoreError::JSON_UNCLOSED_STRING;
        case UNSUPPORTED_ARCHITECTURE: return VectorStoreError::JSON_UNSUPPORTED_ARCHITECTURE;
        case INCORRECT_TYPE: return VectorStoreError::JSON_INCORRECT_TYPE;
        case NUMBER_OUT_OF_RANGE: return VectorStoreError::JSON_NUMBER_OUT_OF_RANGE;
        case INDEX_OUT_OF_BOUNDS: return VectorStoreError::JSON_INDEX_OUT_OF_BOUNDS;
        case NO_SUCH_FIELD: return VectorStoreError::JSON_NO_SUCH_FIELD;
        case IO_ERROR: return VectorStoreError::JSON_IO_ERROR;
        case INVALID_JSON_POINTER: return VectorStoreError::JSON_INVALID_JSON_POINTER;
        case INVALID_URI_FRAGMENT: return VectorStoreError::JSON_INVALID_URI_FRAGMENT;
        case UNEXPECTED_ERROR: return VectorStoreError::JSON_UNEXPECTED_ERROR;
        case PARSER_IN_USE: return VectorStoreError::JSON_PARSER_IN_USE;
        case OUT_OF_ORDER_ITERATION: return VectorStoreError::JSON_OUT_OF_ORDER_ITERATION;
        case INSUFFICIENT_PADDING: return VectorStoreError::JSON_INSUFFICIENT_PADDING;
        case INCOMPLETE_ARRAY_OR_OBJECT: return VectorStoreError::JSON_INCOMPLETE_ARRAY_OR_OBJECT;
        case SCALAR_DOCUMENT_AS_VALUE: return VectorStoreError::JSON_SCALAR_DOCUMENT_AS_VALUE;
        case OUT_OF_BOUNDS: return VectorStoreError::JSON_OUT_OF_BOUNDS;
        case TRAILING_CONTENT: return VectorStoreError::JSON_TRAILING_CONTENT;
        default: return VectorStoreError::JSON_PARSE_ERROR;
    }
}

/**
 * @brief Converts VectorStoreError to human-readable string
 * @param error The error code to convert
 * @return C-string with error description
 */
inline const char* vector_store_error_message(VectorStoreError error) {
    switch (error) {
        case VectorStoreError::SUCCESS: return "Success";
        case VectorStoreError::MEMORY_ALLOCATION_FAILED: return "Memory allocation failed";
        case VectorStoreError::DIMENSION_MISMATCH: return "Embedding dimension mismatch";
        case VectorStoreError::MISSING_FIELD: return "Required field missing";
        case VectorStoreError::WRONG_TYPE: return "Wrong field type";
        case VectorStoreError::STORE_NOT_FINALIZED: return "Store must be finalized before searching";
        case VectorStoreError::STORE_ALREADY_FINALIZED: return "Store already finalized, cannot add more documents";
        case VectorStoreError::CAPACITY_EXCEEDED: return "Store capacity exceeded";
        case VectorStoreError::JSON_PARSE_ERROR: return "JSON parsing error";
        case VectorStoreError::FILE_IO_ERROR: return "File I/O error";
        case VectorStoreError::UNKNOWN_ERROR: return "Unknown error";
        // JSON specific errors
        case VectorStoreError::JSON_CAPACITY: return "JSON parser capacity exceeded";
        case VectorStoreError::JSON_TAPE_ERROR: return "JSON tape error";
        case VectorStoreError::JSON_DEPTH_ERROR: return "JSON depth error";
        case VectorStoreError::JSON_STRING_ERROR: return "JSON string error";
        case VectorStoreError::JSON_T_ATOM_ERROR: return "JSON 'true' atom error";
        case VectorStoreError::JSON_F_ATOM_ERROR: return "JSON 'false' atom error";
        case VectorStoreError::JSON_N_ATOM_ERROR: return "JSON 'null' atom error";
        case VectorStoreError::JSON_NUMBER_ERROR: return "JSON number error";
        case VectorStoreError::JSON_UTF8_ERROR: return "JSON UTF-8 error";
        case VectorStoreError::JSON_UNINITIALIZED: return "JSON parser uninitialized";
        case VectorStoreError::JSON_EMPTY: return "JSON document empty";
        case VectorStoreError::JSON_UNESCAPED_CHARS: return "JSON unescaped characters";
        case VectorStoreError::JSON_UNCLOSED_STRING: return "JSON unclosed string";
        case VectorStoreError::JSON_UNSUPPORTED_ARCHITECTURE: return "JSON unsupported architecture";
        case VectorStoreError::JSON_INCORRECT_TYPE: return "JSON incorrect type";
        case VectorStoreError::JSON_NUMBER_OUT_OF_RANGE: return "JSON number out of range";
        case VectorStoreError::JSON_INDEX_OUT_OF_BOUNDS: return "JSON index out of bounds";
        case VectorStoreError::JSON_NO_SUCH_FIELD: return "JSON field not found";
        case VectorStoreError::JSON_IO_ERROR: return "JSON I/O error";
        case VectorStoreError::JSON_INVALID_JSON_POINTER: return "JSON invalid pointer";
        case VectorStoreError::JSON_INVALID_URI_FRAGMENT: return "JSON invalid URI fragment";
        case VectorStoreError::JSON_UNEXPECTED_ERROR: return "JSON unexpected error";
        case VectorStoreError::JSON_PARSER_IN_USE: return "JSON parser in use";
        case VectorStoreError::JSON_OUT_OF_ORDER_ITERATION: return "JSON out of order iteration";
        case VectorStoreError::JSON_INSUFFICIENT_PADDING: return "JSON insufficient padding";
        case VectorStoreError::JSON_INCOMPLETE_ARRAY_OR_OBJECT: return "JSON incomplete array or object";
        case VectorStoreError::JSON_SCALAR_DOCUMENT_AS_VALUE: return "JSON scalar document as value";
        case VectorStoreError::JSON_OUT_OF_BOUNDS: return "JSON out of bounds";
        case VectorStoreError::JSON_TRAILING_CONTENT: return "JSON trailing content";
        default: return "Unknown error";
    }
}

/**
 * @class ArenaAllocator
 * @brief Arena memory allocator optimized for batch allocations
 * 
 * Allocates memory in 64MB chunks to minimize fragmentation and improve cache locality.
 * Per-thread instance - no synchronization needed.
 * Memory is never freed until the allocator is destroyed.
 */
class ArenaAllocator {
    static constexpr size_t CHUNK_SIZE = 1 << 26;  ///< 64MB chunks for cache efficiency
    
    /**
     * @struct Chunk
     * @brief Memory chunk with offset tracking
     */
    struct Chunk {
        alignas(64) char data[CHUNK_SIZE];
        size_t offset{0};
        Chunk* next{nullptr};
    };
    
    std::unique_ptr<Chunk> head_;
    Chunk* current_;
    
public:
    ArenaAllocator();
    void* allocate(size_t size, size_t align = 64);
    ~ArenaAllocator();
};


/**
 * @struct Document
 * @brief Lightweight document representation using string views
 * 
 * Zero-copy structure that references data in arena memory.
 * All string_views point to arena-allocated memory that persists
 * for the lifetime of the VectorStore.
 */
struct Document {
    std::string_view id;              ///< Unique document identifier
    std::string_view text;            ///< Document text content
    std::string_view metadata_json;   ///< Full JSON metadata including embedding
};

/**
 * @struct TopK
 * @brief Per-thread top-k result tracker using min-heap
 * 
 * Efficiently maintains top-k results during parallel search operations.
 * Uses a min-heap to keep only the k best results, discarding lower scores.
 * Move-only semantics prevent accidental copies in parallel contexts.
 */
struct TopK {
    size_t k;                                    ///< Number of top results to keep
    std::vector<std::pair<float, size_t>> heap; ///< Min-heap of (score, index) pairs
    
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

/**
 * @class VectorStore
 * @brief High-performance vector similarity search engine with hybrid BM25 support
 * 
 * Two-phase lifecycle design:
 * 1. Loading Phase: Add documents concurrently, no searches allowed
 * 2. Serving Phase: After finalize(), searches allowed, no modifications
 * 
 * Features:
 * - SIMD-optimized similarity computation with OpenMP
 * - Lock-free parallel document loading
 * - BM25 text search with configurable parameters  
 * - Hybrid search combining vector and text similarity
 * - Arena allocation for cache-efficient memory layout
 * - Zero-copy document storage
 * 
 * Thread Safety:
 * - Loading phase: Concurrent add_document() calls safe
 * - Serving phase: Unlimited concurrent searches
 * - Phase transition: finalize() provides memory barrier
 * 
 * @note Designed for <1M documents, optimal for <100k
 */
class VectorStore {
public:
    /**
     * @struct Entry
     * @brief Internal document entry with embedding and BM25 metadata
     */
    struct Entry {
        Document doc;       ///< Document data (id, text, metadata)
        float* embedding;   ///< Direct pointer to embedding for SIMD operations
        
        // BM25 fields
        size_t length;      ///< Total number of tokens in document
        phmap::flat_hash_map<std::string, int> tf;  ///< Term frequencies for BM25
    };

private:
    const size_t dim_;
    
    // Per-thread arena allocators for zero-contention parallel allocation
    std::vector<std::unique_ptr<ArenaAllocator>> thread_arenas_;
    
    std::vector<Entry> entries_;
    std::atomic<size_t> count_{0};  // Atomic for parallel loading
    std::atomic<bool> is_finalized_{false};  // Simple flag: false = loading, true = serving
    mutable std::shared_mutex search_mutex_;  // Protects against overlapping OpenMP teams
    
    // Auto-detect text field name for Spring AI compatibility
    enum class TextFieldType { UNKNOWN, TEXT, CONTENT };
    std::atomic<TextFieldType> text_field_type_{TextFieldType::UNKNOWN};
    
    // BM25 index structures - using parallel hashmap for lock-free concurrent updates
    phmap::parallel_flat_hash_map<
        std::string, 
        std::vector<size_t>,
        phmap::priv::hash_default_hash<std::string>,
        phmap::priv::hash_default_eq<std::string>,
        std::allocator<std::pair<const std::string, std::vector<size_t>>>,
        4,  // 2^4 = 16 submaps for parallelism
        std::mutex  // Use std::mutex for each submap
    > postings_;  // term -> list of doc indices
    
    phmap::parallel_flat_hash_map<
        std::string,
        int,  // Regular int - parallel hashmap provides synchronization
        phmap::priv::hash_default_hash<std::string>,
        phmap::priv::hash_default_eq<std::string>,
        std::allocator<std::pair<const std::string, int>>,
        4,  // 16 submaps
        std::mutex
    > doc_freq_;  // document frequencies
    
    std::atomic<size_t> total_length_{0};  // sum of all document lengths - now atomic
    // Note: bm25_index_mutex_ removed - no longer needed with parallel hashmap!
    
    // BM25 parameters
    double k1_ = 1.2;
    double b_ = 0.75; 
    double delta_ = 1.0;
    
public:
    /**
     * @brief Constructs a VectorStore with specified embedding dimensions
     * @param dim Number of dimensions for embeddings
     */
    explicit VectorStore(size_t dim);
    
    /**
     * @brief Adds a document from simdjson document object
     * @param json_doc JSON document containing id, text/content, and metadata.embedding
     * @return Error code indicating success or failure reason
     */
    VectorStoreError add_document(simdjson::ondemand::document& json_doc);
    
    /**
     * @brief Adds a document from simdjson object
     * @param json_doc JSON object containing id, text/content, and metadata.embedding
     * @return Error code indicating success or failure reason
     */
    VectorStoreError add_document(simdjson::ondemand::object& json_doc);
    
    /**
     * @struct BatchState
     * @brief Thread-local batch reservation for lock-free parallel loading
     * 
     * Reduces contention on atomic counter by reserving document indices
     * in batches per thread.
     */
    struct BatchState {
        size_t batch_size;    ///< Number of indices to reserve at once
        size_t batch_start;   ///< Start index of current batch
        size_t batch_offset;  ///< Current offset within batch
        
        BatchState(size_t size = 128) : batch_size(size), batch_start(0), batch_offset(0) {}
        
        /**
         * @brief Reserves next document index from batch
         * @param count Global atomic document counter
         * @return Reserved document index
         */
        size_t reserve_next(std::atomic<size_t>& count) {
            if (batch_offset >= batch_size) {
                // Need new batch
                batch_start = count.fetch_add(batch_size, std::memory_order_relaxed);
                batch_offset = 0;
            }
            return batch_start + batch_offset++;
        }
    };
    
    /**
     * @brief Gets thread-local batch state for current thread
     * @return Reference to thread's batch state
     */
    static BatchState& get_batch_state();
    
    /**
     * @brief Finalizes store for searching - normalizes embeddings and switches to serving phase
     * 
     * Must be called after all documents are added and before any searches.
     * Provides memory barrier for safe phase transition.
     */
    void finalize();
    
    /**
     * @deprecated Use finalize() instead
     */
    void normalize_all();
    
    /**
     * @brief Performs k-nearest neighbor vector similarity search
     * @param query Query embedding vector (must have dim_ dimensions)
     * @param k Number of top results to return
     * @return Vector of (score, document_index) pairs sorted by score descending
     * @note Uses SIMD-optimized dot product with OpenMP parallelization
     */
    std::vector<std::pair<float, size_t>> 
    search(const float* __restrict__ query, size_t k) const;
    
    /**
     * @brief Performs BM25 text search
     * @param query_terms Vector of query terms (should be tokenized/normalized)
     * @return Vector of (document_index, BM25_score) pairs
     */
    std::vector<std::pair<size_t, double>> 
    search_bm25(const std::vector<std::string>& query_terms) const;
    
    /**
     * @brief Performs hybrid search combining vector and BM25 scores
     * @param query_vector Query embedding vector
     * @param query_terms Query terms for BM25
     * @param vector_weight Weight for vector similarity (0-1)
     * @param bm25_weight Weight for BM25 score (0-1)
     * @param k Number of top results to return
     * @return Vector of (document_index, combined_score) pairs
     * @note Uses Reciprocal Rank Fusion (RRF) for score combination
     */
    std::vector<std::pair<size_t, double>>
    search_hybrid(const float* __restrict__ query_vector, const std::vector<std::string>& query_terms, 
                  double vector_weight = 0.7, double bm25_weight = 0.3, size_t k = 10) const;
    
    /**
     * @brief Sets BM25 algorithm parameters
     * @param k1 Term frequency saturation parameter (typical: 1.2)
     * @param b Document length normalization (0-1, typical: 0.75)
     * @param delta Smoothing parameter (typical: 1.0)
     */
    void set_bm25_parameters(double k1, double b, double delta);
    
    /**
     * @brief Gets document entry by index
     * @param idx Document index
     * @return Reference to Entry structure
     */
    const Entry& get_entry(size_t idx) const;
    
    /**
     * @brief Gets number of documents in store
     * @return Document count
     */
    size_t size() const;
    
    /**
     * @brief Checks if store is finalized and ready for searching
     * @return True if finalized, false if still in loading phase
     */
    bool is_finalized() const;
    
    /**
     * @brief Gets average document length for BM25 calculations
     * @return Average document length in tokens
     */
    double avg_doc_length() const;
};