#!/usr/bin/env node

/**
 * This script validates and demonstrates the correct JSON format for native-vector-store documents
 */

const fs = require('fs');

// Example of CORRECT formats
const correctFormats = [
  {
    name: "Standard format (with 'text')",
    doc: {
      id: "doc-123",
      text: "This is the document content",
      metadata: {
        embedding: [0.1, 0.2, 0.3], // Array of numbers, dimensions must match VectorStore constructor
        // Other metadata fields are optional
        category: "example",
        source: "demo"
      }
    }
  },
  {
    name: "Spring AI format (with 'content')",
    doc: {
      id: "doc-456",
      content: "This is the document content", // 'content' instead of 'text'
      metadata: {
        embedding: [0.1, 0.2, 0.3],
        category: "spring-ai",
        source: "demo"
      }
    }
  }
];

// Common INCORRECT formats
const incorrectFormats = [
  {
    name: "Missing id",
    doc: {
      text: "Content",
      metadata: { embedding: [0.1, 0.2, 0.3] }
    }
  },
  {
    name: "Missing text",
    doc: {
      id: "doc-1",
      metadata: { embedding: [0.1, 0.2, 0.3] }
    }
  },
  {
    name: "Missing metadata",
    doc: {
      id: "doc-1",
      text: "Content",
      embedding: [0.1, 0.2, 0.3]  // Wrong: embedding should be inside metadata
    }
  },
  {
    name: "Embedding not in metadata",
    doc: {
      id: "doc-1",
      text: "Content",
      embedding: [0.1, 0.2, 0.3],  // Wrong location
      metadata: {}
    }
  },
  {
    name: "Wrong embedding type",
    doc: {
      id: "doc-1",
      text: "Content",
      metadata: {
        embedding: "0.1,0.2,0.3"  // Wrong: must be array of numbers
      }
    }
  }
];

function validateDocument(doc) {
  const errors = [];
  
  // Check required fields
  if (!doc.id) {
    errors.push("Missing required field 'id'");
  }
  if (!doc.text && !doc.content) {
    errors.push("Missing required field 'text' or 'content'");
  }
  if (!doc.metadata) {
    errors.push("Missing required field 'metadata'");
  } else {
    if (!doc.metadata.embedding) {
      errors.push("Missing required field 'embedding' inside 'metadata'");
    } else if (!Array.isArray(doc.metadata.embedding)) {
      errors.push("Field 'metadata.embedding' must be an array of numbers");
    } else if (doc.metadata.embedding.length === 0) {
      errors.push("Field 'metadata.embedding' cannot be empty");
    } else if (!doc.metadata.embedding.every(v => typeof v === 'number')) {
      errors.push("Field 'metadata.embedding' must contain only numbers");
    }
  }
  
  return errors;
}

console.log("=== Correct Format Examples ===");
correctFormats.forEach(({ name, doc }) => {
  console.log(`\n${name}:`);
  console.log(JSON.stringify(doc, null, 2));
  console.log("Validation:", validateDocument(doc));
});

console.log("\n=== Common Mistakes ===");
incorrectFormats.forEach(({ name, doc }) => {
  console.log(`\n${name}:`);
  console.log(JSON.stringify(doc, null, 2));
  console.log("Errors:", validateDocument(doc));
});

// If a file was provided, validate it
if (process.argv[2]) {
  console.log(`\n=== Validating File: ${process.argv[2]} ===`);
  try {
    const content = fs.readFileSync(process.argv[2], 'utf8');
    const data = JSON.parse(content);
    
    // Handle both single document and array of documents
    const documents = Array.isArray(data) ? data : [data];
    
    documents.forEach((doc, index) => {
      const errors = validateDocument(doc);
      if (errors.length > 0) {
        console.log(`Document ${index}: INVALID`);
        errors.forEach(e => console.log(`  - ${e}`));
      } else {
        console.log(`Document ${index}: VALID`);
      }
    });
  } catch (e) {
    console.error("Error reading/parsing file:", e.message);
  }
}

console.log("\n=== Usage ===");
console.log("To validate a JSON file: node validate-format.js <filename.json>");