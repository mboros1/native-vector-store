# Worker CLAUDE.md - native-vector-store

**Branch**: feature/initial-npm-package
**Private Branch**: private/worker_vector_store/Initial_npm_package_implementation
**Assignment PR**: #1
**Started**: 2025-07-09

## Project Context

Building a native vector store npm package for MCP server integration. The package provides:
- C++ vector store with arena allocation for memory efficiency
- OpenMP SIMD-optimized similarity search operations
- Node.js N-API bindings for TypeScript integration
- Target performance: <1s load time for 100k docs, <10ms search latency

## My Task Understanding

I need to implement a complete npm package that:
1. Ports C++ VectorStore class with arena allocator from research workspace
2. Adds OpenMP parallel processing and SIMD optimizations
3. Creates Node.js N-API wrapper with TypeScript definitions
4. Includes comprehensive testing with research data
5. Provides MCP server integration example
6. Ensures cross-platform build support

## Technical Approach

**Architecture**:
- C++ core with arena allocator for cache-friendly memory layout
- OpenMP SIMD pragmas for vectorized dot product operations
- Node.js N-API bindings for TypeScript integration
- JSON document loading with parallel processing

**File Structure**:
- src/: C++ implementation (vector_store.h, binding.cc, binding.gyp)
- lib/: TypeScript definitions (index.d.ts)
- test/: Unit tests, integration tests, performance benchmarks
- examples/: MCP server integration example

## Progress Log

2025-07-09: Started assignment, created private branch, set up workspace

---

**IMPORTANT**: This file should NEVER be committed to your feature branch!


## Vector Store Implementation Details

### Core Architecture
- **Arena Allocator**: Custom 64MB chunk-based allocator for cache-friendly memory layout
- **SIMD Optimization**: OpenMP SIMD pragmas for vectorized dot product operations
- **Parallel Processing**: Multi-threaded JSON loading and search operations
- **Memory Layout**: Contiguous storage of embeddings, strings, and metadata

### Performance Targets
- **Load Performance**: <1 second for 100k documents
- **Search Performance**: <10ms for top-k similarity search
- **Memory Efficiency**: Minimal fragmentation via arena allocation
- **Scalability**: Designed for <1M embeddings

### API Design
The TypeScript API provides:
- Document loading from JSON directories
- Configurable embedding dimensions
- Cosine similarity search
- Batch normalization
- Thread-safe read operations

### Test Data Integration
Use the preprocessed JSON documents from the orchestrator research workspace (`/project_workspace/vector_store_research/out/`) as test data for validating the vector store implementation.


## MCP Server Integration

### Target Use Case
The native-vector-store package is designed for integration with MCP servers to provide fast local RAG capabilities. The typical workflow:

1. **Startup**: Load document corpus from JSON files
2. **Query Processing**: Accept embedding vectors from MCP server
3. **Similarity Search**: Return top-k most similar documents
4. **Response**: Provide document text and metadata for context

### Integration Pattern
```typescript
const store = new VectorStore(1536); // OpenAI embedding dimensions
store.loadDir('./documents'); // Load document corpus
const results = store.search(queryEmbedding, 5); // Top-5 similarity search
```

### Expected Performance
With the C++ implementation and SIMD optimizations, the package should provide:
- Fast cold-start performance for MCP server initialization
- Low-latency search responses for interactive use
- Memory-efficient storage for knowledge bases
