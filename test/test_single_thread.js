const { VectorStore } = require('../index');

console.log('🧪 Testing AddDocument directly');
console.log('===============================\n');

try {
    // Create store
    const store = new VectorStore(3);
    console.log('✅ Created VectorStore');
    
    // Add document directly
    const doc = {
        id: 'test1',
        text: 'Test document',
        metadata: {
            embedding: [0.1, 0.2, 0.3]
        }
    };
    
    console.log('📄 Adding document:', JSON.stringify(doc));
    store.addDocument(doc);
    
    console.log(`📊 Store size: ${store.size()}`);
    
    // Also test normalize
    console.log('\n🔄 Normalizing...');
    store.normalize();
    console.log('✅ Normalized');
    
    console.log(`📊 Store size after normalize: ${store.size()}`);
    
} catch (error) {
    console.error('❌ Error:', error);
    console.error('Stack:', error.stack);
}