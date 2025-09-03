# Test Files

## Performance Test (perf_test.cpp)
- Benchmarks PDF parsing performance with different thread counts
- Tests scaling from 1 to N threads
- Measures pages/second throughput
- Usage: `make perf-test && ./perf-test input.pdf`

## Token Test (token_test.cpp)
- Tests the tiktoken tokenizer implementation
- Validates token counting accuracy
- Compares with estimated token counts
- Usage: `make token-test && ./token-test`