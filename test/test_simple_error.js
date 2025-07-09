const { VectorStore } = require('../index');

console.log('🧪 Testing Simple Error Cases');
console.log('=============================\n');

try {
    const store = new VectorStore(3);
    
    // Test: Wrong embedding dimension
    console.log('📄 Testing wrong embedding dimension...');
    try {
        const badDoc = {
            id: 'test1',
            text: 'Test document',
            metadata: {
                embedding: [0.1, 0.2] // Only 2 values, expecting 3
            }
        };
        store.addDocument(badDoc);
        console.log('❌ Should have thrown an error');
    } catch (error) {
        console.log('✅ Correctly caught error:', error.message);
    }
    
    console.log(`\nStore size after error: ${store.size()}`);
    
    // Test: Valid document
    console.log('\n📄 Testing valid document...');
    try {
        const goodDoc = {
            id: 'test2',
            text: 'Valid document',
            metadata: {
                embedding: [0.1, 0.2, 0.3]
            }
        };
        store.addDocument(goodDoc);
        console.log('✅ Valid document added successfully');
    } catch (error) {
        console.log('❌ Unexpected error:', error.message);
    }
    
    console.log(`Final store size: ${store.size()}`);
    
} catch (error) {
    console.error('❌ Fatal error:', error);
}