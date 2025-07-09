const { VectorStore } = require('../index');
const fs = require('fs');
const path = require('path');

function testDocumentLoader() {
  console.log('🧪 Testing Document Loader Functionality');
  console.log('========================================\n');
  
  const dim = 20; // Match our test documents
  const store = new VectorStore(dim);
  
  // Test 1: Load from directory
  console.log('📂 Test 1: Loading documents from directory');
  
  const testDataDir = path.join(__dirname, '.');
  
  try {
    const startTime = Date.now();
    store.loadDir(testDataDir);
    const loadTime = Date.now() - startTime;
    
    console.log(`✅ Successfully loaded directory in ${loadTime}ms`);
    console.log(`📊 Total documents loaded: ${store.size()}`);
    
    if (store.size() === 0) {
      console.log('❌ No documents were loaded - check file format');
      return false;
    }
    
  } catch (error) {
    console.error('❌ Error loading directory:', error.message);
    return false;
  }
  
  // Test 2: Verify embeddings are loaded correctly
  console.log('\n🔍 Test 2: Verifying embedding integrity');
  
  // Create a test query that should match our known embeddings
  const testQuery = new Float32Array([0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0]);
  
  try {
    const results = store.search(testQuery, 3);
    
    console.log(`✅ Search returned ${results.length} results`);
    
    results.forEach((result, index) => {
      console.log(`   ${index + 1}. ID: ${result.id}, Score: ${result.score.toFixed(4)}`);
      console.log(`      Text: ${result.text.substring(0, 50)}...`);
      
      // Verify the result has the expected structure
      if (!result.id || !result.text || typeof result.score !== 'number') {
        console.log('❌ Result structure is invalid');
        return false;
      }
    });
    
    // Test that we get reasonable similarity scores
    const topScore = results[0].score;
    if (topScore < 0.5) {
      console.log(`⚠️  Warning: Top similarity score is low (${topScore.toFixed(4)})`);
    } else {
      console.log(`✅ Top similarity score looks good (${topScore.toFixed(4)})`);
    }
    
  } catch (error) {
    console.error('❌ Error during search:', error.message);
    return false;
  }
  
  // Test 3: Verify document metadata preservation
  console.log('\n📋 Test 3: Checking metadata preservation');
  
  try {
    const results = store.search(testQuery, 1);
    if (results.length > 0) {
      const result = results[0];
      
      console.log(`✅ Result has metadata_json: ${result.metadata_json ? 'Yes' : 'No'}`);
      
      if (result.metadata_json) {
        try {
          const metadata = JSON.parse(result.metadata_json);
          console.log(`   Metadata keys: ${Object.keys(metadata).join(', ')}`);
          
          // Check if embedding exists in metadata
          if (metadata.metadata && metadata.metadata.embedding) {
            console.log(`   Embedding length: ${metadata.metadata.embedding.length}`);
            console.log(`✅ Embedding preserved in metadata`);
          } else {
            console.log('❌ No embedding found in metadata');
          }
          
        } catch (parseError) {
          console.log('❌ Failed to parse metadata JSON');
        }
      }
    }
    
  } catch (error) {
    console.error('❌ Error checking metadata:', error.message);
    return false;
  }
  
  // Test 4: Test normalization
  console.log('\n🧮 Test 4: Testing normalization');
  
  try {
    const startTime = Date.now();
    store.normalize();
    const normTime = Date.now() - startTime;
    
    console.log(`✅ Normalization completed in ${normTime}ms`);
    
    // Test search after normalization
    const results = store.search(testQuery, 1);
    if (results.length > 0) {
      console.log(`✅ Search works after normalization, score: ${results[0].score.toFixed(4)}`);
    }
    
  } catch (error) {
    console.error('❌ Error during normalization:', error.message);
    return false;
  }
  
  console.log('\n🎉 All document loader tests passed!');
  return true;
}

function testSingleDocumentAdd() {
  console.log('\n🧪 Testing Single Document Addition');
  console.log('===================================\n');
  
  const dim = 20;
  const store = new VectorStore(dim);
  
  // Test adding a single document
  const testDoc = {
    id: 'single_test',
    text: 'This is a single test document for verification.',
    metadata: {
      embedding: Array.from({length: dim}, (_, i) => Math.sin(i * 0.1)),
      category: 'Test',
      timestamp: new Date().toISOString()
    }
  };
  
  try {
    console.log('📝 Adding single document...');
    store.addDocument(testDoc);
    
    console.log(`✅ Document added successfully`);
    console.log(`📊 Store size: ${store.size()}`);
    
    // Test search
    const query = new Float32Array(testDoc.metadata.embedding);
    const results = store.search(query, 1);
    
    if (results.length > 0) {
      const result = results[0];
      console.log(`✅ Document found with score: ${result.score.toFixed(4)}`);
      console.log(`   ID: ${result.id}`);
      console.log(`   Text: ${result.text.substring(0, 50)}...`);
      
      // Should have perfect similarity to itself
      if (result.score > 0.99) {
        console.log('✅ Perfect similarity achieved');
      } else {
        console.log(`⚠️  Expected higher similarity, got ${result.score.toFixed(4)}`);
      }
    }
    
  } catch (error) {
    console.error('❌ Error adding single document:', error.message);
    return false;
  }
  
  console.log('\n🎉 Single document addition test passed!');
  return true;
}

async function main() {
  console.log('🚀 Vector Store Document Loader Test Suite');
  console.log('==========================================\n');
  
  // Check if test data exists
  const testDataFile = path.join(__dirname, 'sample_docs.json');
  if (!fs.existsSync(testDataFile)) {
    console.error('❌ Test data file not found:', testDataFile);
    console.log('Please ensure sample_docs.json exists in the test directory');
    process.exit(1);
  }
  
  console.log('📁 Test data file found:', testDataFile);
  
  // Run tests
  const test1Passed = testDocumentLoader();
  const test2Passed = testSingleDocumentAdd();
  
  // Summary
  console.log('\n📋 Test Summary:');
  console.log(`   Document loader tests: ${test1Passed ? '✅ PASSED' : '❌ FAILED'}`);
  console.log(`   Single document tests: ${test2Passed ? '✅ PASSED' : '❌ FAILED'}`);
  
  if (test1Passed && test2Passed) {
    console.log('\n🎉 All tests passed! Document loader is working correctly.');
    process.exit(0);
  } else {
    console.log('\n❌ Some tests failed. Please check the implementation.');
    process.exit(1);
  }
}

// Run tests if this file is executed directly
if (require.main === module) {
  main();
}