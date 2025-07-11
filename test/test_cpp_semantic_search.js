#!/usr/bin/env node

const { VectorStore } = require('../');
const { OpenAI } = require('openai');
const path = require('path');
const fs = require('fs');

// Initialize OpenAI client
const openai = new OpenAI({
  apiKey: process.env.OPENAI_API_KEY
});

async function getEmbedding(text) {
  const response = await openai.embeddings.create({
    model: "text-embedding-3-small",
    input: text,
  });
  return response.data[0].embedding;
}

async function runCppSemanticSearchTests() {
  console.log('🔍 C++ Standard Library Semantic Search Tests\n');
  
  // Create vector store
  const store = new VectorStore(1536); // OpenAI embedding dimension
  
  // Load C++ standard documents
  const docsPath = path.join(__dirname, '../cpp_std_embedded');
  console.log(`📂 Loading C++ standard documents from ${docsPath}...`);
  
  // Check if embedded docs exist
  if (!fs.existsSync(docsPath)) {
    console.error('❌ Error: cpp_std_embedded directory not found');
    console.error('   Please run: node scripts/generate_embeddings.js cpp_std_embeddings cpp_std_embedded');
    process.exit(1);
  }
  
  const startLoad = Date.now();
  
  try {
    // Try memory-mapped loader first
    if (store.loadDirMMap) {
      console.log('   Using memory-mapped loader for better performance...');
      store.loadDirMMap(docsPath);
    } else {
      store.loadDir(docsPath);
    }
    const loadTime = Date.now() - startLoad;
    console.log(`✅ Loaded ${store.size()} documents in ${loadTime}ms`);
    console.log(`   Store finalized: ${store.isFinalized()}\n`);
  } catch (error) {
    console.error('❌ Error loading documents:', error);
    process.exit(1);
  }
  
  // C++ specific test queries
  const testQueries = [
    {
      query: "How do I use std::vector and what are its performance characteristics?",
      expectedTopics: ["vector", "container", "performance", "complexity", "iterator"],
      category: "Containers"
    },
    {
      query: "What is the difference between std::unique_ptr and std::shared_ptr?",
      expectedTopics: ["unique_ptr", "shared_ptr", "smart pointer", "ownership", "memory management"],
      category: "Memory Management"
    },
    {
      query: "How does move semantics and rvalue references work in C++11?",
      expectedTopics: ["move", "rvalue", "&&", "forward", "perfect forwarding"],
      category: "Modern C++"
    },
    {
      query: "What are the rules for template argument deduction and SFINAE?",
      expectedTopics: ["template", "deduction", "SFINAE", "substitution", "overload resolution"],
      category: "Templates"
    },
    {
      query: "How do I use std::async and std::future for concurrent programming?",
      expectedTopics: ["async", "future", "promise", "thread", "concurrent", "parallel"],
      category: "Concurrency"
    },
    {
      query: "What are constexpr functions and when should I use them?",
      expectedTopics: ["constexpr", "compile-time", "constant expression", "evaluation"],
      category: "Compile-time Programming"
    },
    {
      query: "How does the std::ranges library work in C++20?",
      expectedTopics: ["ranges", "view", "algorithm", "pipeline", "lazy evaluation"],
      category: "Ranges"
    },
    {
      query: "What is undefined behavior and how can I avoid it?",
      expectedTopics: ["undefined behavior", "UB", "well-formed", "diagnostic", "implementation-defined"],
      category: "Language Rules"
    },
    {
      query: "How do concepts work in C++20 and how do I define my own?",
      expectedTopics: ["concept", "requires", "constraint", "template", "C++20"],
      category: "Concepts"
    },
    {
      query: "What are the exception safety guarantees in the STL?",
      expectedTopics: ["exception", "safety", "strong guarantee", "basic guarantee", "noexcept"],
      category: "Exception Handling"
    }
  ];
  
  // Track category performance
  const categoryScores = {};
  
  for (const test of testQueries) {
    console.log(`\n📝 ${test.category}: "${test.query}"`);
    
    // Get embedding
    const embedding = await getEmbedding(test.query);
    
    // Search
    const startSearch = Date.now();
    const results = store.search(new Float32Array(embedding), 10);
    const searchTime = Date.now() - startSearch;
    
    console.log(`⏱️  Search time: ${searchTime}ms`);
    console.log(`📊 Top 5 results:`);
    
    let categoryRelevance = 0;
    
    results.slice(0, 5).forEach((result, idx) => {
      const docInfo = result.text || result.id;
      const preview = docInfo.substring(0, 150).replace(/\n/g, ' ');
      
      console.log(`   ${idx + 1}. Score: ${result.score.toFixed(4)}`);
      console.log(`      Document: ${result.id}`);
      console.log(`      Preview: ${preview}...`);
      
      // Check if any expected topics appear in the result
      const matchedTopics = test.expectedTopics.filter(topic => {
        const lowerText = (result.text || '').toLowerCase();
        const lowerId = result.id.toLowerCase();
        return lowerText.includes(topic.toLowerCase()) || lowerId.includes(topic.toLowerCase());
      });
      
      if (matchedTopics.length > 0) {
        console.log(`      ✅ Matched topics: ${matchedTopics.join(', ')}`);
        categoryRelevance += result.score * matchedTopics.length;
      }
    });
    
    // Analyze top result relevance
    const topResult = results[0];
    const isRelevant = test.expectedTopics.some(topic => {
      const lowerText = (topResult.text || '').toLowerCase();
      const lowerId = topResult.id.toLowerCase();
      return lowerText.includes(topic.toLowerCase()) || lowerId.includes(topic.toLowerCase());
    });
    
    console.log(`\n   Relevance check: ${isRelevant ? '✅ Top result appears relevant' : '⚠️  Top result may not be most relevant'}`);
    
    // Track category performance
    if (!categoryScores[test.category]) {
      categoryScores[test.category] = [];
    }
    categoryScores[test.category].push({
      topScore: topResult.score,
      relevant: isRelevant,
      avgRelevance: categoryRelevance / 5
    });
  }
  
  // Summary statistics
  console.log('\n📈 Category Performance Summary:');
  for (const [category, scores] of Object.entries(categoryScores)) {
    const avgTopScore = scores.reduce((sum, s) => sum + s.topScore, 0) / scores.length;
    const relevanceRate = scores.filter(s => s.relevant).length / scores.length;
    
    console.log(`\n   ${category}:`);
    console.log(`     Average top score: ${avgTopScore.toFixed(4)}`);
    console.log(`     Relevance rate: ${(relevanceRate * 100).toFixed(0)}%`);
  }
  
  // Additional analysis queries
  console.log('\n\n🔬 Additional Analysis Queries:');
  
  const analysisQueries = [
    "std::vector push_back complexity",
    "template specialization rules",
    "copy elision RVO NRVO",
    "memory order relaxed acquire release",
    "lambda capture by value reference"
  ];
  
  for (const query of analysisQueries) {
    console.log(`\n   Query: "${query}"`);
    const embedding = await getEmbedding(query);
    const results = store.search(new Float32Array(embedding), 3);
    
    console.log(`   Top result: ${results[0].id} (score: ${results[0].score.toFixed(4)})`);
    console.log(`   Preview: ${results[0].text.substring(0, 100).replace(/\n/g, ' ')}...`);
  }
  
  console.log('\n✨ C++ semantic search tests completed!\n');
}

// Run tests if this file is executed directly
if (require.main === module) {
  if (!process.env.OPENAI_API_KEY) {
    console.error('❌ Error: OPENAI_API_KEY environment variable not set');
    console.error('   Please set your OpenAI API key: export OPENAI_API_KEY=your-key-here');
    process.exit(1);
  }
  
  runCppSemanticSearchTests().catch(error => {
    console.error('❌ Error:', error);
    process.exit(1);
  });
}

module.exports = { runCppSemanticSearchTests };