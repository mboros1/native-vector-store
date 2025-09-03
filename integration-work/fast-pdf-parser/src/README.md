# Source Code Organization

## Hierarchical Chunker (hierarchical_chunker.cpp)
- Main implementation using 7-pass hierarchical chunking algorithm
- Uses **nlohmann/json** for consistent JSON handling
- Eliminates bimodal distribution problem
- Produces chunks primarily in 400-550 token range
- Compilation: `make hierarchical-chunker`
- Usage: `./hierarchical-chunker input.pdf [max_tokens] [overlap] [page_limit]`

### Core Dependencies:
- **fast_pdf_parser.cpp** - PDF document handling and page iteration
- **thread_pool.cpp** - Multi-threaded page processing
- **text_extractor.cpp** - MuPDF-based text extraction from PDF pages

## Architecture
The implementation follows a clean separation:
1. PDF parsing layer (MuPDF integration)
2. Text extraction with structured output
3. Hierarchical chunking with semantic boundaries
4. JSON serialization for output