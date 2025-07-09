const { VectorStore } = require('../index');
const path = require('path');

console.log('🧪 VS Code Ready Test - Vector Store');
console.log('====================================\n');

async function test() {
    try {
        // Create vector store
        const store = new VectorStore(1536);
        console.log('✅ Created VectorStore with dimension 1536');
        
        // Load embedded documents
        const embeddedDir = path.join(__dirname, '..', 'out_embedded');
        console.log(`📂 Loading from: ${embeddedDir}`);
        
        store.loadDir(embeddedDir);
        console.log(`✅ Loaded ${store.size()} documents`);
        
        if (store.size() > 0) {
            // Test with a simple random query
            const query = new Float32Array(1536);
            for (let i = 0; i < 1536; i++) {
                query[i] = Math.random() - 0.5;
            }
            
            const results = store.search(query, 5);
            console.log(`🔍 Search found ${results.length} results`);
            
            if (results.length > 0) {
                console.log('\n📊 Top result:');
                console.log(`   ID: ${results[0].id}`);
                console.log(`   Score: ${results[0].score}`);
                console.log(`   Text: ${results[0].text.substring(0, 100)}...`);
                
                console.log('\n✅ Vector store is working correctly!');
                console.log('💻 You can now debug in VS Code using:');
                console.log('   - Press F5 to debug the C++ test');
                console.log('   - Use Ctrl+Shift+P -> "Tasks: Run Task" to build');
            } else {
                console.log('❌ No search results returned');
            }
        } else {
            console.log('❌ No documents loaded');
        }
        
    } catch (error) {
        console.error('❌ Error:', error.message);
    }
}

test();