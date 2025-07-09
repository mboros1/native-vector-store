const { VectorStore } = require('../index');
const path = require('path');
const fs = require('fs');

async function debugLoad() {
    console.log('🔍 Debug Document Loading');
    console.log('========================\n');
    
    // Create vector store
    const store = new VectorStore(1536);
    
    // Test loading a single document manually
    const embeddedDir = path.join(__dirname, '..', 'out_embedded');
    const testFile = path.join(embeddedDir, 'ASSIGNMENT_TEMPLATE.json');
    
    console.log('📄 Reading test file:', testFile);
    const content = fs.readFileSync(testFile, 'utf8');
    const docs = JSON.parse(content);
    
    console.log(`📊 File contains ${docs.length} documents`);
    console.log(`📊 First doc has embedding: ${!!docs[0].metadata?.embedding}`);
    console.log(`📊 Embedding length: ${docs[0].metadata?.embedding?.length}`);
    
    // Try adding manually
    console.log('\n🔧 Adding first document manually...');
    store.addDocument(docs[0]);
    console.log(`✅ Store size after manual add: ${store.size()}`);
    
    // Normalize
    store.normalize();
    
    // Try search
    const query = new Float32Array(docs[0].metadata.embedding);
    const results = store.search(query, 1);
    console.log(`🔍 Search result: score=${results[0]?.score}`);
    
    // Now try loadDir
    console.log('\n📂 Testing loadDir...');
    const store2 = new VectorStore(1536);
    console.log('Loading from:', embeddedDir);
    
    store2.loadDir(embeddedDir);
    console.log(`✅ Store size after loadDir: ${store2.size()}`);
    
    // Check what files are being read
    const files = fs.readdirSync(embeddedDir).filter(f => f.endsWith('.json'));
    console.log(`\n📊 JSON files in directory: ${files.length}`);
    console.log('Files:', files.slice(0, 5).join(', '), '...');
}

debugLoad().catch(console.error);