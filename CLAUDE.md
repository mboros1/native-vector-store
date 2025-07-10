# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Development Commands

### Build & Test
```bash
# Build the native module
npm run build

# Run all tests (includes Node.js and C++ tests)
make test

# Run only Node.js tests
npm test

# Run only C++ tests
make cpp-test

# Run performance benchmarks
npm run benchmark
# or
make benchmark
```

### Build System
- **Native Module**: Built with `node-gyp` using `binding.gyp`
- **C++ Testing**: Standalone tests via `src/Makefile`
- **CI Target**: `make check` runs complete test suite

### Development Workflow
```bash
# Clean build
make clean

# Full rebuild and test
make all && make test

# Package validation
make distcheck
```

## Design Decisions

### No-Exceptions Design
This codebase is built with **-fno-exceptions** for compatibility with Node.js native modules:
- **Error Handling**: Uses error codes instead of C++ exceptions
- **Allocation Failures**: Return nullptr or simdjson error codes
- **API Consistency**: All errors propagated through return values
- **Performance**: Avoids exception handling overhead

### Two-Phase Lifecycle Design
- **Loading Phase**: Concurrent document insertion using atomic counter, no searches allowed
- **Serving Phase**: No document insertion, concurrent searches allowed
- **Finalization**: Explicit transition normalizes embeddings and enables searches
- **No Race Conditions**: Phase separation eliminates all concurrency issues

### Producer-Consumer Loading Pattern
- **Sequential Disk I/O**: Single producer thread reads files to avoid I/O contention
- **Lock-Free Queue**: atomic_queue enables wait-free producer-consumer communication
- **Parallel Parsing**: Multiple consumer threads parse JSON concurrently
- **Atomic Allocation**: Document slots allocated via atomic counter increment

### Memory Management Philosophy
- **Arena Allocation**: 64MB chunks minimize fragmentation, improve cache locality
- **Zero-Copy Design**: String views into arena memory avoid allocations
- **Pre-Allocation**: entries_ vector sized upfront to avoid reallocation
- **Alignment Guarantees**: All allocations aligned for SIMD operations

## Architecture

### Core Components

**VectorStore (C++)**: High-performance vector similarity search engine
- **Arena Allocator**: 64MB chunks for cache-friendly memory layout
- **SIMD Optimization**: OpenMP pragmas for vectorized operations
- **Thread Safety**: Atomic operations for concurrent access

**Node.js Binding**: N-API wrapper exposing C++ functionality
- **File Loading**: Producer-consumer pattern with atomic_queue
- **Document Management**: Efficient conversion between JS and C++ objects
- **Search Interface**: Float32Array queries with normalized results

### Dependencies
- **atomic_queue**: Lock-free MPSC queue for producer-consumer pattern (header-only)
- **simdjson**: High-performance JSON parsing library
- **OpenMP**: Compiler support for parallel and SIMD operations

### Memory Layout
Documents stored in contiguous arena-allocated blocks:
```
[embedding][id][text][metadata_json]
```

Each entry contains:
- `float* embedding`: Direct pointer for SIMD operations
- `Document doc`: String views into arena memory
- Atomic reference counting for thread safety

### Performance Targets
- **Load**: <1s for 100k documents (achieved: ~560ms for 100k docs)
- **Search**: <10ms for similarity queries (achieved: 1-2ms for 10k corpus)
- **Memory**: Minimal fragmentation via arena allocation
- **Scale**: Designed for <1M embeddings
- **Throughput**: 178k+ documents/second with producer-consumer loading

## Key Implementation Details

### SIMD Operations
- **Dot Product**: `#pragma omp simd reduction(+:score)`
- **Normalization**: Vectorized sqrt and multiplication
- **Parallel Search**: OpenMP threading across document corpus

### JSON Processing
- **simdjson**: High-performance streaming parser
- **Error Handling**: Comprehensive error codes for parsing failures
- **Batch Loading**: Producer-consumer pattern for optimal I/O and CPU utilization

### LoadDir Implementation
- **Producer Thread**: Sequential file reading to maximize disk throughput
- **Consumer Pool**: `std::thread::hardware_concurrency()` workers for JSON parsing
- **Bounded Queue**: 1024 capacity to limit memory usage (~100MB buffered data)
- **Array Support**: Handles both single documents and document arrays per file

### Cross-Platform Build
- **macOS**: Homebrew OpenMP and simdjson
- **Linux**: Standard libomp and simdjson
- **Windows**: MSVC OpenMP support

## Implementation Constraints

### Maximum Capacity
- **Document Limit**: 1M documents (pre-allocated in entries_ vector)
- **Allocation Size**: Maximum 64MB per allocation (chunk size limit)
- **Embedding Dimensions**: No hard limit, but designed for <10k dimensions
- **Alignment**: Maximum 4096 bytes, must be power of 2

### Error Handling Patterns
```cpp
// Allocation failures return nullptr
void* ptr = arena_.allocate(size, align);
if (!ptr) {
    return simdjson::MEMALLOC;
}

// All API methods return simdjson::error_code
auto error = store.add_document(doc);
if (error) {
    // Handle error without exceptions
}
```

### Concurrency Guarantees
- **Loading Phase**: Thread-safe concurrent insertion via atomic counter
- **Serving Phase**: Unlimited concurrent searches, no modifications allowed
- **Phase Transition**: `finalize()` provides memory barrier for safe transition
- **Search Parallelism**: OpenMP used internally for parallel score computation
- **Memory Safety**: Arena allocator uses mutex for chunk creation, atomic ops for allocation

## Testing Strategy

### Test Categories
1. **Unit Tests**: Basic functionality (add, search, normalize)
2. **Integration Tests**: File loading and MCP server examples
3. **Performance Tests**: Load time and search latency benchmarks
4. **Error Handling**: Malformed JSON and dimension mismatches
5. **Stress Tests**: Concurrent operations with sanitizer support

### Running Stress Tests
```bash
cd src

# Default: Run with AddressSanitizer (ASAN)
make stress

# Run with ThreadSanitizer (TSAN)
make stress SANITIZER=thread

# Run without sanitizers (faster but less checking)
make stress SANITIZER=none

# Or use specific targets
make test-asan    # Force ASAN
make test-tsan    # Force TSAN
```

### Test Data
- **Synthetic**: Generated embeddings for consistent benchmarking
- **Real Data**: JSON documents with OpenAI-style embeddings
- **Edge Cases**: Empty documents, large batches, boundary conditions

## MCP Integration

The vector store is designed for Model Context Protocol servers:
- **Fast Context Retrieval**: Sub-10ms similarity search
- **Efficient Loading**: Parallel document processing
- **Memory Management**: Arena allocation prevents fragmentation
- **TypeScript Support**: Full type definitions in `lib/index.d.ts`

Example MCP server implementation in `examples/mcp-server.js` demonstrates:
- Document corpus loading
- Query handling with similarity thresholds
- Error handling and logging