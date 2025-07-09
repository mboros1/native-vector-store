const { VectorStore } = require('../index');
const fs = require('fs');
const path = require('path');

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

function performanceTest() {
  console.log('🚀 Starting performance tests...');
  
  const dim = 1536; // OpenAI embedding dimension
  const store = new VectorStore(dim);
  
  // Test 1: Large-scale document loading
  console.log('📚 Test 1: Large-scale document loading');
  const startLoad = Date.now();
  
  const numDocs = 10000;
  for (let i = 0; i < numDocs; i++) {
    const doc = createTestDocument(
      `doc-${i}`,
      `This is test document number ${i} with some content.`,
      dim
    );
    store.addDocument(doc);
  }
  
  const loadTime = Date.now() - startLoad;
  console.log(`✅ Loaded ${numDocs} documents in ${loadTime}ms (${(loadTime/numDocs).toFixed(2)}ms per doc)`);
  
  // Test 2: Search performance
  console.log('🔍 Test 2: Search performance');
  const query = generateRandomEmbedding(dim);
  const k = 10;
  
  const startSearch = Date.now();
  const results = store.search(query, k);
  const searchTime = Date.now() - startSearch;
  
  console.log(`✅ Found ${results.length} results in ${searchTime}ms`);
  console.log(`   Top result: ${results[0].id} (score: ${results[0].score.toFixed(4)})`);
  
  // Test 3: Normalization performance
  console.log('🧮 Test 3: Normalization performance');
  const startNorm = Date.now();
  store.normalize();
  const normTime = Date.now() - startNorm;
  
  console.log(`✅ Normalized ${store.size()} documents in ${normTime}ms`);
  
  // Performance summary
  console.log('\n📊 Performance Summary:');
  console.log(`   Load time: ${loadTime}ms (target: <1000ms for 100k docs)`);
  console.log(`   Search time: ${searchTime}ms (target: <10ms)`);
  console.log(`   Normalization time: ${normTime}ms`);
  console.log(`   Total documents: ${store.size()}`);
  
  // Verify performance targets
  const loadPassed = loadTime < 1000; // Should be much faster for 10k docs
  const searchPassed = searchTime < 10;
  
  console.log(`\n🎯 Performance Targets:`);
  console.log(`   Load performance: ${loadPassed ? '✅' : '❌'} (${loadTime}ms)`);
  console.log(`   Search performance: ${searchPassed ? '✅' : '❌'} (${searchTime}ms)`);
  
  return loadPassed && searchPassed;
}

function functionalTest() {
  console.log('🧪 Starting functional tests...');
  
  const dim = 384; // Smaller dimension for testing
  const store = new VectorStore(dim);
  
  // Test 1: Basic document operations
  console.log('📄 Test 1: Basic document operations');
  const doc1 = createTestDocument('test-1', 'Hello world', dim);
  const doc2 = createTestDocument('test-2', 'Goodbye world', dim);
  
  store.addDocument(doc1);
  store.addDocument(doc2);
  
  console.log(`✅ Added 2 documents, store size: ${store.size()}`);
  
  // Test 2: Search functionality
  console.log('🔍 Test 2: Search functionality');
  const query = generateRandomEmbedding(dim);
  const results = store.search(query, 2);
  
  console.log(`✅ Search returned ${results.length} results`);
  console.log(`   Result 1: ${results[0].id} (score: ${results[0].score.toFixed(4)})`);
  console.log(`   Result 2: ${results[1].id} (score: ${results[1].score.toFixed(4)})`);
  
  // Test 3: Data integrity
  console.log('🔒 Test 3: Data integrity');
  const result = results[0];
  const isValidScore = typeof result.score === 'number' && !isNaN(result.score);
  const hasId = typeof result.id === 'string' && result.id.length > 0;
  const hasText = typeof result.text === 'string' && result.text.length > 0;
  
  console.log(`✅ Data integrity check:`);
  console.log(`   Valid score: ${isValidScore}`);
  console.log(`   Has ID: ${hasId}`);
  console.log(`   Has text: ${hasText}`);
  
  return isValidScore && hasId && hasText;
}

async function main() {
  console.log('🚀 Native Vector Store Test Suite');
  console.log('================================\n');
  
  try {
    // Run functional tests
    const functionalPassed = functionalTest();
    console.log('');
    
    // Run performance tests
    const performancePassed = performanceTest();
    console.log('');
    
    // Summary
    console.log('📋 Test Summary:');
    console.log(`   Functional tests: ${functionalPassed ? '✅ PASSED' : '❌ FAILED'}`);
    console.log(`   Performance tests: ${performancePassed ? '✅ PASSED' : '❌ FAILED'}`);
    
    if (functionalPassed && performancePassed) {
      console.log('\n🎉 All tests passed! Vector store is ready for production.');
      process.exit(0);
    } else {
      console.log('\n❌ Some tests failed. Please check the implementation.');
      process.exit(1);
    }
    
  } catch (error) {
    console.error('💥 Test error:', error);
    process.exit(1);
  }
}

// Run tests if this file is executed directly
if (require.main === module) {
  main();
}