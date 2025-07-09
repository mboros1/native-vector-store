# Worker CLAUDE.md - native-vector-store

**Branch**: feature/initial-npm-package
**Private Branch**: private/worker_vector_store/Initial_npm_package_implementation
**Assignment PR**: #1
**Started**: 2025-07-09

## Project Context

Native C++ vector store implementation for Node.js, designed for high-performance similarity search operations with SIMD optimizations. Target use case is MCP server integration for fast local RAG capabilities.

## My Task Understanding

Implement a high-performance vector store as an npm package with:
- C++ core with Node.js bindings via node-gyp
- SIMD-optimized similarity search
- JSON document loading and management
- TypeScript definitions for API
- Cross-platform compatibility (macOS/Linux)

## Technical Approach

1. **Core Implementation**: C++ with simdjson for fast JSON parsing
2. **Memory Management**: Custom arena allocator for cache-friendly layout
3. **SIMD Optimization**: SSE/AVX intrinsics for dot product operations
4. **Parallelization**: OpenMP for multi-threaded operations (with caveats)
5. **Build System**: node-gyp for cross-platform builds

## Progress Log

### Phase 1: Initial Implementation
- ✅ C++ core with SIMD-optimized similarity search
- ✅ Node.js bindings using node-addon-api
- ✅ TypeScript definitions
- ✅ Basic test suite

### Phase 2: Performance Optimization
- ✅ Replaced naive JSON parsing with simdjson
- ✅ Implemented parallel document loading
- ✅ Added comprehensive benchmarking
- ✅ Optimized memory layout for cache efficiency

### Phase 3: Production Readiness
- ✅ Cross-platform build configuration
- ✅ CI/CD with GitHub Actions
- ✅ npm package configuration
- ✅ Example MCP server integration

## Critical Lessons Learned

### 1. OpenMP and Node.js Compatibility Issues

**Problem**: Segmentation faults when using OpenMP parallel regions in Node.js bindings.

**Root Cause**: Node.js runs JavaScript in a single-threaded event loop with its own thread pool for async operations. OpenMP's thread spawning conflicts with Node.js's threading model, causing crashes during V8 garbage collection or callback execution.

**Solution**: 
- Remove `#pragma omp parallel for` from all methods called via Node.js bindings
- Keep OpenMP only for SIMD directives (`#pragma omp simd`)
- Use Node.js worker threads or async patterns for parallelism instead

**Code Example**:
```cpp
// BAD - causes segfaults in Node.js
#pragma omp parallel for
for (size_t i = 0; i < documents.size(); i++) {
    // process documents
}

// GOOD - safe for Node.js
#pragma omp simd
for (size_t j = 0; j < dimensions; j++) {
    dot_product += vec1[j] * vec2[j];
}
```

### 2. Benchmarking Pitfalls: JS Object Creation vs I/O

**Problem**: Initial benchmarks showed 400ms to load 100k documents, but this was misleading.

**Discovery**: The benchmark was measuring JavaScript object creation time, not actual file I/O:
```javascript
// This was being benchmarked
for (let i = 0; i < 100000; i++) {
    docs.push({
        text: `Document ${i}`,
        embedding: Array(1536).fill(Math.random()),
        metadata: { id: i }
    });
}
```

**Real Performance**: When loading from actual JSON files:
- 100 documents: ~10ms
- 10,000 documents: ~500ms  
- 100,000 documents: ~6 seconds (due to actual file I/O)

**Lesson**: Always benchmark real-world scenarios with actual file I/O, not synthetic data generation.

### 3. Cross-Platform Build Complexity

**macOS vs Linux Differences**:
- **Compiler**: macOS uses clang, Linux uses gcc
- **OpenMP Library**: macOS needs `-lomp`, Linux needs `-lgomp`
- **Library Paths**: macOS OpenMP in `/opt/homebrew`, Linux in system paths

**Solution in binding.gyp**:
```python
'conditions': [
    ['OS=="mac"', {
        'xcode_settings': {
            'OTHER_CFLAGS': ['-fopenmp'],
            'OTHER_LDFLAGS': ['-lomp', '-L/opt/homebrew/opt/libomp/lib']
        }
    }],
    ['OS=="linux"', {
        'cflags': ['-fopenmp'],
        'ldflags': ['-lgomp']
    }]
]
```

### 4. simdjson Move-Only Types and Error Handling

**Challenge**: simdjson uses move-only types that can't be copied, and operations return `simdjson_result<T>` that must be checked.

**Pattern That Works**:
```cpp
auto doc_result = parser.iterate(json_data, json_length, json_capacity);
if (doc_result.error()) {
    return false;
}
ondemand::document doc = std::move(doc_result).value();

// For accessing values, use .get() which returns simdjson_result
auto array_result = doc.get_array();
if (array_result.error()) {
    return false;
}

for (auto elem : array_result.value()) {
    // Process elements
}
```

**Key Points**:
- Always check `.error()` before accessing `.value()`
- Use `std::move()` when extracting values from results
- Can't store simdjson objects; must process immediately

### 5. CI/CD Best Practices for Native Modules

**Working GitHub Actions Configuration**:
```yaml
strategy:
  matrix:
    os: [ubuntu-latest, macos-latest]
    node: [18.x, 20.x]
    
steps:
- name: Install OpenMP (Ubuntu)
  if: matrix.os == 'ubuntu-latest'
  run: sudo apt-get install -y libomp-dev

- name: Install OpenMP (macOS)
  if: matrix.os == 'macos-latest'
  run: brew install libomp

- name: Build
  run: npm install && npm run build

- name: Test
  run: npm test
```

**Key Insights**:
- Test on multiple OS and Node versions
- Install system dependencies before npm install
- Use matrix builds for comprehensive coverage
- Cache node_modules but not build artifacts

### 6. Thread Safety Already Built In

**Discovery**: The VectorStore implementation already has thread safety through atomic operations:
```cpp
std::atomic<size_t> document_count{0};
std::atomic<bool> is_normalized{false};
```

**Implication**: No additional synchronization needed for:
- Concurrent reads after loading
- Status checks (is_normalized, document_count)
- Search operations

**Note**: Loading should still be done from a single thread to avoid race conditions in vector resizing.

## Performance Achievements

- **Load Time** (10k documents): ~500ms
- **Search Time** (10k documents): <1ms for top-5 similarity
- **Memory Usage**: ~200MB for 10k documents with 1536-dim embeddings
- **SIMD Speedup**: ~4x over scalar implementation

## Architecture Decisions That Paid Off

1. **simdjson Integration**: 10x faster than naive JSON parsing
2. **Contiguous Memory Layout**: Better cache utilization
3. **SIMD Optimizations**: Significant speedup for similarity calculations
4. **Minimal Dependencies**: Only simdjson as external dependency
5. **Clean API Design**: Simple, intuitive TypeScript interface

## Future Considerations

1. **GPU Acceleration**: For massive datasets (>1M documents)
2. **Quantization**: Reduce memory usage with int8 embeddings
3. **Persistent Storage**: Memory-mapped files for large corpora
4. **Distributed Search**: Sharding across multiple instances

---

**IMPORTANT**: This file should NEVER be committed to your feature branch!