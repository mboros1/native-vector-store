# fast-pdf-parser

A high-performance PDF text extraction and chunking library with Node.js bindings. Built with C++ and MuPDF for maximum performance.

[![npm version](https://badge.fury.io/js/fast-pdf-parser.svg)](https://badge.fury.io/js/fast-pdf-parser)
[![CI](https://github.com/mboros1/fast-pdf-parser/actions/workflows/ci.yml/badge.svg)](https://github.com/mboros1/fast-pdf-parser/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

## Features

- 🚀 **High Performance**: ~35-250 pages/second processing speed
- 🧵 **Multi-threaded**: Utilizes all CPU cores for parallel processing
- 📊 **Smart Chunking**: 7-pass hierarchical algorithm for optimal text chunks
- 🎯 **Consistent Output**: Eliminates bimodal chunk size distribution
- 🔍 **Semantic Preservation**: Maintains document structure and boundaries
- 💾 **Memory Efficient**: Streaming processing for large documents
- 🔧 **Native Bindings**: Pre-built binaries for major platforms

## Installation

```bash
npm install fast-pdf-parser
```

The package includes pre-built binaries for:
- macOS (x64, arm64)
- Linux (x64, arm64)
- Windows (x64)

## Quick Start

```javascript
const { chunkPdf } = require('fast-pdf-parser');

// Simple usage with default options
const result = chunkPdf('document.pdf');
console.log(`Created ${result.totalChunks} chunks from ${result.totalPages} pages`);

// Access the chunks
result.chunks.forEach(chunk => {
    console.log(`Chunk: ${chunk.tokenCount} tokens`);
    console.log(`Text: ${chunk.text.substring(0, 100)}...`);
});
```

## API Reference

### `HierarchicalChunker`

The main class for PDF chunking operations.

```javascript
const { HierarchicalChunker } = require('fast-pdf-parser');

const chunker = new HierarchicalChunker({
    maxTokens: 512,      // Maximum tokens per chunk (default: 512)
    minTokens: 150,      // Minimum tokens per chunk (default: 150)
    overlapTokens: 0,    // Token overlap between chunks (default: 0)
    threadCount: 4       // Number of worker threads (default: CPU count)
});
```

#### Methods

##### `chunkFile(pdfPath, pageLimit?)`

Process a PDF file and return chunks.

```javascript
const result = chunker.chunkFile('document.pdf', 100); // Process first 100 pages
```

Returns:
```typescript
{
    chunks: ChunkResult[],      // Array of chunks
    totalPages: number,         // Total pages processed
    totalChunks: number,        // Total chunks created
    processingTimeMs: number    // Processing time in milliseconds
}
```

##### `getOptions()` / `setOptions(options)`

Get or update chunking options.

```javascript
const options = chunker.getOptions();
chunker.setOptions({ maxTokens: 400 });
```

### `chunkPdf(pdfPath, options?)`

Convenience function for one-shot chunking.

```javascript
const result = chunkPdf('document.pdf', {
    maxTokens: 400,
    minTokens: 100,
    pageLimit: 50    // Optional: limit pages to process
});
```

## Chunk Output Format

Each chunk contains:

```typescript
{
    text: string,              // The chunk text content
    tokenCount: number,        // Number of tokens in the chunk
    startPage: number,         // Starting page (0-based)
    endPage: number,           // Ending page (0-based)
    hasMajorHeading: boolean,  // Whether chunk contains a major heading
    minHeadingLevel: number    // Minimum heading level in chunk
}
```

## Performance

Performance varies based on PDF complexity:

| Document Type | Pages/Second | Notes |
|--------------|--------------|--------|
| Text-heavy PDFs | 30-50 | Academic papers, books |
| Mixed content | 50-150 | Reports with images |
| Simple PDFs | 150-250 | Mostly text, simple layout |

## Advanced Usage

### Processing Multiple PDFs

```javascript
const { HierarchicalChunker } = require('fast-pdf-parser');
const fs = require('fs').promises;
const path = require('path');

async function processDirectory(dirPath) {
    const chunker = new HierarchicalChunker({
        maxTokens: 512,
        threadCount: 8
    });
    
    const files = await fs.readdir(dirPath);
    const pdfFiles = files.filter(f => f.endsWith('.pdf'));
    
    for (const file of pdfFiles) {
        const filePath = path.join(dirPath, file);
        const result = chunker.chunkFile(filePath);
        
        console.log(`${file}: ${result.totalChunks} chunks in ${result.processingTimeMs}ms`);
        
        // Save chunks to JSON
        await fs.writeFile(
            `${filePath}.chunks.json`,
            JSON.stringify(result, null, 2)
        );
    }
}
```

### Custom Token Sizes

```javascript
// For smaller chunks (e.g., for embedding models)
const embeddingChunker = new HierarchicalChunker({
    maxTokens: 256,
    minTokens: 50,
    overlapTokens: 25  // 25 token overlap for context
});

// For larger chunks (e.g., for LLM context)
const contextChunker = new HierarchicalChunker({
    maxTokens: 2048,
    minTokens: 512,
    threadCount: 16  // Use more threads for larger documents
});
```

## System Requirements

### Prerequisites

MuPDF must be installed on your system if building from source:

**macOS:**
```bash
brew install mupdf-tools
```

**Ubuntu/Debian:**
```bash
sudo apt-get install libmupdf-dev
```

**Windows:**
Use vcpkg or download MuPDF development files.

### Node.js

- Node.js >= 14.0.0
- Platforms: macOS, Linux, Windows
- Architectures: x64, arm64

## Building from Source

If pre-built binaries are not available for your platform:

```bash
# Clone the repository
git clone https://github.com/mboros1/fast-pdf-parser.git
cd fast-pdf-parser

# Install dependencies
npm install

# Build the native module
npm run build

# Run tests
npm test
```

## TypeScript Support

TypeScript definitions are included. The package exports the following types:

```typescript
interface ChunkOptions {
    maxTokens?: number;
    minTokens?: number;
    overlapTokens?: number;
    threadCount?: number;
}

interface ChunkResult {
    text: string;
    tokenCount: number;
    startPage: number;
    endPage: number;
    hasMajorHeading: boolean;
    minHeadingLevel: number;
}

interface ChunkingResult {
    chunks: ChunkResult[];
    totalPages: number;
    totalChunks: number;
    processingTimeMs: number;
}

class HierarchicalChunker {
    constructor(options?: ChunkOptions);
    chunkFile(pdfPath: string, pageLimit?: number): ChunkingResult;
    getOptions(): ChunkOptions;
    setOptions(options: ChunkOptions): void;
}

function chunkPdf(
    pdfPath: string, 
    options?: ChunkOptions & { pageLimit?: number }
): ChunkingResult;
```

## Troubleshooting

### Common Issues

1. **Module not found**: Ensure MuPDF is installed on your system
2. **Build errors**: Check that you have a C++17 compatible compiler
3. **Performance issues**: Adjust `threadCount` based on your CPU cores

### Debug Output

The library outputs debug information when processing:
```
[TextExtractor::get_page_count] Document has 1366 pages
Starting to process 1366 pages with 10 threads
[TextExtractor::extract_page] Extracting page 0
```

## Contributing

Contributions are welcome! Please see the [GitHub repository](https://github.com/mboros1/fast-pdf-parser) for:
- Issue tracking
- Pull requests
- Development setup

## License

MIT License - see LICENSE file for details.

## Acknowledgments

Built with:
- [MuPDF](https://mupdf.com/) - PDF rendering library
- [tiktoken](https://github.com/openai/tiktoken) - OpenAI's tokenizer
- [node-addon-api](https://github.com/nodejs/node-addon-api) - Node.js native addon API