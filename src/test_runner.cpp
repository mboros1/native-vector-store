// Test runner for Native Vector Store
// This file provides the main() function for doctest
// All other test files should just include doctest.h

#define DOCTEST_CONFIG_IMPLEMENT_WITH_MAIN
#include "../deps/doctest.h"

// The DOCTEST_CONFIG_IMPLEMENT_WITH_MAIN macro generates a main() function
// that runs all registered tests and provides command-line options like:
//
// ./test_runner                    # Run all tests
// ./test_runner -tc="*metadata*"   # Run tests matching pattern
// ./test_runner -s                 # Show successful tests
// ./test_runner -r=xml             # Output in XML format
// ./test_runner -h                 # Show help