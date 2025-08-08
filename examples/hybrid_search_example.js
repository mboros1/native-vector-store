#!/usr/bin/env node

/**
 * Example demonstrating hybrid search capabilities
 * Combines vector similarity (semantic) with BM25 text search (lexical)
 */

const { VectorStore } = require('../build/Release/vector_store');
const fs = require('fs');
const path = require('path');

// Sample documents about programming languages
const documents = [
  {
    id: "python",
    text: "Python is a high-level, interpreted programming language known for its simplicity and readability. It supports multiple programming paradigms including procedural, object-oriented, and functional programming.",
    metadata: {
      embedding: [0.8, 0.2, 0.1, 0.6, 0.3, 0.9, 0.4, 0.7]
    }
  },
  {
    id: "javascript",
    text: "JavaScript is a dynamic programming language primarily used for web development. It runs in browsers and Node.js, enabling both client-side and server-side programming.",
    metadata: {
      embedding: [0.6, 0.8, 0.3, 0.5, 0.7, 0.2, 0.9, 0.4]
    }
  },
  {
    id: "rust",
    text: "Rust is a systems programming language focused on safety, speed, and concurrency. It provides memory safety without using garbage collection through its ownership system.",
    metadata: {
      embedding: [0.3, 0.5, 0.9, 0.2, 0.8, 0.4, 0.1, 0.6]
    }
  },
  {
    id: "go",
    text: "Go is a statically typed, compiled programming language designed for simplicity and efficiency. It features built-in concurrency support through goroutines and channels.",
    metadata: {
      embedding: [0.4, 0.6, 0.7, 0.3, 0.5, 0.8, 0.2, 0.9]
    }
  },
  {
    id: "java",
    text: "Java is a class-based, object-oriented programming language designed to have minimal implementation dependencies. It follows the write once, run anywhere principle through the JVM.",
    metadata: {
      embedding: [0.5, 0.3, 0.8, 0.4, 0.6, 0.1, 0.7, 0.2]
    }
  }
];

console.log("🔍 Hybrid Search Example");
console.log("========================\n");

// Initialize vector store
const store = new VectorStore(8);  // 8-dimensional embeddings

// Add documents
console.log("📚 Loading documents...");
documents.forEach(doc => store.addDocument(doc));
store.finalize();
console.log(`✅ Loaded ${store.size()} documents\n`);

// Example 1: Pure vector search
console.log("1️⃣  Pure Vector Search");
console.log("   Query: Looking for languages similar to Python's embedding");
const pythonLikeEmbedding = new Float32Array([0.75, 0.25, 0.15, 0.55, 0.35, 0.85, 0.45, 0.65]);
const vectorResults = store.searchVector(pythonLikeEmbedding, 3);
console.log("   Results:");
vectorResults.forEach((result, i) => {
  console.log(`   ${i + 1}. ${result.id} (similarity: ${result.score.toFixed(4)})`);
});

// Example 2: Pure BM25 text search
console.log("\n2️⃣  Pure BM25 Text Search");
console.log("   Query: 'memory safety garbage collection'");
const bm25Results = store.searchBM25("memory safety garbage collection", 3);
console.log("   Results:");
bm25Results.forEach((result, i) => {
  console.log(`   ${i + 1}. ${result.id} (BM25 score: ${result.score.toFixed(4)})`);
});

// Example 3: Hybrid search (default mode)
console.log("\n3️⃣  Hybrid Search (Default)");
console.log("   Query: Vector similar to Python + text 'web development browser'");
const hybridResults = store.search(pythonLikeEmbedding, 3, "web development browser");
console.log("   Results:");
hybridResults.forEach((result, i) => {
  console.log(`   ${i + 1}. ${result.id} (combined score: ${result.score.toFixed(4)})`);
});

// Example 4: Hybrid with custom weights
console.log("\n4️⃣  Hybrid Search with Custom Weights");
console.log("   Query: Same as above but 70% text weight, 30% vector weight");
const customHybridResults = store.searchHybrid(
  pythonLikeEmbedding, 
  "web development browser", 
  3,
  0.3,  // vector weight
  0.7   // BM25 weight
);
console.log("   Results:");
customHybridResults.forEach((result, i) => {
  console.log(`   ${i + 1}. ${result.id} (weighted score: ${result.score.toFixed(4)})`);
});

// Example 5: Tuning BM25 parameters
console.log("\n5️⃣  BM25 with Custom Parameters");
console.log("   Setting k1=2.0, b=0.5 for less normalization");
store.setBM25Parameters(2.0, 0.5);
const tunedBM25Results = store.searchBM25("programming language", 3);
console.log("   Results:");
tunedBM25Results.forEach((result, i) => {
  console.log(`   ${i + 1}. ${result.id} (tuned BM25: ${result.score.toFixed(4)})`);
});

console.log("\n✨ Hybrid search combines the best of both worlds:");
console.log("   - Vector search captures semantic similarity");
console.log("   - BM25 captures exact term matches");
console.log("   - Together they provide more relevant results!");