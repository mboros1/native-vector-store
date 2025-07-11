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
const { OpenAI } = require('openai');
const path = require('path');
const fs = require('fs');
require('dotenv').config();

class CppReferenceServer {
  constructor() {
    this.server = new Server(
      {
        name: 'cpp-reference-openai',
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
    this.openai = null;

    this.setupToolHandlers();
    
    // Initialize on startup
    this.initialize();
  }

  async initialize() {
    console.error('🚀 Initializing C++ Reference MCP Server with OpenAI...');
    
    // Check for OpenAI API key
    if (!process.env.OPENAI_API_KEY) {
      console.error('❌ OPENAI_API_KEY environment variable not set');
      console.error('   Please set it in .env file or environment');
      return;
    }

    this.openai = new OpenAI({
      apiKey: process.env.OPENAI_API_KEY
    });

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
      console.error('❌ C++ documentation not found. Please run:');
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
          description: 'Search the C++ standard library reference for relevant documentation. Returns the most relevant sections from the C++ standard.',
          inputSchema: {
            type: 'object',
            properties: {
              query: {
                type: 'string',
                description: 'Natural language query about C++ (e.g., "how to use std::vector", "move semantics", "SFINAE")'
              },
              num_results: {
                type: 'number',
                description: 'Number of results to return (default: 5, max: 20)',
                default: 5,
                minimum: 1,
                maximum: 20
              },
              include_examples: {
                type: 'boolean',
                description: 'Include code examples if available (default: true)',
                default: true
              }
            },
            required: ['query']
          }
        },
        {
          name: 'search_cpp_by_category',
          description: 'Search C++ reference by specific category or component',
          inputSchema: {
            type: 'object',
            properties: {
              category: {
                type: 'string',
                description: 'Category to search within',
                enum: [
                  'containers',
                  'algorithms',
                  'memory',
                  'concurrency',
                  'ranges',
                  'concepts',
                  'utilities',
                  'io',
                  'numerics'
                ]
              },
              query: {
                type: 'string',
                description: 'Query within the category'
              },
              num_results: {
                type: 'number',
                description: 'Number of results (default: 5)',
                default: 5
              }
            },
            required: ['category', 'query']
          }
        },
        {
          name: 'get_cpp_section',
          description: 'Get a specific section of the C++ standard by its identifier',
          inputSchema: {
            type: 'object',
            properties: {
              section_id: {
                type: 'string',
                description: 'Section identifier (e.g., "containers.sequences.vector")'
              }
            },
            required: ['section_id']
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
        
        case 'search_cpp_by_category':
          return await this.searchByCategory(args);
          
        case 'get_cpp_section':
          return await this.getCppSection(args);
        
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
        },
        {
          uri: 'cpp://stats',
          name: 'C++ Reference Statistics',
          description: 'Detailed statistics about the loaded documentation',
          mimeType: 'application/json'
        }
      ]
    }));

    this.server.setRequestHandler(ReadResourceRequestSchema, async (request) => {
      const { uri } = request.params;
      
      if (uri === 'cpp://status') {
        const status = this.isInitialized
          ? `C++ Reference loaded successfully\nDocuments: ${this.vectorStore.size()}\nPath: ${this.docsPath}\nOpenAI: Connected`
          : 'C++ Reference not initialized';
        
        return {
          contents: [{
            uri,
            mimeType: 'text/plain',
            text: status
          }]
        };
      }
      
      if (uri === 'cpp://stats') {
        const stats = {
          initialized: this.isInitialized,
          documents: this.isInitialized ? this.vectorStore.size() : 0,
          path: this.docsPath,
          openai_configured: !!this.openai
        };
        
        return {
          contents: [{
            uri,
            mimeType: 'application/json',
            text: JSON.stringify(stats, null, 2)
          }]
        };
      }
      
      throw new Error(`Unknown resource: ${uri}`);
    });
  }

  async searchCppReference(args) {
    const { query, num_results = 5, include_examples = true } = args;
    
    // Get embedding for the query
    const embedding = await this.getEmbedding(query);
    
    // Search vector store
    const results = this.vectorStore.search(
      new Float32Array(embedding),
      Math.min(num_results, 20)
    );
    
    // Format results with better structure
    const formattedResults = results.map((result, idx) => {
      let content = result.text;
      
      // Extract code examples if requested
      if (include_examples) {
        const codeMatch = content.match(/```[\s\S]*?```/g);
        if (codeMatch) {
          content = `${result.text.substring(0, 600).trim()}...\n\n**Code Example:**\n${codeMatch[0]}`;
        } else {
          content = result.text.substring(0, 800).trim() + '...';
        }
      } else {
        content = result.text.substring(0, 600).trim() + '...';
      }
      
      return `### ${idx + 1}. ${result.id} (Relevance: ${(result.score * 100).toFixed(1)}%)\n\n${content}`;
    }).join('\n\n---\n\n');
    
    return {
      content: [
        {
          type: 'text',
          text: `# C++ Reference Search: "${query}"\n\nFound ${results.length} relevant sections:\n\n${formattedResults}\n\n💡 *Tip: Use \`get_cpp_section\` with a section ID to get the full content.*`
        }
      ]
    };
  }

  async searchByCategory(args) {
    const { category, query, num_results = 5 } = args;
    
    // Enhance query with category context
    const enhancedQuery = `${category} ${query}`;
    
    const embedding = await this.getEmbedding(enhancedQuery);
    const results = this.vectorStore.search(
      new Float32Array(embedding),
      Math.min(num_results * 2, 40) // Get more results to filter
    );
    
    // Filter results by category keywords
    const categoryKeywords = {
      containers: ['vector', 'list', 'map', 'set', 'deque', 'array', 'container'],
      algorithms: ['algorithm', 'sort', 'find', 'search', 'transform', 'reduce'],
      memory: ['memory', 'pointer', 'allocator', 'unique_ptr', 'shared_ptr', 'weak_ptr'],
      concurrency: ['thread', 'mutex', 'atomic', 'future', 'async', 'lock'],
      ranges: ['ranges', 'view', 'range', 'sentinel', 'iterator'],
      concepts: ['concept', 'requires', 'constraint', 'template'],
      utilities: ['utility', 'tuple', 'optional', 'variant', 'any'],
      io: ['stream', 'iostream', 'fstream', 'stringstream', 'io'],
      numerics: ['numeric', 'complex', 'random', 'valarray', 'math']
    };
    
    const keywords = categoryKeywords[category] || [];
    const filteredResults = results
      .filter(r => keywords.some(kw => r.id.toLowerCase().includes(kw) || r.text.toLowerCase().includes(kw)))
      .slice(0, num_results);
    
    const formattedResults = filteredResults.map((result, idx) => {
      const preview = result.text.substring(0, 500).trim();
      return `### ${idx + 1}. ${result.id}\n\n${preview}...`;
    }).join('\n\n---\n\n');
    
    return {
      content: [
        {
          type: 'text',
          text: `# C++ ${category.charAt(0).toUpperCase() + category.slice(1)} Search: "${query}"\n\n${formattedResults}`
        }
      ]
    };
  }

  async getCppSection(args) {
    const { section_id } = args;
    
    // Search for exact section match
    const embedding = await this.getEmbedding(section_id);
    const results = this.vectorStore.search(
      new Float32Array(embedding),
      10
    );
    
    // Find exact match or closest match
    const exactMatch = results.find(r => r.id.toLowerCase().includes(section_id.toLowerCase()));
    const result = exactMatch || results[0];
    
    if (!result) {
      return {
        content: [{
          type: 'text',
          text: `Section "${section_id}" not found.`
        }]
      };
    }
    
    return {
      content: [
        {
          type: 'text',
          text: `# ${result.id}\n\n${result.text}`
        }
      ]
    };
  }

  async getEmbedding(text) {
    try {
      const response = await this.openai.embeddings.create({
        model: "text-embedding-3-small",
        input: text,
      });
      return response.data[0].embedding;
    } catch (error) {
      console.error('Error getting embedding:', error);
      throw new Error('Failed to generate embedding. Check OpenAI API key and connection.');
    }
  }

  async run() {
    const transport = new StdioServerTransport();
    await this.server.connect(transport);
    console.error('🎯 C++ Reference MCP Server (OpenAI) running on stdio');
  }
}

const server = new CppReferenceServer();
server.run().catch(console.error);