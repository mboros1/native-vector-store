const { VectorStore } = require('node-gyp-build')(__dirname);

// VectorStoreV2 is the same as VectorStore - the native module now only implements v2
const VectorStoreV2 = VectorStore;

/**
 * @typedef {Object} Document
 * @property {string} id - Unique identifier for the document
 * @property {string} [text] - The text content of the document (mutually exclusive with content)
 * @property {string} [content] - Alternative to text for Spring AI compatibility (mutually exclusive with text)
 * @property {Object} metadata - Document metadata
 * @property {number[]} metadata.embedding - The embedding vector for the document
 * @property {*} [metadata.*] - Additional metadata properties
 */

/**
 * @typedef {Object} SearchResult
 * @property {number} score - Similarity score (0-1, higher is more similar)
 * @property {string} id - Document identifier
 * @property {string} text - Document text content
 * @property {string} metadata_json - Serialized metadata as JSON string
 */

/**
 * High-performance vector store with SIMD optimization for similarity search.
 * Designed for immutable, one-time loading scenarios with fast searches over focused corpora.
 * 
 * @class VectorStore
 * @example
 * // Basic usage
 * const store = new VectorStore(1536);
 * store.loadDir('./documents');
 * const results = store.search(queryEmbedding, 10);
 * 
 * @example
 * // Multiple domain-specific stores
 * const productStore = new VectorStore(1536);
 * const supportStore = new VectorStore(1536);
 * productStore.loadDir('./knowledge/products');
 * supportStore.loadDir('./knowledge/support');
 */
class VectorStoreWrapper {
  /**
   * Creates a new VectorStore instance
   * @param {number} dimensions - The dimensionality of embedding vectors (e.g., 1536 for OpenAI embeddings)
   * @throws {TypeError} If dimensions is not a positive integer
   */
  constructor(dimensions) {
    return new VectorStore(dimensions);
  }

  /**
   * Load all JSON documents from a directory and automatically finalize the store.
   * Documents should contain embedding vectors in their metadata field.
   * Supports both single documents and arrays of documents per file.
   * 
   * @param {string} path - Absolute or relative path to directory containing JSON files
   * @returns {void}
   * @throws {Error} If directory doesn't exist or contains invalid JSON
   * 
   * @example
   * // Load documents from a directory
   * store.loadDir('./knowledge-base');
   * // Store is automatically finalized and ready for searches
   * 
   * @example
   * // Standard format with 'text' field
   * {
   *   "id": "doc-123",
   *   "text": "Document content...",
   *   "metadata": {
   *     "embedding": [0.1, 0.2, ...],  // Required: embedding vector
   *     "category": "product"           // Optional: additional metadata
   *   }
   * }
   * 
   * @example
   * // Spring AI format with 'content' field
   * {
   *   "id": "doc-456",
   *   "content": "Document content...",  // 'content' instead of 'text'
   *   "metadata": {
   *     "embedding": [0.1, 0.2, ...],
   *     "category": "spring-ai"
   *   }
   * }
   */
  loadDir(path) {}

  /**
   * Add a single document to the store. Only works before finalization.
   * 
   * @param {Document} doc - Document object with embedding in metadata
   * @returns {void}
   * @throws {Error} If called after finalization or document format is invalid
   * 
   * @example
   * store.addDocument({
   *   id: 'doc-1',
   *   text: 'Sample document',
   *   metadata: {
   *     embedding: new Array(1536).fill(0).map(() => Math.random())
   *   }
   * });
   */
  addDocument(doc) {}

  /**
   * Search for the k most similar documents. Uses hybrid search if queryText is provided,
   * otherwise performs vector-only search.
   * 
   * @param {Float32Array} query - Query embedding vector (must match store dimensions)
   * @param {number} k - Number of results to return (top-k nearest neighbors)
   * @param {string} [queryText] - Optional text query for hybrid search (BM25 + vector)
   * @returns {SearchResult[]} Array of search results sorted by score (highest first)
   * @throws {Error} If store is not finalized or query dimensions don't match
   * 
   * @example
   * // Vector-only search
   * const queryEmbedding = new Float32Array(1536);
   * const results = store.search(queryEmbedding, 10);
   * 
   * @example
   * // Hybrid search (combines vector similarity with BM25 text search)
   * const results = store.search(queryEmbedding, 10, "machine learning algorithms");
   * 
   * @example
   * // Filter results by score threshold
   * const results = store.search(queryEmbedding, 20, "neural networks")
   *   .filter(r => r.score > 0.7);
   */
  search(query, k, queryText) {}
  
