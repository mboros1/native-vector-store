const { VectorStore } = require('../index');
const path = require('path');

async function testEmbeddedDocs() {
    console.log('🧪 Testing Vector Store with Embedded Documents');
    console.log('==============================================\n');
    
    // Create vector store with OpenAI embedding dimensions
    const store = new VectorStore(1536);
    
    // Load documents from embedded directory
    const embeddedDir = path.join(__dirname, '..', 'out_embedded');
    
    console.log('📂 Loading documents from:', embeddedDir);
    const startTime = Date.now();
    
    store.loadDir(embeddedDir);
    
    const loadTime = Date.now() - startTime;
    console.log(`✅ Loaded ${store.size()} documents in ${loadTime}ms\n`);
    
    // Test search with a query
    console.log('🔍 Testing search functionality...');
    
    // Create a random query vector
    const query = new Float32Array(1536);
    for (let i = 0; i < 1536; i++) {
        query[i] = Math.random() - 0.5;
    }
    
    const searchStart = Date.now();
    const results = store.search(query, 10);
    const searchTime = Date.now() - searchStart;
    
    console.log(`✅ Search completed in ${searchTime}ms`);
    console.log(`📊 Found ${results.length} results:\n`);
    
    // Display top results
    results.forEach((result, i) => {
        console.log(`${i + 1}. ${result.id} (score: ${result.score.toFixed(4)})`);
        console.log(`   Text: ${result.text.substring(0, 100)}...`);
        console.log(`   Metadata: ${result.metadata_json ? 'Present' : 'Missing'}\n`);
    });
    
    // Performance summary
    console.log('📊 Performance Summary:');
    console.log('======================');
    console.log(`   Documents loaded: ${store.size()}`);
    console.log(`   Load time: ${loadTime}ms`);
    console.log(`   Search time: ${searchTime}ms`);
    console.log(`   Documents/sec: ${Math.round(store.size() / (loadTime / 1000))}`);
}

testEmbeddedDocs().catch(console.error);