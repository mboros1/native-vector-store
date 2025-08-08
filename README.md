# native-vector-store

High-performance vector store with SIMD optimization for MCP servers and local RAG applications.

📚 **[API Documentation](https://mboros1.github.io/native-vector-store/)** | 📦 **[npm](https://www.npmjs.com/package/native-vector-store)** | 🐙 **[GitHub](https://github.com/mboros1/native-vector-store)**

## Design Philosophy

This vector store is designed for **immutable, one-time loading** scenarios common in modern cloud deployments:

- **📚 Load Once, Query Many**: Documents are loaded at startup and remain immutable during serving
- **🚀 Optimized for Cold Starts**: Perfect for serverless functions and containerized deployments
- **📁 File-Based Organization**: Leverages filesystem for natural document organization and versioning
- **🎯 Focused API**: Does one thing exceptionally well - fast similarity search over focused corpora (sweet spot: <100k documents)

This design eliminates complex state management, ensures consistent performance, and aligns perfectly with cloud-native deployment patterns where domain-specific knowledge bases are the norm.

## Features

- **🚀 High Performance**: C++ implementation with OpenMP SIMD optimization
- **📦 Arena Allocation**: Memory-efficient storage with 64MB chunks
- **⚡ Fast Search**: Sub-10ms similarity search for large document collections
- **🔍 Hybrid Search**: Combines vector similarity (semantic) with BM25 text search (lexical)
- **🔧 MCP Integration**: Built for Model Context Protocol servers
- **🌐 Cross-Platform**: Works on Linux, macOS, and Windows
- **📊 TypeScript Support**: Full type definitions included
- **🔄 Producer-Consumer Loading**: Parallel document loading at 178k+ docs/sec

## Performance Targets

- **Load Time**: <1 second for 100,000 documents (achieved: ~560ms)
- **Search Latency**: <10ms for top-k similarity search (achieved: 1-2ms)
- **Memory Efficiency**: Minimal fragmentation via arena allocation
- **Scalability**: Designed for focused corpora (<100k documents optimal, <1M maximum)
- **Throughput**: 178k+ documents per second with parallel loading

📊 **[Production Case Study](docs/PRODUCTION_CASE_STUDY.md)**: Real-world deployment with 65k documents (1.5GB) on AWS Lambda achieving 15-20s cold start and 40-45ms search latency.

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
- Linux (x64, arm64, musl/Alpine) - x64 builds are AWS Lambda compatible (no AVX-512)
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

// Option 1: Vector-only search (traditional)
const results = store.search(queryEmbedding, 5); // Top 5 results

// Option 2: Hybrid search (NEW - combines vector + BM25 text search)
const hybridResults = store.search(queryEmbedding, 5, "your search query text");

// Option 3: BM25 text-only search
const textResults = store.searchBM25("your search query", 5);

// Results format - array of SearchResult objects, sorted by score (highest first):
console.log(results);
// [
//   {
//     score: 0.987654,            // Similarity score (0-1, higher = more similar)
//     id: "doc-1",                // Your document ID
//     text: "Example document...", // Full document text
//     metadata_json: "{\"embedding\":[0.1,0.2,...],\"category\":\"example\"}"  // JSON string
//   },
//   { score: 0.943210, id: "doc-7", text: "Another doc...", metadata_json: "..." },
//   // ... up to 5 results
// ]

// Parse metadata from the top result
const topResult = results[0];
const metadata = JSON.parse(topResult.metadata_json);
console.log(metadata.category); // "example"
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

// Load different knowledge domains at startup
const stores = {
  products: new VectorStore(1536),
  support: new VectorStore(1536),
  general: new VectorStore(1536)
};

stores.products.loadDir('./knowledge/products');
stores.support.loadDir('./knowledge/support');
stores.general.loadDir('./knowledge/general');

// Route searches to appropriate domain
server.on('search', (query) => {
  const store = stores[query.domain] || stores.general;
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

Structure your documents by category for separate vector stores:

```
knowledge-base/
├── products/          # Product documentation
│   ├── api-reference.json
│   └── user-guide.json
├── support/           # Support articles
│   ├── faq.json
│   └── troubleshooting.json
└── context/           # Context-specific docs
    ├── company-info.json
    └── policies.json
```

Load each category into its own VectorStore:

```javascript
// Create separate stores for different domains
const productStore = new VectorStore(1536);
const supportStore = new VectorStore(1536);
const contextStore = new VectorStore(1536);

// Load each category independently
productStore.loadDir('./knowledge-base/products');
supportStore.loadDir('./knowledge-base/support');
contextStore.loadDir('./knowledge-base/context');

// Search specific domains
const productResults = productStore.search(queryEmbedding, 5);
const supportResults = supportStore.search(queryEmbedding, 5);
```

Each JSON file contains self-contained documents with embeddings:

```json
{
  "id": "unique-id",              // Required: unique document identifier
  "text": "Document content...",   // Required: searchable text content (or use "content" for Spring AI)
  "metadata": {                    // Required: metadata object
    "embedding": [0.1, 0.2, ...],  // Required: array of numbers matching vector dimensions
    "category": "product",         // Optional: additional metadata
    "lastUpdated": "2024-01-01"    // Optional: additional metadata
  }
}
```

**Spring AI Compatibility**: You can use `"content"` instead of `"text"` for the document field. The library auto-detects which field name you're using from the first document and optimizes subsequent lookups.

**Common Mistakes:**
- ❌ Putting `embedding` at the root level instead of inside `metadata`
- ❌ Using string format for embeddings instead of number array
- ❌ Missing required fields (`id`, `text`, or `metadata`)
- ❌ Wrong embedding dimensions (must match VectorStore constructor)

**Validate your JSON format:**
```bash
node node_modules/native-vector-store/examples/validate-format.js your-file.json
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

## Hybrid Search

The vector store now supports hybrid search, combining semantic similarity (vector search) with lexical matching (BM25 text search) for improved retrieval accuracy:

```javascript
const { VectorStore } = require('native-vector-store');

const store = new VectorStore(1536);
store.loadDir('./documents');

// Hybrid search automatically combines vector and text search
const queryEmbedding = new Float32Array(1536);
const results = store.search(
  queryEmbedding, 
  10,                               // Top 10 results
  "machine learning algorithms"    // Query text for BM25
);

// You can also use individual search methods
const vectorResults = store.searchVector(queryEmbedding, 10);
const textResults = store.searchBM25("machine learning", 10);

// Or explicitly control the hybrid weights
const customResults = store.searchHybrid(
  queryEmbedding,
  "machine learning",
  10,
  0.3,  // Vector weight (30%)
  0.7   // BM25 weight (70%)
);

// Tune BM25 parameters for your corpus
store.setBM25Parameters(
  1.2,  // k1: Term frequency saturation (default: 1.2)
  0.75, // b: Document length normalization (default: 0.75)
  1.0   // delta: Smoothing parameter (default: 1.0)
);
```

Hybrid search is particularly effective for:
- **Question answering**: BM25 finds documents with exact terms while vectors capture semantic meaning
- **Knowledge retrieval**: Combines conceptual similarity with keyword matching
- **Multi-lingual search**: Vectors handle cross-language similarity while BM25 matches exact terms

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

Full API documentation is available at:
- **[Latest Documentation](https://mboros1.github.io/native-vector-store/)** - Always current
- **Versioned Documentation** - Available at `https://mboros1.github.io/native-vector-store/{version}/` (e.g., `/v0.3.0/`)
- **Local Documentation** - After installing: `open node_modules/native-vector-store/docs/index.html`

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
Search for k most similar documents. Returns an array sorted by score (highest first).

```typescript
interface SearchResult {
  score: number;        // Cosine similarity (0-1, higher = more similar)
  id: string;           // Document ID
  text: string;         // Document text content
  metadata_json: string; // JSON string with all metadata including embedding
}

// Example return value:
[
  {
    score: 0.98765,
    id: "doc-123", 
    text: "Introduction to machine learning...",
    metadata_json: "{\"embedding\":[0.1,0.2,...],\"author\":\"Jane Doe\",\"tags\":[\"ML\",\"intro\"]}"
  },
  {
    score: 0.94321,
    id: "doc-456",
    text: "Deep learning fundamentals...", 
    metadata_json: "{\"embedding\":[0.3,0.4,...],\"difficulty\":\"intermediate\"}"
  }
  // ... more results
]
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
| Loading (from disk) | 10,000 | 153ms | 65k docs/sec |
| Loading (from disk) | 100,000 | ~560ms | 178k docs/sec |
| Loading (production) | 65,000 | 15-20s | 3.2-4.3k docs/sec |
| Search (k=10) | 10,000 corpus | 2ms | 500 queries/sec |
| Search (k=10) | 65,000 corpus | 40-45ms | 20-25 queries/sec |
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

4. **Corpus Size Optimization**:
   - Sweet spot: <100k documents for optimal load/search balance
   - Beyond 100k: Consider if your use case truly needs all documents
   - Focus on curated, domain-specific content rather than exhaustive datasets

### Comparison with Alternatives

| Feature | native-vector-store | Faiss | ChromaDB | Pinecone |
|---------|-------------------|--------|----------|----------|
| Load 100k docs | <1s | 2-5s | 30-60s | N/A (API) |
| Search latency | 1-2ms | 0.5-1ms | 50-200ms | 50-300ms |
| Memory efficiency | High | Medium | Low | N/A |
| Dependencies | Minimal | Heavy | Heavy | None |
| Deployment | Simple | Complex | Complex | SaaS |
| Sweet spot | <100k docs | Any size | Any size | Any size |

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
- Fast document loading from focused knowledge bases
- Low-latency similarity search for context retrieval
- Memory-efficient storage for domain-specific corpora

### Knowledge Management
Perfect for personal knowledge management systems:
- Index personal documents and notes (typically <10k documents)
- Fast semantic search across focused content
- Offline operation without external dependencies

### Research Applications
Suitable for academic and research projects with focused datasets:
- Literature review within specific domains
- Semantic clustering of curated paper collections
- Cross-reference discovery in specialized corpora

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
| Load | 10,000 | 153ms | 65.4k docs/sec |
| Search | 10,000 | 2ms | 5M docs/sec |
| Normalize | 10,000 | 12ms | 833k docs/sec |

*Results may vary based on hardware and document characteristics.*