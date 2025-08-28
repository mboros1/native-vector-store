#!/usr/bin/env node

/**
 * @file mcp-server.js
 * @description MCP (Model Context Protocol) Server Integration Example
 * 
 * This example demonstrates how to use native-vector-store in an MCP server
 * for fast local RAG (Retrieval-Augmented Generation) capabilities.
 * 
 * @example
 * // Usage as MCP server
 * const server = new MCPVectorServer(1536);
 * await server.loadDocuments('./knowledge-base');
 * const results = server.search(queryEmbedding, 10, 0.7);
 * 
 * @author Martin Boros
 * @license MIT
 */

const { VectorStoreV2 } = require('../index');
const { execSync } = require('child_process');
const fs = require('fs');
const path = require('path');

/**
 * @class MCPVectorServer
 * @description Wrapper class for VectorStore optimized for MCP server usage
 * 
 * Provides high-level methods for document management and semantic search
 * with built-in error handling and performance monitoring.
 */
class MCPVectorServer {
  /**
   * @constructor
   * @param {string} bundlePath - Path to vector store bundle
   */
  constructor(bundlePath) {
    this.bundlePath = bundlePath;
    this.store = null;
    this.isLoaded = false;
  }

  /**
   * Initialize the vector store from a bundle or create one
   * @async 
   * @param {string} [documentsPath] - Optional path to create bundle from
   * @returns {Promise<Object>} Loading result with success status, document count, and timing
   * @returns {boolean} returns.success - Whether loading succeeded
   * @returns {number} [returns.documentCount] - Number of documents loaded
   * @returns {number} [returns.loadTimeMs] - Time taken to load in milliseconds
   * @returns {string} [returns.error] - Error message if loading failed
   */
  async initialize(documentsPath) {
    const startTime = Date.now();
    
    try {
      // Check if bundle exists
      if (!fs.existsSync(this.bundlePath) && documentsPath) {
        // Create bundle from documents
        console.log(`📦 Creating bundle from ${documentsPath}...`);
        execSync(`./bin/nvs-pack ${documentsPath} ${this.bundlePath}`, {
          cwd: path.join(__dirname, '../src'),
          stdio: 'inherit'
        });
      }
      
      // Load the bundle
      console.log(`Loading bundle from: ${this.bundlePath}`);
      this.store = new VectorStoreV2(this.bundlePath);
      
      const loadTime = Date.now() - startTime;
      const docCount = this.store.size();
      
      console.log(`✅ Loaded ${docCount} documents in ${loadTime}ms`);
      console.log(`   Average: ${(loadTime / docCount).toFixed(2)}ms per document`);
      
      this.isLoaded = true;
      return { success: true, documentCount: docCount, loadTimeMs: loadTime };
      
    } catch (error) {
      console.error('❌ Error loading bundle:', error);
      return { success: false, error: error.message };
    }
  }

  /**
   * Note: VectorStoreV2 uses immutable bundles. Documents cannot be added after initialization.
   * To add documents, create a new bundle with all documents.
   * @deprecated Use bundle creation workflow instead
   */
  addDocument(document) {
    throw new Error('VectorStoreV2 uses immutable bundles. Create a new bundle with all documents.');
  }

  /**
   * Search for similar documents using vector similarity
   * @param {number[]} queryEmbedding - Query embedding vector
   * @param {number} [k=5] - Number of top results to return
   * @param {number} [threshold=0.0] - Minimum similarity score threshold (0-1)
   * @returns {Object} Search results with timing information
   * @returns {Array<Object>} returns.results - Array of matching documents
   * @returns {number} returns.searchTimeMs - Search execution time in milliseconds
   * @returns {number} returns.totalDocuments - Total documents in store
   * @throws {Error} If no documents loaded or dimension mismatch
   */
  search(queryEmbedding, k = 5, threshold = 0.0) {
    if (!this.isLoaded) {
      throw new Error('No documents loaded. Call loadDocuments() first.');
    }
    
    // VectorStoreV2 handles dimension validation internally
    
    const startTime = Date.now();
    
    // Convert to Float32Array for optimal performance
    const queryArray = new Float32Array(queryEmbedding);
    
    // Search with normalization enabled
    const results = this.store.search(queryArray, k, true);
    
    const searchTime = Date.now() - startTime;
    
    // Filter by threshold if specified
    const filteredResults = results.filter(result => result.score >= threshold);
    
    console.log(`🔍 Search completed in ${searchTime}ms, found ${filteredResults.length}/${results.length} results above threshold ${threshold}`);
    
    return {
      results: filteredResults,
      searchTimeMs: searchTime,
      totalDocuments: this.store.size()
    };
  }

  /**
   * Get server statistics and health information
   * @returns {Object} Server statistics
   * @returns {boolean} returns.isLoaded - Whether documents are loaded
   * @returns {number} returns.documentCount - Number of documents in store
   * @returns {number} returns.dimensions - Configured embedding dimensions
   * @returns {boolean} returns.isFinalized - Whether store is finalized for searching
   */
  getStats() {
    return {
      documentCount: this.store.size(),
      dimensions: this.dimensions,
      isLoaded: this.isLoaded,
      memoryUsage: process.memoryUsage()
    };
  }

