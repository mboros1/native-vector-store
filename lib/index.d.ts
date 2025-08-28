export interface Document {
  id: string;
  text?: string;      // Either text or content is required
  content?: string;   // Spring AI compatibility
  metadata: {
    embedding?: number[];
    [key: string]: any;
  };
}

export interface SearchResult {
  score: number;
  id: string;
  text: string;
  metadata_json: string;
}

export class VectorStore {
  constructor(bundlePath: string);
  
  /**
   * Load all JSON documents from a directory
   * Files should contain Document objects with embeddings in metadata
   */
  loadDir(path: string): void;
  
  /**
   * Add a single document
   */
  addDocument(doc: Document): void;
  
  /**
   * Search for k most similar documents
   * Uses hybrid search if queryText is provided, otherwise vector-only search
   * @param query - Query embedding vector
   * @param k - Number of results to return
   * @param queryText - Optional text query for hybrid search (BM25 + vector)
   */
  search(query: Float32Array, k: number, queryText?: string): SearchResult[];
  
  /**
   * Pure vector similarity search
   * @param query - Query embedding vector
   * @param k - Number of results to return
   * @param normalizeQuery - Whether to L2 normalize the query (default: true)
   */
  searchVector(query: Float32Array, k: number, normalizeQuery?: boolean): SearchResult[];
  
  /**
   * Pure BM25 text search
   * @param queryText - Text query or array of query terms
   * @param k - Number of results to return
   */
  searchBM25(queryText: string | string[], k: number): SearchResult[];
  
  /**
   * Hybrid search combining vector similarity and BM25 text search
   * @param query - Query embedding vector
   * @param queryText - Text query for BM25 component
   * @param k - Number of results to return
   * @param vectorWeight - Weight for vector similarity (default: 0.5)
   * @param bm25Weight - Weight for BM25 score (default: 0.5)
   */
  searchHybrid(query: Float32Array, queryText: string, k: number, vectorWeight?: number, bm25Weight?: number): SearchResult[];
  
  /**
   * Set BM25 parameters for text search
   * @param k1 - Controls term frequency saturation (default: 1.2)
   * @param b - Controls document length normalization (default: 0.75)
   * @param delta - Smoothing parameter (default: 1.0)
   */
  setBM25Parameters(k1: number, b: number, delta?: number): void;
  
  /**
   * Normalize all stored embeddings
   * @deprecated Use finalize() instead
   */
  normalize(): void;
  
  /**
   * Finalize the store: normalize embeddings and switch to serving mode
   * After calling this, no more documents can be added
   */
  finalize(): void;
  
  /**
   * Check if the store has been finalized
   */
  isFinalized(): boolean;
  
  /**
   * Get the number of documents in the store
   */
  size(): number;
}

export class VectorStoreV2 extends VectorStore {
  constructor(bundlePath: string);
}