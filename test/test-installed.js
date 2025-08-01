// Test script for installed package
// This is a simplified version that tests the core functionality

const { VectorStore } = require('native-vector-store');

function generateRandomEmbedding(dim) {
  const embedding = new Float32Array(dim);
  let sum = 0;
  for (let i = 0; i < dim; i++) {
    embedding[i] = Math.random() - 0.5;
    sum += embedding[i] * embedding[i];
  }
  // Normalize
  const norm = Math.sqrt(sum);
  for (let i = 0; i < dim; i++) {
    embedding[i] /= norm;
  }
  return embedding;
}

function createTestDocument(id, text, dim) {
  return {
    id: id,
    text: text,
    metadata: {
      embedding: Array.from(generateRandomEmbedding(dim)),
      timestamp: Date.now()
    }
  };
}

console.log('🚀 Native Vector Store Installation Test');
console.log('=======================================\n');

try {
  // Test 1: Basic functionality
  console.log('🧪 Test 1: Basic operations');
  const dim = 128; // Smaller dimension for quick testing
  const store = new VectorStore(dim);
  
  // Add a few documents
  for (let i = 0; i < 10; i++) {
    const doc = createTestDocument(
      `test-${i}`,
      `Test document ${i}`,
      dim
    );
    store.addDocument(doc);
  }
  
  console.log('✅ Added 10 documents');
  
  // Finalize
  store.finalize();
  console.log('✅ Store finalized');
  
  // Search
  const query = generateRandomEmbedding(dim);
  const results = store.search(query, 5);
  console.log(`✅ Search returned ${results.length} results`);
  
  // Test 2: Verify results structure
  console.log('\n🧪 Test 2: Result validation');
  if (results.length > 0) {
    const firstResult = results[0];
    if (firstResult.id && firstResult.text && typeof firstResult.score === 'number') {
      console.log('✅ Result structure is valid');
      console.log(`   First result: ${firstResult.id} (score: ${firstResult.score.toFixed(4)})`);
    } else {
      throw new Error('Invalid result structure');
    }
  }
  
  // Test 3: Performance check
  console.log('\n🧪 Test 3: Performance check');
  const perfStore = new VectorStore(1536); // OpenAI dimension
  
  // Add more documents
  const numDocs = 1000;
  const startAdd = Date.now();
  for (let i = 0; i < numDocs; i++) {
    const doc = createTestDocument(`perf-${i}`, `Performance test ${i}`, 1536);
    perfStore.addDocument(doc);
  }
  const addTime = Date.now() - startAdd;
  console.log(`✅ Added ${numDocs} documents in ${addTime}ms`);
  
  perfStore.finalize();
  
  // Search performance
  const perfQuery = generateRandomEmbedding(1536);
  const startSearch = Date.now();
  const perfResults = perfStore.search(perfQuery, 10);
  const searchTime = Date.now() - startSearch;
  console.log(`✅ Search completed in ${searchTime}ms`);
  
  console.log('\n🎉 All tests passed! The installed package works correctly.\n');
  process.exit(0);
  
} catch (error) {
  console.error('\n❌ Test failed:', error.message);
  console.error(error.stack);
  process.exit(1);
}