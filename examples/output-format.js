#!/usr/bin/env node

/**
 * This script demonstrates the JSON output format from native-vector-store searches
 */

const { VectorStore } = require('../index.js');

// Create a store with 3-dimensional embeddings for demo
const store = new VectorStore(3);

// Add some sample documents
const documents = [
  {
    id: "doc-001",
    text: "The quick brown fox jumps over the lazy dog",
    metadata: {
      embedding: [0.1, 0.2, 0.3],
      category: "animals",
      source: "nursery-rhyme",
      timestamp: "2024-01-15T10:30:00Z"
    }
  },
  {
    id: "doc-002", 
    text: "Artificial intelligence is transforming the technology landscape",
    metadata: {
      embedding: [0.4, 0.5, 0.6],
      category: "technology",
      source: "tech-blog",
      author: "Jane Smith"
    }
  },
  {
    id: "doc-003",
    text: "Machine learning models require large amounts of training data",
    metadata: {
      embedding: [0.35, 0.45, 0.55],
      category: "technology", 
      source: "research-paper",
      year: 2023
    }
  }
];

// Add documents to store
documents.forEach(doc => store.addDocument(doc));

// Finalize the store (required before searching)
store.finalize();

// Create a query embedding (similar to doc-002 and doc-003)
const queryEmbedding = new Float32Array([0.38, 0.48, 0.58]);

// Search for top 2 most similar documents
console.log("=== Search Results (k=2) ===\n");
const results = store.search(queryEmbedding, 2);

// Show raw output
console.log("Raw JavaScript output:");
console.log(results);
console.log();

// Show formatted JSON output
console.log("JSON formatted output:");
console.log(JSON.stringify(results, null, 2));
console.log();

// Demonstrate how to parse and use the results
console.log("=== Parsed Results ===\n");
results.forEach((result, index) => {
  console.log(`Result ${index + 1}:`);
  console.log(`  Score: ${result.score.toFixed(4)} (higher is more similar, max 1.0)`);
  console.log(`  ID: ${result.id}`);
  console.log(`  Text: ${result.text}`);
  
  // Parse the metadata JSON
  const metadata = JSON.parse(result.metadata_json);
  console.log(`  Metadata:`);
  console.log(`    Category: ${metadata.category}`);
  console.log(`    Source: ${metadata.source}`);
  console.log(`    Embedding: [${metadata.embedding.join(', ')}]`);
  
  // Show any additional metadata fields
  const knownFields = ['embedding', 'category', 'source'];
  const additionalFields = Object.keys(metadata).filter(k => !knownFields.includes(k));
  if (additionalFields.length > 0) {
    additionalFields.forEach(field => {
      console.log(`    ${field}: ${metadata[field]}`);
    });
  }
  console.log();
});

// Example of filtering by score threshold
console.log("=== Filtering by Score Threshold ===\n");
const threshold = 0.99;
const highScoreResults = results.filter(r => r.score > threshold);
console.log(`Documents with score > ${threshold}:`);
if (highScoreResults.length > 0) {
  highScoreResults.forEach(r => {
    console.log(`  - ${r.id}: ${r.text.substring(0, 50)}... (score: ${r.score.toFixed(4)})`);
  });
} else {
  console.log("  No documents found above threshold");
}

// Example of extracting specific metadata fields
console.log("\n=== Extracting Categories ===\n");
const categories = results.map(r => {
  const metadata = JSON.parse(r.metadata_json);
  return metadata.category;
});
console.log("Categories of results:", categories);

// Show the SearchResult TypeScript interface for reference
console.log("\n=== TypeScript Interface ===\n");
console.log(`interface SearchResult {
  score: number;        // Similarity score (0-1, higher is more similar)
  id: string;           // Document identifier  
  text: string;         // Document text content
  metadata_json: string; // Serialized metadata including embedding
}`);