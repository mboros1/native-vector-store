const { VectorStore } = require('../index');
const path = require('path');
const fs = require('fs');

async function testNormalization() {
    console.log('🧪 Testing Vector Normalization');
    console.log('================================\n');
    
    // Create vector store
    const store = new VectorStore(3); // Small dimension for easy testing
    
    // Add test document with known embedding
    const testDoc = {
        id: 'test1',
        text: 'Test document',
        metadata: {
            embedding: [3, 4, 0] // Will have magnitude 5 after normalization -> [0.6, 0.8, 0]
        }
    };
    
    console.log('📄 Adding test document with embedding:', testDoc.metadata.embedding);
    store.addDocument(testDoc);
    
    // Normalize
    console.log('🔄 Normalizing embeddings...');
    store.normalize();
    
    // Search with same embedding (should get score 1.0)
    console.log('\n🔍 Searching with same embedding...');
    const query1 = new Float32Array([3, 4, 0]);
    const results1 = store.search(query1, 1);
    console.log('Result:', results1[0]);
    console.log('Expected score: 1.0');
    console.log('Actual score:', results1[0].score);
    
    // Search with orthogonal embedding (should get score 0.0)
    console.log('\n🔍 Searching with orthogonal embedding...');
    const query2 = new Float32Array([0, 0, 1]);
    const results2 = store.search(query2, 1);
    console.log('Expected score: 0.0');
    console.log('Actual score:', results2[0].score);
    
    // Now test with actual embedded documents
    console.log('\n\n📊 Testing with Real Embedded Documents');
    console.log('========================================\n');
    
    const store2 = new VectorStore(1536);
    const embeddedDir = path.join(__dirname, '..', 'out_embedded');
    
    // Load just one file for testing
    const firstFile = fs.readdirSync(embeddedDir).find(f => f.endsWith('.json'));
    const docs = JSON.parse(fs.readFileSync(path.join(embeddedDir, firstFile), 'utf8'));
    
    console.log(`📄 Loading ${docs.length} documents from ${firstFile}`);
    
    // Check if embeddings exist and are valid
    let validDocs = 0;
    for (const doc of docs) {
        if (doc.metadata && doc.metadata.embedding && Array.isArray(doc.metadata.embedding)) {
            // Check for NaN or invalid values
            const hasNaN = doc.metadata.embedding.some(v => isNaN(v));
            const allZero = doc.metadata.embedding.every(v => v === 0);
            
            if (!hasNaN && !allZero) {
                validDocs++;
            } else {
                console.log(`⚠️  Document ${doc.id} has invalid embedding (NaN: ${hasNaN}, AllZero: ${allZero})`);
            }
        }
    }
    
    console.log(`✅ Found ${validDocs} documents with valid embeddings`);
    
    // Load the file
    store2.loadFile(path.join(embeddedDir, firstFile));
    console.log(`📊 Loaded ${store2.size()} documents into store`);
    
    // Use first document's embedding as query
    if (docs[0] && docs[0].metadata && docs[0].metadata.embedding) {
        const queryEmb = new Float32Array(docs[0].metadata.embedding);
        
        // Check query embedding magnitude
        let mag = 0;
        for (let i = 0; i < queryEmb.length; i++) {
            mag += queryEmb[i] * queryEmb[i];
        }
        console.log(`\n🔍 Query embedding magnitude: ${Math.sqrt(mag)}`);
        
        const results = store2.search(queryEmb, 5);
        console.log(`\n📊 Search results:`);
        results.forEach((r, i) => {
            console.log(`${i + 1}. Score: ${r.score}, ID: ${r.id}`);
        });
    }
}

testNormalization().catch(console.error);