const fs = require('fs');
const path = require('path');

// Generate test documents with correct embedding dimensions
const NUM_DOCS = 1000;
const EMBEDDING_DIM = 128;

const testDir = path.join(__dirname, 'cache_test_data');
if (!fs.existsSync(testDir)) {
    fs.mkdirSync(testDir);
}

console.log(`Generating ${NUM_DOCS} test documents with ${EMBEDDING_DIM}-dimensional embeddings...`);

// Generate documents in batches (100 docs per file)
const BATCH_SIZE = 100;
const NUM_FILES = Math.ceil(NUM_DOCS / BATCH_SIZE);

for (let fileIdx = 0; fileIdx < NUM_FILES; fileIdx++) {
    const docs = [];
    const startIdx = fileIdx * BATCH_SIZE;
    const endIdx = Math.min(startIdx + BATCH_SIZE, NUM_DOCS);
    
    for (let i = startIdx; i < endIdx; i++) {
        // Generate a random embedding
        const embedding = [];
        for (let j = 0; j < EMBEDDING_DIM; j++) {
            embedding.push(Math.random() * 2 - 1);
        }
        
        // Generate text with keywords
        const keywords = ['machine', 'learning', 'neural', 'network', 'data', 'science', 'algorithm', 'model'];
        const selectedKeywords = keywords.filter(() => Math.random() > 0.5).join(' ');
        
        const doc = {
            id: `doc-${i}`,
            text: `This is document ${i}. Keywords: ${selectedKeywords}. Lorem ipsum dolor sit amet.`,
            metadata: {
                embedding: embedding,
                index: i,
                batch: fileIdx,
                timestamp: Date.now()
            }
        };
        
        docs.push(doc);
    }
    
    const filename = path.join(testDir, `batch_${fileIdx}.json`);
    fs.writeFileSync(filename, JSON.stringify(docs, null, 2));
    console.log(`Created ${filename} with ${docs.length} documents`);
}

console.log(`\nGenerated ${NUM_DOCS} documents in ${NUM_FILES} files`);
console.log(`Test data saved to: ${testDir}`);