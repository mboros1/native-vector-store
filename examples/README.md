# MCP Examples

This directory contains example MCP (Model Context Protocol) servers using the native vector store.

## C++ Reference MCP Server

A powerful MCP server that provides semantic search over the C++ standard library documentation.

### Features

- **Semantic Search**: Natural language queries to find relevant C++ documentation
- **Category Search**: Search within specific categories (containers, algorithms, etc.)
- **Section Retrieval**: Get specific sections by identifier
- **Real-time Embeddings**: Uses OpenAI embeddings for accurate semantic search

### Setup

1. **Generate C++ Documentation Embeddings**
   ```bash
   # First, download C++ standard documentation
   # Then generate embeddings:
   node scripts/generate_embeddings.js cpp_std_embeddings cpp_std_embedded
   ```

2. **Set Environment Variables**
   Create a `.env` file:
   ```env
   OPENAI_API_KEY=your-api-key-here
   CPP_DOCS_PATH=/path/to/cpp_std_embedded  # Optional if in default location
   ```

3. **Configure Claude Desktop**
   
   Add to your Claude Desktop config (`~/Library/Application Support/Claude/claude_desktop_config.json` on macOS):
   
   ```json
   {
     "mcpServers": {
       "cpp-reference": {
         "command": "node",
         "args": [
           "/absolute/path/to/native-vector-store/examples/mcp-cpp-reference-openai.js"
         ],
         "env": {
           "OPENAI_API_KEY": "your-openai-api-key"
         }
       }
     }
   }
   ```

### Available Tools

#### `search_cpp_reference`
Search the C++ standard library documentation with natural language queries.

Example queries:
- "How do I use std::vector?"
- "What is move semantics?"
- "Explain SFINAE"
- "How to use std::async for parallel processing"

#### `search_cpp_by_category`
Search within specific categories:
- containers
- algorithms
- memory
- concurrency
- ranges
- concepts
- utilities
- io
- numerics

#### `get_cpp_section`
Retrieve a specific section by its identifier.

### Testing Locally

You can test the MCP server directly:

```bash
# With OpenAI (recommended)
node examples/mcp-cpp-reference-openai.js

# Without OpenAI (demo mode with random embeddings)
node examples/mcp-cpp-reference.js
```

Then interact with it using the MCP Inspector or integrate with Claude Desktop.

### Performance

- Loads ~6,500 C++ documentation sections in <1 second
- Search latency: 1-3ms per query
- Memory efficient with arena allocation

### Troubleshooting

1. **"C++ documentation not found"**
   - Run the embedding generation script
   - Set `CPP_DOCS_PATH` environment variable

2. **"OPENAI_API_KEY not set"**
   - Add your OpenAI API key to `.env` file
   - Or use the demo version without real embeddings

3. **"Vector store not initialized"**
   - Check server logs for initialization errors
   - Ensure C++ docs are properly generated

## Other Examples

- `mcp-server.js`: Original MCP server example for general document search
- More examples coming soon!

## Contributing

Feel free to contribute more MCP server examples showcasing different use cases of the native vector store!