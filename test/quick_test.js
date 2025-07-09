const { VectorStore } = require('../index');

console.log('🧪 Quick Vector Store Test');
console.log('==========================');

try {
  const store = new VectorStore(20);
  console.log('✅ VectorStore created successfully');
  
  // Test adding a single document
  const doc = {
    id: 'test1',
    text: 'Hello world test document',
    metadata: {
      embedding: Array.from({length: 20}, (_, i) => i * 0.1),
      category: 'Test'
    }
  };
  
  console.log('📝 Adding document...');
  store.addDocument(doc);
  console.log('✅ Document added successfully');
  
  console.log(`📊 Store size: ${store.size()}`);
  
  // Test search
  console.log('🔍 Testing search...');
  const query = new Float32Array(doc.metadata.embedding);
  const results = store.search(query, 1);
  
  console.log(`✅ Search completed, found ${results.length} results`);
  if (results.length > 0) {
    console.log(`   Score: ${results[0].score.toFixed(4)}`);
    console.log(`   ID: ${results[0].id}`);
  }
  
  // Test loadDir with a simple directory
  console.log('\n📂 Testing loadDir...');
  
  try {
    store.loadDir('./test');
    console.log('✅ loadDir completed successfully');
    console.log(`📊 Store size after loadDir: ${store.size()}`);
  } catch (loadError) {
    console.error('❌ loadDir failed:', loadError.message);
  }
  
} catch (error) {
  console.error('❌ Error:', error.message);
  console.error('Stack:', error.stack);
}