#!/usr/bin/env node

const fs = require('fs');
const path = require('path');

// Configuration
const NUM_FILES = 100;
const DOCS_PER_FILE = 10;
const DIM = 1536; // OpenAI embedding dimension
const OUTPUT_DIR = path.join(__dirname, '../test_data');

// Create output directory
if (!fs.existsSync(OUTPUT_DIR)) {
    fs.mkdirSync(OUTPUT_DIR, { recursive: true });
}

console.log(`📁 Generating test data in ${OUTPUT_DIR}`);
console.log(`   Files: ${NUM_FILES}`);
console.log(`   Documents per file: ${DOCS_PER_FILE}`);
console.log(`   Total documents: ${NUM_FILES * DOCS_PER_FILE}`);
console.log(`   Embedding dimensions: ${DIM}`);

// Generate random embedding
function generateRandomEmbedding(dim) {
    const embedding = new Array(dim);
    let sum = 0;
    
    for (let i = 0; i < dim; i++) {
        embedding[i] = Math.random() * 2 - 1; // Random between -1 and 1
        sum += embedding[i] * embedding[i];
    }
    
    // Normalize
    const norm = Math.sqrt(sum);
    for (let i = 0; i < dim; i++) {
        embedding[i] /= norm;
    }
    
    return embedding;
}

// Generate files
let totalDocs = 0;
const startTime = Date.now();

for (let fileIdx = 0; fileIdx < NUM_FILES; fileIdx++) {
    const documents = [];
    
    for (let docIdx = 0; docIdx < DOCS_PER_FILE; docIdx++) {
        const globalIdx = fileIdx * DOCS_PER_FILE + docIdx;
        const doc = {
            id: `stress-doc-${globalIdx}`,
            text: `This is stress test document ${globalIdx}. It contains sample text for testing the vector store's performance under load. The document has index ${globalIdx} and is in file ${fileIdx}.`,
            metadata: {
                embedding: generateRandomEmbedding(DIM),
                file_index: fileIdx,
                doc_index: docIdx,
                timestamp: Date.now()
            }
        };
        documents.push(doc);
        totalDocs++;
    }
    
    // Write file
    const filename = path.join(OUTPUT_DIR, `stress_test_${fileIdx.toString().padStart(3, '0')}.json`);
    fs.writeFileSync(filename, JSON.stringify(documents, null, 2));
    
    if ((fileIdx + 1) % 10 === 0) {
        process.stdout.write(`\r   Progress: ${fileIdx + 1}/${NUM_FILES} files`);
    }
}

const elapsed = Date.now() - startTime;
console.log(`\n✅ Generated ${totalDocs} documents in ${NUM_FILES} files (${elapsed}ms)`);
console.log(`   Average: ${(elapsed / NUM_FILES).toFixed(1)}ms per file`);
console.log(`\n📝 To run stress tests:`);
console.log(`   cd src && make stress`);