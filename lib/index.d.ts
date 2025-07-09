export interface Document {
  id: string;
  text: string;
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
  constructor(dimensions: number);
  
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
   * @param query - Query embedding vector
   * @param k - Number of results to return
   * @param normalizeQuery - Whether to L2 normalize the query (default: true)
   */
  search(query: Float32Array, k: number, normalizeQuery?: boolean): SearchResult[];
  
  /**
   * Normalize all stored embeddings
   */
  normalize(): void;
  
  /**
   * Get the number of documents in the store
   */
  size(): number;
}