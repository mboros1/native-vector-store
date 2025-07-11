#!/usr/bin/env node

const { Server } = require('@modelcontextprotocol/sdk/server/index.js');
const { StdioServerTransport } = require('@modelcontextprotocol/sdk/server/stdio.js');
const {
  CallToolRequestSchema,
  ListResourcesRequestSchema,
  ListToolsRequestSchema,
  ReadResourceRequestSchema,
} = require('@modelcontextprotocol/sdk/types.js');
const { VectorStore } = require('../');
const path = require('path');
const fs = require('fs');

class CppReferenceServer {
  constructor() {
    this.server = new Server(
      {
        name: 'cpp-reference',
        version: '0.1.0',
      },
      {
        capabilities: {
          tools: {},
          resources: {},
        },
      }
    );

    this.vectorStore = null;
    this.isInitialized = false;
    this.docsPath = null;

    this.setupToolHandlers();
    
    // Initialize on startup
    this.initialize();
  }

  async initialize() {
    console.error('🚀 Initializing C++ Reference MCP Server...');
    
    // Try to find C++ docs in several locations
    const possiblePaths = [
      path.join(__dirname, '../cpp_std_embedded'),
      path.join(process.cwd(), 'cpp_std_embedded'),
      process.env.CPP_DOCS_PATH
    ].filter(Boolean);

    for (const docPath of possiblePaths) {
      if (fs.existsSync(docPath)) {
        this.docsPath = docPath;
        break;
      }
    }

    if (!this.docsPath) {
      console.error('❌ C++ documentation not found. Please set CPP_DOCS_PATH environment variable or run:');
      console.error('   node scripts/generate_embeddings.js cpp_std_embeddings cpp_std_embedded');
      return;
    }

    console.error(`📂 Loading C++ docs from: ${this.docsPath}`);
    
    // Initialize vector store with OpenAI embedding dimensions
    this.vectorStore = new VectorStore(1536);
    
    // Load documents
    const start = Date.now();
    this.vectorStore.loadDir(this.docsPath);
    const loadTime = Date.now() - start;
    
    console.error(`✅ Loaded ${this.vectorStore.size()} documents in ${loadTime}ms`);
    console.error(`✅ Vector store finalized: ${this.vectorStore.isFinalized()}`);
    
    this.isInitialized = true;
  }

  setupToolHandlers() {
    this.server.setRequestHandler(ListToolsRequestSchema, async () => ({
      tools: [
        {
          name: 'search_cpp_reference',
          description: 'Search the C++ standard library reference for relevant documentation',
          inputSchema: {
            type: 'object',
            properties: {
              query: {
                type: 'string',
                description: 'Natural language query about C++ (e.g., "how to use std::vector", "move semantics")'
              },
              num_results: {
                type: 'number',
                description: 'Number of results to return (default: 5, max: 20)',
                default: 5,
                minimum: 1,
                maximum: 20
              }
            },
            required: ['query']
          }
        },
        {
          name: 'get_cpp_topics',
          description: 'Get a list of major C++ topics and categories covered in the reference',
          inputSchema: {
            type: 'object',
            properties: {}
          }
        }
      ]
    }));

    this.server.setRequestHandler(CallToolRequestSchema, async (request) => {
      if (!this.isInitialized) {
        throw new Error('Vector store not initialized. Please check server logs.');
      }

      const { name, arguments: args } = request.params;

      switch (name) {
        case 'search_cpp_reference':
          return await this.searchCppReference(args);
        
        case 'get_cpp_topics':
          return await this.getCppTopics();
        
        default:
          throw new Error(`Unknown tool: ${name}`);
      }
    });

    this.server.setRequestHandler(ListResourcesRequestSchema, async () => ({
      resources: [
        {
          uri: 'cpp://status',
          name: 'C++ Reference Status',
          description: 'Information about the loaded C++ documentation',
          mimeType: 'text/plain'
        }
      ]
    }));

    this.server.setRequestHandler(ReadResourceRequestSchema, async (request) => {
      const { uri } = request.params;
      
      if (uri === 'cpp://status') {
        const status = this.isInitialized
          ? `C++ Reference loaded successfully\nDocuments: ${this.vectorStore.size()}\nPath: ${this.docsPath}`
          : 'C++ Reference not initialized';
        
        return {
          contents: [{
            uri,
            mimeType: 'text/plain',
            text: status
          }]
        };
      }
      
      throw new Error(`Unknown resource: ${uri}`);
    });
  }

  async searchCppReference(args) {
    const { query, num_results = 5 } = args;
    
    // Get embedding for the query
    const embedding = await this.getEmbedding(query);
    
    // Search vector store
    const results = this.vectorStore.search(
      new Float32Array(embedding),
      Math.min(num_results, 20)
    );
    
    // Format results
    const formattedResults = results.map((result, idx) => {
      const preview = result.text.substring(0, 500).trim();
      return `### Result ${idx + 1} (Score: ${result.score.toFixed(4)})\n**${result.id}**\n\n${preview}${result.text.length > 500 ? '...' : ''}`;
    }).join('\n\n---\n\n');
    
    return {
      content: [
        {
          type: 'text',
          text: `## C++ Reference Search Results for: "${query}"\n\n${formattedResults}`
        }
      ]
    };
  }

  async getCppTopics() {
    // Predefined list of major C++ topics
    const topics = [
      'Containers (vector, map, set, etc.)',
      'Algorithms (sort, find, transform, etc.)',
      'Memory Management (unique_ptr, shared_ptr)',
      'Iterators and Ranges',
      'Templates and Generic Programming',
      'Concurrency (thread, mutex, async)',
      'Type Traits and SFINAE',
      'Input/Output Streams',
      'Exception Handling',
      'Move Semantics and Rvalue References',
      'Lambda Expressions',
      'Concepts (C++20)',
      'Modules (C++20)',
      'Coroutines (C++20)',
      'Ranges Library (C++20)'
    ];
    
    return {
      content: [
        {
          type: 'text',
          text: `## C++ Standard Library Topics\n\n${topics.map(t => `- ${t}`).join('\n')}\n\nUse \`search_cpp_reference\` to search for detailed information on any topic.`
        }
      ]
    };
  }

  async getEmbedding(text) {
    // For the example, we'll use random embeddings
    // In production, you'd call OpenAI API here
    console.error(`⚠️  Using random embeddings for demo. For real results, implement OpenAI API call.`);
    
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

  async run() {
    const transport = new StdioServerTransport();
    await this.server.connect(transport);
    console.error('🎯 C++ Reference MCP Server running on stdio');
  }
}

const server = new CppReferenceServer();
server.run().catch(console.error);