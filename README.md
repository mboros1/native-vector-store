# native-vector-store

High-performance vector store with SIMD optimization for MCP servers and local RAG applications.

## Design Philosophy

This vector store is designed for **immutable, one-time loading** scenarios common in modern cloud deployments:

- **📚 Load Once, Query Many**: Documents are loaded at startup and remain immutable during serving
- **🚀 Optimized for Cold Starts**: Perfect for serverless functions and containerized deployments
- **📁 File-Based Organization**: Leverages filesystem for natural document organization and versioning
- **🎯 Focused API**: Does one thing exceptionally well - fast similarity search

This design eliminates complex state management, ensures consistent performance, and aligns perfectly with cloud-native deployment patterns.

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

**Runtime Requirements:**
- OpenMP runtime library (for parallel processing)
  - **Linux**: `sudo apt-get install libgomp1` (Ubuntu/Debian) or `dnf install libgomp` (Fedora)
  - **Alpine**: `apk add libgomp`
  - **macOS**: `brew install libomp`
  - **Windows**: Included with Visual C++ runtime

Prebuilt binaries are included for:
- Linux (x64, arm64, musl/Alpine)
- macOS (x64, arm64/Apple Silicon)
- Windows (x64)

If building from source, you'll need:
- Node.js ≥14.0.0
- C++ compiler with OpenMP support
- simdjson library (vendored, no installation needed)

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

## Usage Patterns

### Serverless Deployment (AWS Lambda, Vercel)

```javascript
// Initialize once during cold start
let store;

async function initializeStore() {
  if (!store) {
    store = new VectorStore(1536);
    store.loadDir('./knowledge-base'); // Loads and finalizes
  }
  return store;
}

// Handler reuses the store across invocations
export async function handler(event) {
  const store = await initializeStore();
  const embedding = new Float32Array(event.embedding);
  return store.search(embedding, 10);
}
```

### Local MCP Server

```javascript
const { VectorStore } = require('native-vector-store');

// Load at server startup
const store = new VectorStore(1536);
store.loadDir('./context');

// Serve many requests without reloading
server.on('search', (query) => {
  const results = store.search(query.embedding, 5);
  return results.filter(r => r.score > 0.7);
});
```

### CLI Tool with Persistent Context

```javascript
#!/usr/bin/env node
const { VectorStore } = require('native-vector-store');

// Load knowledge base once
const store = new VectorStore(1536);
store.loadDir(process.env.KNOWLEDGE_PATH || './docs');

// Interactive REPL with fast responses
const repl = require('repl');
const r = repl.start('> ');
r.context.search = (embedding, k = 5) => store.search(embedding, k);
```

### File Organization Best Practices

Structure your documents for optimal organization and performance:

```
knowledge-base/
├── products/          # Product documentation
│   ├── api-reference.json
│   └── user-guide.json
├── support/           # Support articles
│   ├── faq.json
│   └── troubleshooting.json
├── context/           # Context-specific docs
│   ├── company-info.json
│   └── policies.json
└── embeddings.json    # Shared embeddings
```

Each JSON file should contain a document or array of documents:

```json
{
  "id": "unique-id",
  "text": "Document content...",
  "metadata": {
    "embedding": [0.1, 0.2, ...],
    "category": "product",
    "lastUpdated": "2024-01-01"
  }
}
```

### Deployment Strategies

#### Blue-Green Deployment

```javascript
// Load new version without downtime
const newStore = new VectorStore(1536);
newStore.loadDir('./knowledge-base-v2');

// Atomic switch
app.locals.store = newStore;
```

#### Versioned Directories

```
deployments/
├── v1.0.0/
│   └── documents/
├── v1.1.0/
│   └── documents/
└── current -> v1.1.0  # Symlink to active version
```

#### Watch for Updates (Development)

```javascript
const fs = require('fs');

function reloadStore() {
  const newStore = new VectorStore(1536);
  newStore.loadDir('./documents');
  global.store = newStore;
  console.log(`Reloaded ${newStore.size()} documents`);
}

// Initial load
reloadStore();

// Watch for changes in development
if (process.env.NODE_ENV === 'development') {
  fs.watch('./documents', { recursive: true }, reloadStore);
}
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

## Performance

### Why It's Fast

The native-vector-store achieves exceptional performance through:

1. **Producer-Consumer Loading**: Parallel file I/O and JSON parsing achieve 178k+ documents/second
2. **SIMD Optimizations**: OpenMP vectorization for dot product calculations
3. **Arena Allocation**: Contiguous memory layout with 64MB chunks for cache efficiency
4. **Zero-Copy Design**: String views and pre-allocated buffers minimize allocations
5. **Two-Phase Architecture**: Loading phase allows concurrent writes, serving phase optimizes for reads

### Benchmarks

Performance on typical hardware (M1 MacBook Pro):

| Operation | Documents | Time | Throughput |
|-----------|-----------|------|------------|
| Loading (from disk) | 100,000 | ~560ms | 178k docs/sec |
| Search (k=10) | 10,000 corpus | 1-2ms | 500-1000 queries/sec |
| Search (k=100) | 100,000 corpus | 8-12ms | 80-125 queries/sec |
| Normalization | 100,000 | <100ms | 1M+ docs/sec |

### Performance Tips

1. **Optimal File Organization**: 
   - Keep 1000-10000 documents per JSON file for best I/O performance
   - Use arrays of documents in each file rather than one file per document

2. **Memory Considerations**:
   - Each document requires: `embedding_size * 4 bytes + metadata_size + text_size`
   - 100k documents with 1536-dim embeddings ≈ 600MB embeddings + metadata

3. **Search Performance**:
   - Scales linearly with corpus size and k value
   - Use smaller k values (5-20) for interactive applications
   - Pre-normalize query embeddings if making multiple searches

4. **Deployment Optimization**:
   - Preload in container images for faster cold starts
   - Use memory-mapped files for very large corpora
   - Consider sharding beyond 1M documents

### Comparison with Alternatives

| Feature | native-vector-store | Faiss | ChromaDB | Pinecone |
|---------|-------------------|--------|----------|----------|
| Load 100k docs | <1s | 2-5s | 30-60s | N/A (API) |
| Search latency | 1-2ms | 0.5-1ms | 50-200ms | 50-300ms |
| Memory efficiency | High | Medium | Low | N/A |
| Dependencies | Minimal | Heavy | Heavy | None |
| Deployment | Simple | Complex | Complex | SaaS |

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