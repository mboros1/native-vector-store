#!/usr/bin/env node

/**
 * MCP Server Integration Example
 * 
 * This example demonstrates how to use native-vector-store in an MCP server
 * for fast local RAG (Retrieval-Augmented Generation) capabilities.
 */

const { VectorStore } = require('../index');
const fs = require('fs');
const path = require('path');

class MCPVectorServer {
  constructor(dimensions = 1536) {
    this.store = new VectorStore(dimensions);
    this.dimensions = dimensions;
    this.isLoaded = false;
  }

  /**
   * Load document corpus from a directory of JSON files
   */
  async loadDocuments(documentsPath) {
    console.log(`Loading documents from: ${documentsPath}`);
    
    const startTime = Date.now();
    
    try {
      // Use native loadDir method for optimal performance
      this.store.loadDir(documentsPath);
      
      const loadTime = Date.now() - startTime;
      const docCount = this.store.size();
      
      console.log(`✅ Loaded ${docCount} documents in ${loadTime}ms`);
      console.log(`   Average: ${(loadTime / docCount).toFixed(2)}ms per document`);
      
      this.isLoaded = true;
      return { success: true, documentCount: docCount, loadTimeMs: loadTime };
      
    } catch (error) {
      console.error('❌ Error loading documents:', error);
      return { success: false, error: error.message };
    }
  }

  /**
   * Add a single document to the vector store
   */
  addDocument(document) {
    if (!document.id || !document.text || !document.metadata?.embedding) {
      throw new Error('Document must have id, text, and metadata.embedding');
    }
    
    if (document.metadata.embedding.length !== this.dimensions) {
      throw new Error(`Embedding dimension mismatch: expected ${this.dimensions}, got ${document.metadata.embedding.length}`);
    }
    
    this.store.addDocument(document);
    return { success: true, totalDocuments: this.store.size() };
  }

  /**
   * Search for similar documents
   */
  search(queryEmbedding, k = 5, threshold = 0.0) {
    if (!this.isLoaded) {
      throw new Error('No documents loaded. Call loadDocuments() first.');
    }
    
    if (queryEmbedding.length !== this.dimensions) {
      throw new Error(`Query embedding dimension mismatch: expected ${this.dimensions}, got ${queryEmbedding.length}`);
    }
    
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
   * Get server statistics
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
          
        case 'load_documents':
          const { path } = params;
          return await this.loadDocuments(path);
          
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
  
  // Initialize server
  const server = new MCPVectorServer(1536);
  
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