# native-vector-store

High-performance vector store with SIMD optimization for MCP servers and local RAG applications.

## Features

- **🚀 High Performance**: C++ implementation with OpenMP SIMD optimization
- **📦 Arena Allocation**: Memory-efficient storage with 64MB chunks
- **⚡ Fast Search**: Sub-10ms similarity search for large document collections
- **🔧 MCP Integration**: Built for Model Context Protocol servers
- **🌐 Cross-Platform**: Works on Linux, macOS, and Windows
- **📊 TypeScript Support**: Full type definitions included
- **🔄 Producer-Consumer Loading**: Parallel document loading at 178k+ docs/sec

## Performance Targets

- **Load Time**: <1 second for 100,000 documents (achieved: ~560ms)
- **Search Latency**: <10ms for top-k similarity search (achieved: 1-2ms)
- **Memory Efficiency**: Minimal fragmentation via arena allocation
- **Scalability**: Designed for <1M embeddings
- **Throughput**: 178k+ documents per second with parallel loading

## Installation

```bash
npm install native-vector-store
```

### Prerequisites

For most users, **no prerequisites are needed** - prebuilt binaries are included for:
- Linux (x64, arm64)
- macOS (x64, arm64/Apple Silicon)
- Windows (x64)

If building from source, you'll need:
- Node.js ≥14.0.0
- C++ compiler with OpenMP support
- simdjson library (automatically handled by node-gyp)

## Quick Start

```javascript
const { VectorStore } = require('native-vector-store');

// Initialize with embedding dimensions (e.g., 1536 for OpenAI)
const store = new VectorStore(1536);

// Load documents from directory
store.loadDir('./documents'); // Automatically finalizes after loading

// Or add documents manually then finalize
const document = {
  id: 'doc-1',
  text: 'Example document text',
  metadata: {
    embedding: new Array(1536).fill(0).map(() => Math.random()),
    category: 'example'
  }
};

store.addDocument(document);
store.finalize(); // Must call before searching!

// Search for similar documents
const queryEmbedding = new Float32Array(1536);
const results = store.search(queryEmbedding, 5); // Top 5 results

console.log(results[0]); // { score: 0.95, id: 'doc-1', text: '...', metadata_json: '...' }
```

## MCP Server Integration

Perfect for building local RAG capabilities in MCP servers:

```javascript
const { MCPVectorServer } = require('native-vector-store/examples/mcp-server');

const server = new MCPVectorServer(1536);

// Load document corpus
await server.loadDocuments('./documents');

// Handle MCP requests
const response = await server.handleMCPRequest('vector_search', {
  query: queryEmbedding,
  k: 5,
  threshold: 0.7
});
```

## API Reference

### `VectorStore`

#### Constructor
```typescript
new VectorStore(dimensions: number)
```

#### Methods

##### `loadDir(path: string): void`
Load all JSON documents from a directory and automatically finalize the store. Files should contain document objects with embeddings.

##### `addDocument(doc: Document): void`
Add a single document to the store. Only works during loading phase (before finalization).

```typescript
interface Document {
  id: string;
  text: string;
  metadata: {
    embedding: number[];
    [key: string]: any;
  };
}
```

##### `search(query: Float32Array, k: number, normalizeQuery?: boolean): SearchResult[]`
Search for k most similar documents.

```typescript
interface SearchResult {
  score: number;
  id: string;
  text: string;
  metadata_json: string;
}
```

##### `finalize(): void`
Finalize the store: normalize all embeddings and switch to serving mode. After this, no more documents can be added but searches become available. This is automatically called by `loadDir()`.

##### `isFinalized(): boolean`
Check if the store has been finalized and is ready for searching.

##### `normalize(): void`
**Deprecated**: Use `finalize()` instead.

##### `size(): number`
Get the number of documents in the store.

## Building from Source

```bash
# Install dependencies
npm install

# Build native module
npm run build

# Run tests
npm test

# Run performance benchmarks
npm run benchmark

# Try MCP server example
npm run example
```

## Architecture

### Memory Layout
- **Arena Allocator**: 64MB chunks for cache-friendly access
- **Contiguous Storage**: Embeddings, strings, and metadata in single allocations
- **Zero-Copy Design**: Direct memory access without serialization overhead

### SIMD Optimization
- **OpenMP Pragmas**: Vectorized dot product operations
- **Parallel Processing**: Multi-threaded JSON loading and search
- **Cache-Friendly**: Aligned memory access patterns

### Performance Characteristics
- **Load Performance**: O(n) with parallel JSON parsing
- **Search Performance**: O(n⋅d) with SIMD acceleration
- **Memory Usage**: ~(d⋅4 + text_size) bytes per document

## Use Cases

### MCP Servers
Ideal for building local RAG (Retrieval-Augmented Generation) capabilities:
- Fast document loading from knowledge bases
- Low-latency similarity search for context retrieval
- Memory-efficient storage for large document collections

### Knowledge Management
Perfect for personal knowledge management systems:
- Index personal documents and notes
- Fast semantic search across content
- Offline operation without external dependencies

### Research Applications
Suitable for academic and research projects:
- Literature review and citation analysis
- Semantic clustering of research papers
- Cross-reference discovery in document collections

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests for new functionality
5. Ensure all tests pass
6. Submit a pull request

## License

MIT License - see LICENSE file for details.

## Benchmarks

Performance on M1 MacBook Pro with 1536-dimensional embeddings:

| Operation | Document Count | Time | Rate |
|-----------|---------------|------|------|
| Load | 10,000 | 245ms | 40.8k docs/sec |
| Search | 10,000 | 3.2ms | 3.1M docs/sec |
| Normalize | 10,000 | 12ms | 833k docs/sec |

*Results may vary based on hardware and document characteristics.*