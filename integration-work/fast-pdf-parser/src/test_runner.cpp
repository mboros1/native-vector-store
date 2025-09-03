// Test runner for fast-pdf-parser
// This file provides the main() function for doctest
// All other test files should just include doctest.h

#define DOCTEST_CONFIG_IMPLEMENT_WITH_MAIN
#include "../deps/doctest.h"

// The DOCTEST_CONFIG_IMPLEMENT_WITH_MAIN macro generates a main() function
// that runs all registered tests and provides command-line options like:
//
// ./test_runner                    # Run all tests
// ./test_runner -tc="*thread*"     # Run tests matching pattern
// ./test_runner -s                 # Show successful tests
// ./test_runner -r=xml             # Output in XML format
// ./test_runner -h                 # Show help
//
// To compile:
// clang++ -std=c++17 -O2 -pthread \
//   -I../include -I../deps \
//   test_runner.cpp thread_pool.cpp hierarchical_chunker.cpp \
//   cl100k_base_data.cpp -o test_runner