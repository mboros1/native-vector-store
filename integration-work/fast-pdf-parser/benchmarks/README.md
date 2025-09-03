# Benchmarks

## Tokenizer Performance Benchmark (benchmark_passes.cpp)
- Measures tiktoken tokenizer performance at various scales
- Tests from 10 to 1000 pages of generated content
- Reports tokens/second and MB/second throughput
- Usage: `make benchmark-passes && ./benchmark-passes`

### Results
- Consistent ~3.4M tokens/second throughput
- ~17 MB/second text processing
- Linear scaling with document size