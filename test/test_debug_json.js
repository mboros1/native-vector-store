const { VectorStore } = require('../index');

console.log('🧪 Testing JSON Generation');
console.log('==========================\n');

try {
    // Create a simple test document
    const doc = {
        id: 'test1',
        text: 'Test document',
        metadata: {
            embedding: [0.1, 0.2, 0.3]
        }
    };
    
    // Manually create the JSON string like the C++ code does
    let json_str = "{";
    json_str += '"id":"' + doc.id + '",';
    json_str += '"text":"' + doc.text + '",';
    json_str += '"metadata":{"embedding":[';
    
    for (let i = 0; i < doc.metadata.embedding.length; i++) {
        if (i > 0) json_str += ",";
        json_str += doc.metadata.embedding[i].toString();
    }
    json_str += "]}}";
    
    console.log('Generated JSON:', json_str);
    console.log('\nParsed back:', JSON.parse(json_str));
    
    // Now test with actual store
    console.log('\n🔧 Testing with VectorStore...');
    const store = new VectorStore(3);
    
    // Try loading the JSON directly
    console.log('Creating test file...');
    const fs = require('fs');
    fs.writeFileSync('test_doc.json', '[' + json_str + ']');
    
    store.loadFile('test_doc.json');
    console.log(`Store size after loadFile: ${store.size()}`);
    
    // Clean up
    fs.unlinkSync('test_doc.json');
    
} catch (error) {
    console.error('❌ Error:', error);
    console.error('Stack:', error.stack);
}