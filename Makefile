# Root Makefile for CI/CD
.PHONY: all build test clean install check distcheck

# Default target
all: build

# Build the Node.js module
build:
	npm run build

# Run all tests
test: build
	npm test
	node test/loader_test.js
	node test/create_benchmark_data.js
	node test/benchmark_parallel.js
	cd src && make && ./test_vector_store ../test

# Clean build artifacts
clean:
	npm run clean
	cd src && make clean
	rm -rf build/
	rm -rf node_modules/

# Install dependencies
install:
	npm install

# Check target for CI
check: test

# Distribution check
distcheck:
	npm run clean
	cd src && make clean
	rm -rf build/
	npm pack
	@echo "Package created successfully"
	tar -tzf native-vector-store-*.tgz | head -20
	rm -f native-vector-store-*.tgz

# C++ only targets
cpp-test:
	cd src && make clean && make && ./test_vector_store ../test

# Performance benchmark
benchmark: build
	node test/test.js

# Help
help:
	@echo "Available targets:"
	@echo "  all        - Build the project (default)"
	@echo "  build      - Build the Node.js native module"
	@echo "  test       - Run all tests"
	@echo "  clean      - Clean build artifacts"
	@echo "  install    - Install dependencies"
	@echo "  check      - Run tests (CI target)"
	@echo "  distcheck  - Check distribution package"
	@echo "  cpp-test   - Run C++ tests only"
	@echo "  benchmark  - Run performance benchmarks"
	@echo "  help       - Show this help message"