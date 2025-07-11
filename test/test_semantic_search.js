#!/usr/bin/env node

const { VectorStore } = require('../');
const { OpenAI } = require('openai');
const path = require('path');

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

async function runSemanticSearchTests() {
  console.log('🔍 Semantic Search Tests\n');
  
  // Create vector store
  const store = new VectorStore(1536); // OpenAI embedding dimension
  
  // Load documents
  const docsPath = path.join(__dirname, '../out_embedded');
  console.log(`📂 Loading documents from ${docsPath}...`);
  const startLoad = Date.now();
  
  try {
    store.loadDir(docsPath);
    const loadTime = Date.now() - startLoad;
    console.log(`✅ Loaded ${store.size()} documents in ${loadTime}ms`);
    console.log(`   Store finalized: ${store.isFinalized()}\n`);
  } catch (error) {
    console.error('❌ Error loading documents:', error);
    process.exit(1);
  }
  
  // Test queries
  const testQueries = [
    {
      query: "How does the orchestrator manage parallel AI development?",
      expectedTopics: ["Architecture", "orchestrator", "parallel", "AI development"]
    },
    {
      query: "What are the best practices for using Claude Code?",
      expectedTopics: ["Best-Practices", "Claude", "practices", "guidelines"]
    },
    {
      query: "How do I create assignments and templates?",
      expectedTopics: ["ASSIGNMENT_TEMPLATE", "template", "assignment", "create"]
    },
    {
      query: "What changes were made in recent updates?",
      expectedTopics: ["CHANGELOG", "updates", "changes", "version"]
    },
    {
      query: "How do I use the Claude Code SDK for automation?",
      expectedTopics: ["claude-code-sdk", "SDK", "automation", "API"]
    }
  ];
  
  for (const test of testQueries) {
    console.log(`\n📝 Query: "${test.query}"`);
    
    // Get embedding
    const embedding = await getEmbedding(test.query);
    
    // Search
    const startSearch = Date.now();
    const results = store.search(new Float32Array(embedding), 5);
    const searchTime = Date.now() - startSearch;
    
    console.log(`⏱️  Search time: ${searchTime}ms`);
    console.log(`📊 Top 5 results:`);
    
    results.forEach((result, idx) => {
      // Extract filename from the document ID or text
      const docInfo = result.text || result.id;
      const relevantInfo = docInfo.substring(0, 100);
      
      console.log(`   ${idx + 1}. Score: ${result.score.toFixed(4)}`);
      console.log(`      Document: ${result.id}`);
      console.log(`      Preview: ${relevantInfo}...`);
      
      // Check if any expected topics appear in the result
      const matchedTopics = test.expectedTopics.filter(topic => 
        result.id.toLowerCase().includes(topic.toLowerCase()) ||
        (result.text && result.text.toLowerCase().includes(topic.toLowerCase()))
      );
      
      if (matchedTopics.length > 0) {
        console.log(`      ✅ Matched topics: ${matchedTopics.join(', ')}`);
      }
    });
    
    // Analyze if the top result makes sense
    const topResult = results[0];
    const isRelevant = test.expectedTopics.some(topic => 
      topResult.id.toLowerCase().includes(topic.toLowerCase()) ||
      (topResult.text && topResult.text.toLowerCase().includes(topic.toLowerCase()))
    );
    
    console.log(`\n   Relevance check: ${isRelevant ? '✅ Top result appears relevant' : '⚠️  Top result may not be most relevant'}`);
  }
  
  console.log('\n✨ Semantic search tests completed!\n');
}

// Run tests if this file is executed directly
if (require.main === module) {
  if (!process.env.OPENAI_API_KEY) {
    console.error('❌ Error: OPENAI_API_KEY environment variable not set');
    console.error('   Please set your OpenAI API key: export OPENAI_API_KEY=your-key-here');
    process.exit(1);
  }
  
  runSemanticSearchTests().catch(error => {
    console.error('❌ Error:', error);
    process.exit(1);
  });
}

module.exports = { runSemanticSearchTests };