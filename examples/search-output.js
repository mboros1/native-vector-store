#!/usr/bin/env node

/**
 * This example demonstrates the JSON output format from native-vector-store searches
 */

const { VectorStore } = require('../index.js');

// Create and populate store
const store = new VectorStore(3);

// Add documents with various metadata
store.addDocument({
  id: "doc-001",
  text: "Introduction to machine learning and neural networks",
  metadata: {
    embedding: [0.1, 0.2, 0.3],
    category: "AI/ML",
    difficulty: "beginner",
    author: "Dr. Smith",
    published: "2024-01-15"
  }
});

store.addDocument({
  id: "doc-002",
  text: "Advanced deep learning architectures and transformers",
  metadata: {
    embedding: [0.4, 0.5, 0.6],
    category: "AI/ML",
    difficulty: "advanced",
    author: "Prof. Johnson",
    citations: 45
  }
});

store.addDocument({
  id: "doc-003",
  text: "Natural language processing with modern techniques",
  metadata: {
    embedding: [0.35, 0.45, 0.55],
    category: "NLP",
    tags: ["transformers", "BERT", "GPT"],
    lastUpdated: "2024-02-01"
  }
});

// Finalize store for searching
store.finalize();

// Perform search
const query = new Float32Array([0.38, 0.48, 0.58]);
const results = store.search(query, 3);

console.log("=== VectorStore Search Output ===\n");

// Show raw output structure
console.log("Raw search results:");
console.log(results);

// Show as formatted JSON
console.log("\nFormatted JSON output:");
console.log(JSON.stringify(results, null, 2));

// Parse and display each result
console.log("\n=== Detailed Results ===\n");
results.forEach((result, index) => {
  console.log(`Result ${index + 1}:`);
  console.log(`  Score: ${result.score.toFixed(6)}`);
  console.log(`  ID: ${result.id}`);
  console.log(`  Text: "${result.text}"`);
  
  // Parse the full metadata
  const metadata = JSON.parse(result.metadata_json);
  console.log(`  Full Metadata:`, metadata);
  console.log();
});

// Example: Filter by score and extract specific fields
console.log("=== Practical Usage Examples ===\n");

// 1. High confidence results only
const highConfidence = results.filter(r => r.score > 0.95);
console.log(`High confidence results (score > 0.95): ${highConfidence.length} found`);

// 2. Extract categories
const categories = results.map(r => {
  const meta = JSON.parse(r.metadata_json);
  return meta.category || 'uncategorized';
});
console.log(`Categories:`, categories);

// 3. Build a rich result object
const enrichedResults = results.map(r => ({
  id: r.id,
  score: r.score,
  text: r.text,
  ...JSON.parse(r.metadata_json)  // Spread all metadata fields
}));

console.log("\nEnriched results with spread metadata:");
console.log(JSON.stringify(enrichedResults[0], null, 2));

// Show the output format summary
console.log("\n=== Output Format Summary ===\n");
console.log(`Each search result contains:
{
  score: number,         // Cosine similarity (0-1, higher = more similar)
  id: string,            // Document ID
  text: string,          // Document text (or 'content' for Spring AI)  
  metadata_json: string  // JSON string with ALL metadata including embedding
}

The metadata_json field preserves ALL fields from the original metadata object,
not just the embedding. This allows you to store and retrieve any additional
data alongside your documents.`);