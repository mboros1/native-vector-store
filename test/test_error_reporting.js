const { VectorStore } = require('../index');

console.log('🧪 Testing Error Reporting');
console.log('==========================\n');

async function testErrors() {
    try {
        const store = new VectorStore(3);
        
        // Test 1: Wrong embedding dimension
        console.log('📄 Test 1: Wrong embedding dimension');
        try {
            const badDoc = {
                id: 'test1',
                text: 'Test document',
                metadata: {
                    embedding: [0.1, 0.2] // Only 2 values, expecting 3
                }
            };
            store.addDocument(badDoc);
            console.log('❌ Should have thrown an error for wrong dimension');
        } catch (error) {
            console.log('✅ Correctly caught error:', error.message);
        }
        
        // Test 2: Missing embedding field
        console.log('\n📄 Test 2: Missing embedding field');
        try {
            const noEmbDoc = {
                id: 'test2',
                text: 'Test document',
                metadata: {
                    // No embedding field
                }
            };
            store.addDocument(noEmbDoc);
            console.log('❌ Should have thrown an error for missing embedding');
        } catch (error) {
            console.log('✅ Correctly caught error:', error.message);
        }
        
        // Test 3: Invalid embedding data
        console.log('\n📄 Test 3: Invalid embedding data');
        try {
            const invalidEmbDoc = {
                id: 'test3',
                text: 'Test document',
                metadata: {
                    embedding: [0.1, "invalid", 0.3] // String instead of number
                }
            };
            store.addDocument(invalidEmbDoc);
            console.log('❌ Should have thrown an error for invalid data');
        } catch (error) {
            console.log('✅ Correctly caught error:', error.message);
        }
        
        // Test 4: Valid document should work
        console.log('\n📄 Test 4: Valid document');
        try {
            const goodDoc = {
                id: 'test4',
                text: 'Valid test document',
                metadata: {
                    embedding: [0.1, 0.2, 0.3]
                }
            };
            store.addDocument(goodDoc);
            console.log(`✅ Valid document added successfully. Store size: ${store.size()}`);
        } catch (error) {
            console.log('❌ Valid document should not throw error:', error.message);
        }
        
    } catch (error) {
        console.error('❌ Unexpected error:', error);
    }
}

testErrors();