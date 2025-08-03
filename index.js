const { VectorStore } = require('node-gyp-build')(__dirname);

/**
 * @typedef {Object} Document
 * @property {string} id - Unique identifier for the document
 * @property {string} text - The text content of the document
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
   * // Document format in JSON files
   * {
   *   "id": "doc-123",
   *   "text": "Document content...",
   *   "metadata": {
   *     "embedding": [0.1, 0.2, ...],  // Required: embedding vector
   *     "category": "product"           // Optional: additional metadata
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
   * Search for the k most similar documents to a query embedding.
   * Uses SIMD-optimized cosine similarity for fast performance.
   * 
   * @param {Float32Array} query - Query embedding vector (must match store dimensions)
   * @param {number} k - Number of results to return (top-k nearest neighbors)
   * @param {boolean} [normalizeQuery=true] - Whether to L2-normalize the query vector
   * @returns {SearchResult[]} Array of search results sorted by similarity (highest first)
   * @throws {Error} If store is not finalized or query dimensions don't match
   * 
   * @example
   * // Basic search
   * const queryEmbedding = new Float32Array(1536);
   * const results = store.search(queryEmbedding, 10);
   * 
   * @example
   * // Search with pre-normalized query
   * const normalized = normalizeVector(queryEmbedding);
   * const results = store.search(normalized, 5, false);
   * 
   * @example
   * // Filter results by score threshold
   * const results = store.search(queryEmbedding, 20)
   *   .filter(r => r.score > 0.7);
   */
  search(query, k, normalizeQuery = true) {}

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
module.exports = { VectorStore };