# Native Vector Store Testing Plan

## Overview
Integration of doctest for comprehensive unit testing across all C++ source files.

## Testing Strategy

### 1. Test Organization

#### Embedded Tests (Preferred)
- Place tests directly in implementation files (.cpp)
- Tests serve as living documentation near the code they test
- Use `DOCTEST_CONFIG_DISABLE` for release builds to exclude tests

#### Separate Test Files (When Needed)
- For integration tests that span multiple components
- For performance benchmarks that shouldn't be in production code
- Named as `*_test.cpp` in the src/ directory

### 2. Implementation Approach

#### Phase 1: Core Infrastructure
- [ ] Create test runner executable (`src/test_runner.cpp`)
- [ ] Update Makefile to support test compilation modes
- [ ] Add test target that compiles with tests enabled

#### Phase 2: Unit Test Coverage
Priority order for adding tests:

1. **nvs_pack.cpp** (Critical - has metadata corruption bug)
   - Test metadata block writing
   - Test index generation
   - Test manifest creation
   - Test file I/O operations

2. **arena_allocator.h**
   - Test allocation/deallocation
   - Test alignment requirements
   - Test chunk management
   - Test thread safety

3. **vector_store.cpp**
   - Test document addition
   - Test search functionality
   - Test normalization
   - Test phase transitions

4. **vector_store_v2.cpp**
   - Test bundle loading
   - Test mmap operations
   - Test search with loaded data

5. **binding.cpp**
   - Test N-API conversions
   - Test error handling
   - Test async operations

### 3. Test Categories

#### Unit Tests
- Function-level testing
- Edge cases and error conditions
- Data structure integrity

#### Integration Tests
- Multi-component workflows
- File I/O operations
- End-to-end scenarios

#### Performance Tests
- Benchmark critical paths
- Memory usage validation
- Threading performance

### 4. Test Patterns

#### Basic Test Structure
```cpp
// In implementation file (e.g., nvs_pack.cpp)

// Exclude main() when tests are enabled
#ifndef NVS_ENABLE_INLINE_TESTS
int main(int argc, char* argv[]) {
    // Main implementation
}
#endif

// Add tests at end of file
#ifndef DOCTEST_CONFIG_DISABLE
#include "../deps/doctest.h"

TEST_CASE("NVSPack metadata generation") {
    SUBCASE("creates valid block headers") {
        // Test implementation
        CHECK(block.block_id == expected_id);
    }
    
    SUBCASE("handles doc alignment correctly") {
        // Test implementation
        REQUIRE(block.doc_count > 0);
    }
}
#endif
```

#### Test Fixtures for Stateful Testing
```cpp
class VectorStoreFixture {
    VectorStore store;
    std::vector<Document> test_docs;
    
public:
    VectorStoreFixture() {
        // Setup test data
    }
};

TEST_CASE_FIXTURE(VectorStoreFixture, "Vector store operations") {
    // Tests using fixture data
}
```

### 5. Build Configuration

#### Development Build (with tests)
```bash
make unit-tests  # Compiles with NVS_ENABLE_INLINE_TESTS and runs tests
make build-tests # Just builds tests without running
```

#### Release Build (no tests)
```bash
make nvs-pack    # Normal build without inline tests
```

#### Continuous Testing
```bash
make watch-test  # Re-runs tests on file changes
```

### 6. Assertions Guidelines

- Use `CHECK()` for non-critical validations that should continue
- Use `REQUIRE()` for critical validations that should stop the test
- Use `CHECK_NOTHROW()` for exception safety testing
- Use `CHECK_THROWS_AS()` for error condition testing

### 7. Coverage Goals

- Minimum 80% code coverage for critical paths
- 100% coverage for public APIs
- Focus on edge cases and error conditions
- Document untestable code with explanations

### 8. Migration Path

1. Start with new code - all new functions get tests
2. Add tests when fixing bugs (regression tests)
3. Gradually add tests to existing code by priority
4. Refactor code for testability where needed

## Implementation Timeline

- **Week 1**: Core infrastructure, test runner, Makefile updates
- **Week 2**: nvs_pack.cpp tests (fix metadata bug)
- **Week 3**: arena_allocator.h and vector_store.cpp tests
- **Week 4**: vector_store_v2.cpp and binding.cpp tests
- **Ongoing**: Add tests with new features and bug fixes

## Success Metrics

- All builds pass tests before merge
- Reduced bug discovery in production
- Faster development through test-driven debugging
- Better code documentation through test examples