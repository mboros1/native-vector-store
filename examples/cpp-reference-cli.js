#!/usr/bin/env node

const { VectorStore } = require('../');
const { OpenAI } = require('openai');
const path = require('path');
const fs = require('fs');
const readline = require('readline');
require('dotenv').config();

class CppReferenceCLI {
  constructor() {
    this.vectorStore = null;
    this.openai = null;
    this.rl = readline.createInterface({
      input: process.stdin,
      output: process.stdout,
      prompt: 'C++ Query> '
    });
  }

  async initialize() {
    console.log('🚀 Initializing C++ Reference Search...\n');
    
    // Check for OpenAI API key
    if (process.env.OPENAI_API_KEY) {
      this.openai = new OpenAI({
        apiKey: process.env.OPENAI_API_KEY
      });
      console.log('✅ OpenAI API configured');
    } else {
      console.log('⚠️  No OPENAI_API_KEY found - using demo mode with random embeddings');
    }

    // Find C++ docs
    const possiblePaths = [
      path.join(__dirname, '../cpp_std_embedded'),
      path.join(process.cwd(), 'cpp_std_embedded'),
      process.env.CPP_DOCS_PATH
    ].filter(Boolean);

    let docsPath = null;
    for (const docPath of possiblePaths) {
      if (fs.existsSync(docPath)) {
        docsPath = docPath;
        break;
      }
    }

    if (!docsPath) {
      console.error('❌ C++ documentation not found. Please run:');
      console.error('   node scripts/generate_embeddings.js cpp_std_embeddings cpp_std_embedded');
      process.exit(1);
    }

    console.log(`📂 Loading C++ docs from: ${docsPath}`);
    
    // Initialize vector store
    this.vectorStore = new VectorStore(1536);
    
    // Load documents
    const start = Date.now();
    this.vectorStore.loadDir(docsPath);
    const loadTime = Date.now() - start;
    
    console.log(`✅ Loaded ${this.vectorStore.size()} documents in ${loadTime}ms`);
    console.log(`✅ Ready for queries!\n`);
  }

  async getEmbedding(text) {
    if (this.openai) {
      try {
        const response = await this.openai.embeddings.create({
          model: "text-embedding-3-small",
          input: text,
        });
        return response.data[0].embedding;
      } catch (error) {
        console.error('OpenAI error:', error.message);
        return this.getDemoEmbedding(text);
      }
    }
    return this.getDemoEmbedding(text);
  }

  getDemoEmbedding(text) {
    // Generate deterministic "random" embedding based on text
    const embedding = new Array(1536);
    let hash = 0;
    for (let i = 0; i < text.length; i++) {
      hash = ((hash << 5) - hash) + text.charCodeAt(i);
      hash |= 0;
    }
    
    for (let i = 0; i < 1536; i++) {
      hash = (hash * 1103515245 + 12345) & 0x7fffffff;
      embedding[i] = (hash / 0x7fffffff) * 2 - 1;
    }
    
    return embedding;
  }

  async search(query, numResults = 5) {
    const embedding = await this.getEmbedding(query);
    const results = this.vectorStore.search(
      new Float32Array(embedding),
      numResults
    );
    
    return results;
  }

  formatResults(results, query) {
    console.log(`\n📍 Search results for: "${query}"\n`);
    
    results.forEach((result, idx) => {
      console.log(`${idx + 1}. ${result.id} (Score: ${result.score.toFixed(4)})`);
      console.log('   ' + '-'.repeat(60));
      
      // Show first 300 chars
      const preview = result.text
        .substring(0, 300)
        .trim()
        .replace(/\n/g, '\n   ');
      
      console.log(`   ${preview}...`);
      console.log('');
    });
  }

  async run() {
    await this.initialize();
    
    console.log('Commands:');
    console.log('  - Type any C++ query (e.g., "how to use std::vector")');
    console.log('  - Type "help" for more commands');
    console.log('  - Type "exit" to quit\n');
    
    this.rl.prompt();
    
    this.rl.on('line', async (line) => {
      const input = line.trim();
      
      if (input === 'exit' || input === 'quit') {
        console.log('Goodbye!');
        process.exit(0);
      }
      
      if (input === 'help') {
        console.log('\nAvailable commands:');
        console.log('  help     - Show this help');
        console.log('  stats    - Show vector store statistics');
        console.log('  exit     - Exit the program');
        console.log('  <query>  - Search for C++ documentation\n');
        console.log('Example queries:');
        console.log('  - std::vector operations');
        console.log('  - move semantics');
        console.log('  - template specialization');
        console.log('  - smart pointers\n');
        this.rl.prompt();
        return;
      }
      
      if (input === 'stats') {
        console.log('\nVector Store Statistics:');
        console.log(`  Documents: ${this.vectorStore.size()}`);
        console.log(`  Finalized: ${this.vectorStore.isFinalized()}`);
        console.log(`  OpenAI: ${this.openai ? 'Connected' : 'Demo mode'}\n`);
        this.rl.prompt();
        return;
      }
      
      if (input) {
        try {
          const results = await this.search(input, 5);
          this.formatResults(results, input);
        } catch (error) {
          console.error('Search error:', error.message);
        }
      }
      
      this.rl.prompt();
    });
  }
}

// Run the CLI
const cli = new CppReferenceCLI();
cli.run().catch(console.error);