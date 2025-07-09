const fs = require('fs');
const path = require('path');

// Create benchmark data directory
const dataDir = path.join(__dirname, 'benchmark_data');
if (!fs.existsSync(dataDir)) {
    fs.mkdirSync(dataDir, { recursive: true });
}

// Number of files and documents per file
const NUM_FILES = 100;
const DOCS_PER_FILE = 100;
const EMBEDDING_DIM = 20;

console.log(`Creating ${NUM_FILES} files with ${DOCS_PER_FILE} documents each...`);

for (let fileIdx = 0; fileIdx < NUM_FILES; fileIdx++) {
    const documents = [];
    
    for (let docIdx = 0; docIdx < DOCS_PER_FILE; docIdx++) {
        const globalIdx = fileIdx * DOCS_PER_FILE + docIdx;
        const embedding = Array(EMBEDDING_DIM).fill(0).map(() => Math.random());
        
        documents.push({
            id: `doc-${globalIdx}`,
            text: `This is test document number ${globalIdx}. It contains some sample text for testing the vector store performance with parallel loading.`,
            metadata: {
                embedding: embedding,
                category: `category-${fileIdx % 10}`,
                timestamp: Date.now()
            }
        });
    }
    
    const filePath = path.join(dataDir, `batch_${fileIdx.toString().padStart(3, '0')}.json`);
    fs.writeFileSync(filePath, JSON.stringify(documents, null, 2));
    
    if ((fileIdx + 1) % 10 === 0) {
        console.log(`Created ${fileIdx + 1} files...`);
    }
}

console.log(`✅ Created ${NUM_FILES} files with ${NUM_FILES * DOCS_PER_FILE} total documents`);