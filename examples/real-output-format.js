#!/usr/bin/env node

/**
 * This script shows the ACTUAL JSON output format from native-vector-store
 * 
 * IMPORTANT: The current implementation only preserves the embedding in metadata_json,
 * not other metadata fields. This is by design for performance and memory efficiency.
 */

const { VectorStore } = require('../index.js');
const fs = require('fs');

// Create test data file
const testData = [
  {
    id: "product-001",
    text: "High-performance gaming laptop with RTX 4090 graphics",
    metadata: {
      embedding: [0.1, 0.2, 0.3],
      category: "electronics",  // Note: This won't be preserved in output
      price: 2499.99           // Note: This won't be preserved in output
    }
  },
  {
    id: "product-002",
    text: "Ergonomic office chair with lumbar support",
    metadata: {
      embedding: [0.7, 0.8, 0.9],
      category: "furniture"
    }
  },
  {
    id: "product-003",
    text: "Professional gaming desk with RGB lighting",
    metadata: {
      embedding: [0.6, 0.7, 0.8],
      category: "furniture"
    }
  }
];

// Write to temporary file
fs.writeFileSync('temp-products.json', JSON.stringify(testData, null, 2));

// Create store and load from file
const store = new VectorStore(3);
store.loadDir('.');  // Loads temp-products.json

// Search for furniture items (embedding similar to products 2 and 3)
const furnitureQuery = new Float32Array([0.65, 0.75, 0.85]);
const results = store.search(furnitureQuery, 3);

console.log("=== Actual VectorStore Output ===\n");
console.log("Search results as JSON:");
console.log(JSON.stringify(results, null, 2));

console.log("\n=== What Each Field Contains ===\n");
results.forEach((result, i) => {
  console.log(`Result ${i + 1}:`);
  console.log(`  score: ${result.score} (cosine similarity, 0-1 range)`);
  console.log(`  id: "${result.id}"`);
  console.log(`  text: "${result.text}"`);
  console.log(`  metadata_json: '${result.metadata_json}'`);
  
  // Parse metadata_json
  const metadata = JSON.parse(result.metadata_json);
  console.log(`  Parsed metadata:`, metadata);
  console.log(`    - Only contains 'embedding' field`);
  console.log(`    - Other metadata fields (category, price, etc.) are NOT stored\n`);
});

console.log("=== Important Notes ===\n");
console.log("1. metadata_json ONLY contains the embedding array, not other metadata fields");
console.log("2. This is by design for memory efficiency - embeddings are the only metadata needed for search");
console.log("3. If you need other metadata, store it separately and use the document ID to look it up");
console.log("4. The embedding values are preserved with 6 decimal places of precision");

console.log("\n=== Example Usage Pattern ===\n");
console.log(`// Separate metadata storage (e.g., in your application)
const productMetadata = {
  "product-001": { category: "electronics", price: 2499.99 },
  "product-002": { category: "furniture", price: 299.99 },
  "product-003": { category: "furniture", price: 199.99 }
};

// After search, look up full metadata
results.forEach(result => {
  const fullMetadata = productMetadata[result.id];
  console.log(\`\${result.id}: \${result.text} (\${fullMetadata.category})\`);
});`);

// Clean up
fs.unlinkSync('temp-products.json');

console.log("\n=== TypeScript Types ===\n");
console.log(`// What you get back from search()
interface SearchResult {
  score: number;         // 0-1, higher = more similar
  id: string;            // Your document ID
  text: string;          // Full document text (or content for Spring AI)
  metadata_json: string; // JSON string containing ONLY: {"embedding":[...]}
}

// Example parsing
const result: SearchResult = results[0];
const metadata = JSON.parse(result.metadata_json);
const embedding: number[] = metadata.embedding;`);