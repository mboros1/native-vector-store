#!/usr/bin/env node

/**
 * Complete example showing ACTUAL JSON output from native-vector-store
 */

const { VectorStore } = require('../index.js');
const fs = require('fs');
const path = require('path');

// Create test directory
const testDir = './test-data';
if (!fs.existsSync(testDir)) {
  fs.mkdirSync(testDir);
}

// Create test documents with rich metadata
const documents = [
  {
    id: "user-guide-001",
    text: "Getting started with native-vector-store: Installation and setup",
    metadata: {
      embedding: [0.1, 0.2, 0.3],
      docType: "tutorial",
      section: "installation", 
      author: "Jane Doe",
      lastModified: "2024-12-01T10:00:00Z",
      tags: ["setup", "installation", "quickstart"],
      version: "1.0"
    }
  },
  {
    id: "api-ref-search", 
    text: "The search() method returns an array of SearchResult objects sorted by similarity score",
    metadata: {
      embedding: [0.4, 0.5, 0.6],
      docType: "api-reference",
      method: "search",
      parameters: ["query", "k", "normalizeQuery"],
      returnType: "SearchResult[]",
      since: "0.1.0"
    }
  },
  {
    id: "perf-guide-001",
    text: "Optimizing search performance with proper embedding normalization",
    metadata: {
      embedding: [0.35, 0.45, 0.55],
      docType: "guide",
      topic: "performance",
      readingTime: "5 min",
      difficulty: "intermediate",
      relatedDocs: ["api-ref-search", "user-guide-001"]
    }
  }
];

// Write to JSON file
fs.writeFileSync(path.join(testDir, 'docs.json'), JSON.stringify(documents, null, 2));

// Load and search
const store = new VectorStore(3);
store.loadDir(testDir);

// Search for API documentation (embedding similar to api-ref-search)
const apiQuery = new Float32Array([0.38, 0.48, 0.58]);
const results = store.search(apiQuery, 3);

console.log("=== Example JSON Output from VectorStore ===\n");

// 1. Raw output exactly as returned
console.log("1. Raw JavaScript object returned by search():");
console.log(results);

// 2. JSON stringified output
console.log("\n2. JSON.stringify() output:");
console.log(JSON.stringify(results, null, 2));

// 3. What each field contains
console.log("\n3. Detailed field breakdown:");
results.forEach((result, i) => {
  console.log(`\nResult ${i + 1}:`);
  console.log(`├─ score: ${result.score} (number, 0-1 range)`);
  console.log(`├─ id: "${result.id}" (string)`);
  console.log(`├─ text: "${result.text.substring(0, 50)}..." (string)`);
  console.log(`└─ metadata_json: (string containing JSON)`);
  
  // Parse and show the metadata
  const metadata = JSON.parse(result.metadata_json);
  console.log(`   └─ Parsed content:`, JSON.stringify(metadata, null, 2).split('\n').join('\n      '));
});

// 4. Common usage patterns
console.log("\n=== Common Usage Patterns ===\n");

// Pattern 1: Extract specific metadata fields
console.log("1. Extract document types:");
const docTypes = results.map(r => JSON.parse(r.metadata_json).docType);
console.log(`   ${JSON.stringify(docTypes)}`);

// Pattern 2: Filter by metadata
console.log("\n2. Filter for guides only:");
const guides = results.filter(r => {
  const meta = JSON.parse(r.metadata_json);
  return meta.docType === 'guide';
});
guides.forEach(g => console.log(`   - ${g.id}: ${g.text.substring(0, 40)}...`));

// Pattern 3: Create enriched objects
console.log("\n3. Create enriched result objects:");
const enriched = results.slice(0, 1).map(r => ({
  ...r,
  metadata: JSON.parse(r.metadata_json),
  preview: r.text.substring(0, 50) + '...'
}));
console.log(JSON.stringify(enriched[0], null, 2));

// 5. TypeScript interface
console.log("\n=== TypeScript Interface ===\n");
console.log(`interface SearchResult {
  score: number;         // Cosine similarity score (0-1)
  id: string;            // Document ID from your input
  text: string;          // Full text (or 'content' for Spring AI)
  metadata_json: string; // JSON string with ALL your metadata fields
}

// Example type-safe usage:
function processResults(results: SearchResult[]): void {
  results.forEach(result => {
    const metadata = JSON.parse(result.metadata_json);
    // metadata contains ALL fields from your original metadata object
  });
}`);

// Clean up
fs.rmSync(testDir, { recursive: true });

console.log("\n=== Key Points ===\n");
console.log("• metadata_json preserves ALL fields from your original metadata object");
console.log("• The embedding array is stored with 6 decimal places of precision");
console.log("• Additional metadata fields (author, tags, etc.) are fully preserved");
console.log("• The metadata_json is a string that needs to be parsed with JSON.parse()");
console.log("• Search results are always sorted by score (highest first)");