  /**
   * MCP Server Tool Implementation
   */
  async handleMCPRequest(method, params) {
    try {
      switch (method) {
        case 'vector_search':
          const { query, k = 5, threshold = 0.0 } = params;
          return this.search(query, k, threshold);
          
        case 'add_document':
          const { document } = params;
          return this.addDocument(document);
          
        case 'initialize':
          const { path } = params;
          return await this.initialize(path);
          
        case 'get_stats':
          return this.getStats();
          
        default:
          throw new Error(`Unknown method: ${method}`);
      }
    } catch (error) {
      return { error: error.message };
    }
  }
}

// Example usage and demonstration
async function demonstration() {
  console.log('🚀 MCP Vector Server Demonstration');
  console.log('==================================\n');
  
  // Initialize server with bundle path
  const bundlePath = path.join(__dirname, '../sample_bundle');
  const server = new MCPVectorServer(bundlePath);
  
  // Example 1: Create sample documents
  console.log('📝 Creating sample documents...');
  const sampleDocs = [
    {
      id: 'doc-1',
      text: 'Machine learning algorithms for natural language processing',
      metadata: {
        embedding: Array.from({ length: 1536 }, () => Math.random() - 0.5),
        category: 'AI/ML',
        timestamp: Date.now()
      }
    },
    {
      id: 'doc-2', 
      text: 'Vector databases and similarity search optimization',
      metadata: {
        embedding: Array.from({ length: 1536 }, () => Math.random() - 0.5),
        category: 'Database',
        timestamp: Date.now()
      }
    },
    {
      id: 'doc-3',
      text: 'Building scalable web applications with Node.js',
      metadata: {
        embedding: Array.from({ length: 1536 }, () => Math.random() - 0.5),
        category: 'Web Development',
        timestamp: Date.now()
      }
    }
  ];
  
  // Add documents
  for (const doc of sampleDocs) {
    const result = server.addDocument(doc);
    console.log(`✅ Added document ${doc.id}: ${result.totalDocuments} total docs`);
  }
  
  // Example 2: Search simulation
  console.log('\n🔍 Simulating MCP search requests...');
  const queryEmbedding = Array.from({ length: 1536 }, () => Math.random() - 0.5);
  
  const searchResult = server.search(queryEmbedding, 2, 0.0);
  console.log(`Found ${searchResult.results.length} results:`);
  
  searchResult.results.forEach((result, index) => {
    console.log(`  ${index + 1}. ${result.id} (score: ${result.score.toFixed(4)})`);
    console.log(`     Text: ${result.text.substring(0, 50)}...`);
  });
  
  // Example 3: Performance stats
  console.log('\n📊 Server Statistics:');
  const stats = server.getStats();
  console.log(`   Documents loaded: ${stats.documentCount}`);
  console.log(`   Vector dimensions: ${stats.dimensions}`);
  console.log(`   Memory usage: ${Math.round(stats.memoryUsage.heapUsed / 1024 / 1024)}MB`);
  
  // Example 4: MCP request handling
  console.log('\n🔌 MCP Request Examples:');
  
  // Simulate MCP search request
  const mcpSearchResponse = await server.handleMCPRequest('vector_search', {
    query: queryEmbedding,
    k: 3,
    threshold: 0.0
  });
  
  console.log('Vector search response:', {
    resultsCount: mcpSearchResponse.results?.length,
    searchTime: mcpSearchResponse.searchTimeMs
  });
  
  // Simulate MCP stats request
  const mcpStatsResponse = await server.handleMCPRequest('get_stats', {});
  console.log('Stats response:', {
    documentCount: mcpStatsResponse.documentCount,
    dimensions: mcpStatsResponse.dimensions
  });
  
  console.log('\n🎉 MCP Server demonstration complete!');
  console.log('\nTo use this in a real MCP server:');
  console.log('1. Initialize MCPVectorServer with your embedding dimensions');
  console.log('2. Load your document corpus using loadDocuments()');
  console.log('3. Handle MCP requests using handleMCPRequest()');
  console.log('4. Enjoy fast local RAG capabilities! 🚀');
}

// CLI interface
if (require.main === module) {
  const args = process.argv.slice(2);
  
  if (args.length === 0) {
    demonstration();
  } else {
    const command = args[0];
    
    switch (command) {
      case 'demo':
        demonstration();
        break;
        
      case 'load':
        if (args.length < 2) {
          console.error('Usage: node mcp-server.js load <documents-path>');
          process.exit(1);
        }
        const server = new MCPVectorServer();
        server.loadDocuments(args[1]);
        break;
        
      default:
        console.error('Available commands: demo, load');
        process.exit(1);
    }
  }
}

module.exports = { MCPVectorServer };