const { VectorStore } = require('../index');

console.log('🧪 Testing Simple Document Add');
console.log('==============================\n');

try {
    // Create vector store with small dimension for testing
    const store = new VectorStore(3);
    console.log('✅ Created VectorStore with dimension 3');
    
    // Create a simple test document
    const doc = {
        id: 'test1',
        text: 'Test document',
        metadata: {
            embedding: [0.1, 0.2, 0.3]
        }
    };
    
    console.log('📄 Test document:', JSON.stringify(doc, null, 2));
    
    console.log('\n🔧 Adding document...');
    store.addDocument(doc);
    console.log('✅ Document added successfully');
    
    console.log(`📊 Store size: ${store.size()}`);
    
    console.log('\n🔄 Normalizing...');
    store.normalize();
    console.log('✅ Normalized successfully');
    
    console.log('\n🔍 Searching...');
    const query = new Float32Array([0.1, 0.2, 0.3]);
    const results = store.search(query, 1);
    console.log('✅ Search completed');
    console.log('Results:', results);
    
} catch (error) {
    console.error('❌ Error:', error);
    console.error('Stack:', error.stack);
}