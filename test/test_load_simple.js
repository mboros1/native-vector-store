const { VectorStore } = require('../index');
const fs = require('fs');
const path = require('path');

console.log('🧪 Testing Simple Load');
console.log('======================\n');

try {
    // Create a temporary directory
    const testDir = path.join(__dirname, 'test_load_dir');
    if (!fs.existsSync(testDir)) {
        fs.mkdirSync(testDir);
    }
    
    // Create a simple test document
    const docs = [{
        id: 'test1',
        text: 'Test document',
        metadata: {
            embedding: [0.1, 0.2, 0.3]
        }
    }];
    
    // Write to file
    const testFile = path.join(testDir, 'test.json');
    fs.writeFileSync(testFile, JSON.stringify(docs, null, 2));
    console.log('📄 Created test file:', testFile);
    console.log('Content:', JSON.stringify(docs, null, 2));
    
    // Create store and load
    console.log('\n🔧 Loading with VectorStore...');
    const store = new VectorStore(3);
    
    store.loadDir(testDir);
    console.log(`✅ Store size after loadDir: ${store.size()}`);
    
    if (store.size() > 0) {
        // Test search
        console.log('\n🔍 Testing search...');
        const query = new Float32Array([0.1, 0.2, 0.3]);
        const results = store.search(query, 1);
        console.log('Results:', results);
    }
    
    // Clean up
    fs.unlinkSync(testFile);
    fs.rmdirSync(testDir);
    console.log('\n🧹 Cleaned up test files');
    
} catch (error) {
    console.error('❌ Error:', error);
    console.error('Stack:', error.stack);
}