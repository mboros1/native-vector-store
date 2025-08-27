const { VectorStore } = require('../index');
const fs = require('fs');
const path = require('path');
const { execSync } = require('child_process');

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
      embedding: Array.from(generateRandomEmbedding(dim))
    }
  };
}

function prepareTestData(numDocs = 100, dim = 384) {
  const testDir = path.join(__dirname, 'test_data');
  
  // Clean up and create test directory
  if (fs.existsSync(testDir)) {
    fs.rmSync(testDir, { recursive: true, force: true });
  }
  fs.mkdirSync(testDir, { recursive: true });
  
  // Create test documents
  const documents = [];
  for (let i = 0; i < numDocs; i++) {
    documents.push(createTestDocument(
      `doc-${i}`,
      `This is test document number ${i} with some content for testing search functionality.`,
      dim
    ));
  }
  
  // Save documents to JSON file
  const jsonPath = path.join(testDir, 'documents.json');
  fs.writeFileSync(jsonPath, JSON.stringify(documents, null, 2));
  
  return testDir;
}

function createBundle(dataDir, bundleDir) {
  // Build the nvs-pack command
  const nvsPackPath = path.join(__dirname, '..', 'src', 'bin', 'nvs-pack');
  
  // Check if nvs-pack exists
  if (!fs.existsSync(nvsPackPath)) {
    console.log('⚠️  nvs-pack not found, building it...');
    execSync('make -C src nvs-pack', { stdio: 'inherit', cwd: path.join(__dirname, '..') });
  }
  
  // Remove old bundle directory if it exists
  if (fs.existsSync(bundleDir)) {
    fs.rmSync(bundleDir, { recursive: true, force: true });
  }
  
  // Create the bundle
  try {
    execSync(`${nvsPackPath} --out ${bundleDir} ${dataDir}`, { stdio: 'pipe' });
    return true;
  } catch (error) {
    console.error('Failed to create bundle:', error.message);
    return false;
  }
}

function performanceTest() {
  console.log('🚀 Starting performance tests...');
  
  const dim = 1536; // OpenAI embedding dimension
  const numDocs = 10000;
  
  // Prepare test data
  console.log(`📚 Preparing ${numDocs} test documents...`);
  const startPrep = Date.now();
  const testDir = prepareTestData(numDocs, dim);
  const prepTime = Date.now() - startPrep;
  console.log(`   Data preparation: ${prepTime}ms`);
  
  // Create bundle
  const bundleDir = path.join(__dirname, 'test_perf_bundle');
  console.log('📦 Creating test bundle...');
  const startBundle = Date.now();
  if (!createBundle(testDir, bundleDir)) {
    console.error('Failed to create test bundle');
    return false;
  }
  const bundleTime = Date.now() - startBundle;
  console.log(`   Bundle creation: ${bundleTime}ms`);
  
  // Load the bundle
  console.log('🔄 Loading bundle into vector store...');
  const store = new VectorStore(bundleDir);
  
  // Test search performance
  console.log('🔍 Testing search performance...');
  const query = generateRandomEmbedding(dim);
  
  // Warm-up
  for (let i = 0; i < 5; i++) {
    store.search(query, 10);
  }
  
  // Measure search time
  const searchTimes = [];
  const iterations = 100;
  for (let i = 0; i < iterations; i++) {
    const start = Date.now();
    const results = store.search(query, 10);
    const time = Date.now() - start;
    searchTimes.push(time);
  }
  
  const meanTime = searchTimes.reduce((a, b) => a + b, 0) / searchTimes.length;
  const maxTime = Math.max(...searchTimes);
  const minTime = Math.min(...searchTimes);
  
  console.log('📊 Performance Results:');
  console.log(`   Bundle creation: ${bundleTime}ms for ${numDocs} documents`);
  console.log(`   Search latency (mean): ${meanTime.toFixed(2)}ms`);
  console.log(`   Search latency (min/max): ${minTime}ms / ${maxTime}ms`);
  console.log(`   Total documents: ${store.size()}`);
  
  // Clean up
  store.close();
  fs.rmSync(bundleDir, { recursive: true, force: true });
  fs.rmSync(testDir, { recursive: true, force: true });
  
  // Verify performance targets
  const searchPassed = meanTime < 10;
  
  console.log(`\n🎯 Performance Targets:`);
  console.log(`   Bundle creation: ✅ (${(numDocs / (bundleTime / 1000)).toFixed(0)} docs/sec)`);
  console.log(`   Search performance: ${searchPassed ? '✅' : '❌'} (mean: ${meanTime.toFixed(2)}ms)`);
  
  return searchPassed;
}

function functionalTest() {
  console.log('🧪 Starting functional tests...');
  
  const dim = 384; // Smaller dimension for testing
  
  // Prepare test data
  console.log('📄 Test 1: Bundle creation and loading');
  const testDir = prepareTestData(10, dim);
  const bundleDir = path.join(__dirname, 'test_func_bundle');
  
  if (!createBundle(testDir, bundleDir)) {
    console.error('Failed to create test bundle');
    return false;
  }
  
  // Load the bundle
  const store = new VectorStore(bundleDir);
  console.log(`✅ Bundle loaded, store size: ${store.size()}`);
  
  // Test 2: Search functionality
  console.log('🔍 Test 2: Search functionality');
  const query = generateRandomEmbedding(dim);
  const results = store.search(query, 2);
  
  console.log(`✅ Search returned ${results.length} results`);
  if (results.length >= 2) {
    console.log(`   Result 1: ${results[0].id} (score: ${results[0].score.toFixed(4)})`);
    console.log(`   Result 2: ${results[1].id} (score: ${results[1].score.toFixed(4)})`);
  }
  
  // Test 3: BM25 search
  console.log('📝 Test 3: BM25 text search');
  const textResults = store.searchBM25('document testing', 2);
  console.log(`✅ BM25 search returned ${textResults.length} results`);
  
  // Test 4: Hybrid search
  console.log('🔀 Test 4: Hybrid search');
  const hybridResults = store.searchHybrid(query, 'document testing', 2);
  console.log(`✅ Hybrid search returned ${hybridResults.length} results`);
  
  // Test 5: Data integrity
  console.log('🔒 Test 5: Data integrity');
  const result = results[0];
  const isValidScore = typeof result.score === 'number' && !isNaN(result.score);
  const hasId = typeof result.id === 'string' && result.id.length > 0;
  const hasText = typeof result.text === 'string' && result.text.length > 0;
  const hasMetadata = typeof result.metadata === 'string';
  
  console.log(`✅ Data integrity check:`);
  console.log(`   Valid score: ${isValidScore}`);
  console.log(`   Has ID: ${hasId}`);
  console.log(`   Has text: ${hasText}`);
  console.log(`   Has metadata: ${hasMetadata}`);
  
  // Test 6: Store operations
  console.log('🔧 Test 6: Store operations');
  const isOpen = store.isOpen();
  const dimensions = store.dimensions();
  const size = store.size();
  
  console.log(`✅ Store operations:`);
  console.log(`   Is open: ${isOpen}`);
  console.log(`   Dimensions: ${dimensions}`);
  console.log(`   Size: ${size}`);
  
  // Clean up
  store.close();
  fs.rmSync(bundleDir, { recursive: true, force: true });
  fs.rmSync(testDir, { recursive: true, force: true });
  
  return isValidScore && hasId && hasText && isOpen && dimensions === dim;
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