  /**
   * Pure vector similarity search using SIMD-optimized cosine similarity.
   * 
   * @param {Float32Array} query - Query embedding vector (must match store dimensions)
   * @param {number} k - Number of results to return
   * @param {boolean} [normalizeQuery=true] - Whether to L2-normalize the query vector
   * @returns {SearchResult[]} Array of search results sorted by similarity
   * @throws {Error} If store is not finalized or query dimensions don't match
   * 
   * @example
   * const results = store.searchVector(queryEmbedding, 10);
   */
  searchVector(query, k, normalizeQuery = true) {}
  
  /**
   * Pure BM25 text search for lexical matching.
   * 
   * @param {string|string[]} queryText - Query text or array of pre-tokenized terms
   * @param {number} k - Number of results to return
   * @returns {SearchResult[]} Array of search results sorted by BM25 score
   * @throws {Error} If store is not finalized
   * 
   * @example
   * // Search with text string
   * const results = store.searchBM25("machine learning algorithms", 10);
   * 
   * @example
   * // Search with pre-tokenized terms
   * const results = store.searchBM25(["machine", "learning"], 10);
   */
  searchBM25(queryText, k) {}
  
  /**
   * Hybrid search combining vector similarity and BM25 text search.
   * Uses Reciprocal Rank Fusion (RRF) to combine scores.
   * 
   * @param {Float32Array} query - Query embedding vector
   * @param {string} queryText - Query text for BM25 component
   * @param {number} k - Number of results to return
   * @param {number} [vectorWeight=0.5] - Weight for vector similarity (0-1)
   * @param {number} [bm25Weight=0.5] - Weight for BM25 score (0-1)
   * @returns {SearchResult[]} Array of search results sorted by combined score
   * @throws {Error} If store is not finalized or weights don't sum to 1
   * 
   * @example
   * // Equal weighting (default)
   * const results = store.searchHybrid(embedding, "neural networks", 10);
   * 
   * @example
   * // Favor text matching (70% BM25, 30% vector)
   * const results = store.searchHybrid(embedding, "deep learning", 10, 0.3, 0.7);
   */
  searchHybrid(query, queryText, k, vectorWeight = 0.5, bm25Weight = 0.5) {}
  
  /**
   * Set BM25 algorithm parameters for text search.
   * 
   * @param {number} k1 - Term frequency saturation parameter (default: 1.2, range: 0-3)
   * @param {number} b - Document length normalization (default: 0.75, range: 0-1)
   * @param {number} [delta=1.0] - Smoothing parameter for term frequency
   * @returns {void}
   * 
   * @example
   * // Reduce length normalization for short documents
   * store.setBM25Parameters(1.2, 0.5);
   * 
   * @example
   * // Increase term frequency saturation for keyword-heavy queries
   * store.setBM25Parameters(2.0, 0.75);
   */
  setBM25Parameters(k1, b, delta = 1.0) {}

  /**
   * Finalize the store: normalize all embeddings and switch to serving mode.
   * After calling this, no more documents can be added but searches become available.
   * This is automatically called by loadDir().
   * 
   * @returns {void}
   * 
   * @example
   * // Manual finalization after adding documents
   * store.addDocument(doc1);
   * store.addDocument(doc2);
   * store.finalize(); // Must call before searching
   */
  finalize() {}

  /**
   * Check if the store has been finalized and is ready for searching.
   * 
   * @returns {boolean} True if finalized, false otherwise
   * 
   * @example
   * if (store.isFinalized()) {
   *   const results = store.search(query, 10);
   * }
   */
  isFinalized() {}

  /**
   * Get the number of documents in the store.
   * 
   * @returns {number} Number of documents loaded
   * 
   * @example
   * console.log(`Loaded ${store.size()} documents`);
   */
  size() {}

  /**
   * @deprecated Use finalize() instead
   * @returns {void}
   */
  normalize() {}
}

// Re-export the native VectorStore as-is, but the JSDoc above provides documentation
module.exports = { VectorStore, VectorStoreV2 };