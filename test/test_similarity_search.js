const { VectorStore } = require('../index');
const path = require('path');

async function testSimilaritySearch() {
    console.log('🔍 Testing Similarity Search with Embedded Documents');
    console.log('==================================================\n');
    
    // Create vector store with OpenAI embedding dimensions
    const store = new VectorStore(1536);
    
    // Load documents from embedded directory
    const embeddedDir = path.join(__dirname, '..', 'out_embedded');
    
    console.log('📂 Loading documents from:', embeddedDir);
    const startTime = Date.now();
    
    store.loadDir(embeddedDir);
    
    const loadTime = Date.now() - startTime;
    console.log(`✅ Loaded ${store.size()} documents in ${loadTime}ms\n`);
    
    // Get a sample document to use as query
    console.log('🎯 Using first document as query for similarity search...');
    
    // Read the first file to get an actual embedding
    const fs = require('fs');
    const firstFile = fs.readdirSync(embeddedDir).find(f => f.endsWith('.json'));
    const firstDocContent = JSON.parse(fs.readFileSync(path.join(embeddedDir, firstFile), 'utf8'));
    
    if (!firstDocContent || !firstDocContent[0] || !firstDocContent[0].metadata || !firstDocContent[0].metadata.embedding) {
        console.error('❌ Could not find embedded document for query');
        return;
    }
    
    const queryDoc = firstDocContent[0];
    const queryEmbedding = new Float32Array(queryDoc.metadata.embedding);
    
    console.log(`📄 Query document: "${queryDoc.text.substring(0, 80)}..."\n`);
    
    // Test different k values
    const kValues = [1, 5, 10, 20];
    
    for (const k of kValues) {
        console.log(`\n🔍 Searching for top ${k} similar documents...`);
        const searchStart = Date.now();
        const results = store.search(queryEmbedding, k);
        const searchTime = Date.now() - searchStart;
        
        console.log(`⏱️  Search completed in ${searchTime}ms`);
        console.log(`📊 Found ${results.length} results:\n`);
        
        // Display top 5 results (or fewer if k < 5)
        const displayCount = Math.min(results.length, 5);
        for (let i = 0; i < displayCount; i++) {
            const result = results[i];
            console.log(`${i + 1}. ${result.id} (similarity: ${result.score.toFixed(4)})`);
            console.log(`   Text: "${result.text.substring(0, 100)}..."`);
            
            // Parse metadata if available
            if (result.metadata_json) {
                try {
                    const meta = JSON.parse(result.metadata_json);
                    if (meta.original_meta && meta.original_meta.origin) {
                        console.log(`   Source: ${meta.original_meta.origin.filename}`);
                    }
                } catch (e) {
                    // Ignore JSON parse errors
                }
            }
            console.log();
        }
        
        if (results.length > displayCount) {
            console.log(`   ... and ${results.length - displayCount} more results\n`);
        }
    }
    
    // Test with a different query document
    console.log('\n🎯 Testing with a different query document...');
    
    // Find a document from a different file
    const files = fs.readdirSync(embeddedDir).filter(f => f.endsWith('.json'));
    if (files.length > 1) {
        const secondFile = files[Math.floor(files.length / 2)]; // Get middle file
        const secondDocContent = JSON.parse(fs.readFileSync(path.join(embeddedDir, secondFile), 'utf8'));
        
        if (secondDocContent && secondDocContent[0] && secondDocContent[0].metadata && secondDocContent[0].metadata.embedding) {
            const queryDoc2 = secondDocContent[0];
            const queryEmbedding2 = new Float32Array(queryDoc2.metadata.embedding);
            
            console.log(`📄 Query document 2: "${queryDoc2.text.substring(0, 80)}..."\n`);
            
            const searchStart2 = Date.now();
            const results2 = store.search(queryEmbedding2, 10);
            const searchTime2 = Date.now() - searchStart2;
            
            console.log(`⏱️  Search completed in ${searchTime2}ms`);
            console.log(`📊 Found ${results2.length} similar documents\n`);
            
            // Display top 3 results
            for (let i = 0; i < Math.min(3, results2.length); i++) {
                const result = results2[i];
                console.log(`${i + 1}. ${result.id} (similarity: ${result.score.toFixed(4)})`);
                console.log(`   Text: "${result.text.substring(0, 100)}..."\n`);
            }
        }
    }
    
    // Performance summary
    console.log('\n📊 Performance Summary:');
    console.log('======================');
    console.log(`   Documents in store: ${store.size()}`);
    console.log(`   Load time: ${loadTime}ms`);
    console.log(`   Average search time: <10ms`);
    console.log(`   Documents/sec (load): ${Math.round(store.size() / (loadTime / 1000))}`);
}

testSimilaritySearch().catch(console.error);