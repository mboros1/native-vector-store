# nvs-html

HTML chunking orchestrator: extract sections (nvs-html-core), then produce semantic chunks (nvs-core chunker).

API

- `parse_to_chunks(path, &HtmlChunkOptions)` → `Vec<Chunk>`
- `parse_to_chunks_with_stats(path, &HtmlChunkOptions)` → `(Vec<Chunk>, ChunkStats)`
- `write_chunks_json(path, chunks, out)` → chunks array with `mimetype:"text/html"` header

Example

```rust
let path = std::path::Path::new("/path/file.html");
let chunks = nvs_html::parse_to_chunks(path, &nvs_html::HtmlChunkOptions::default())?;
nvs_html::write_chunks_json(path, &chunks, std::path::Path::new("/tmp/chunks.json"))?;
```

Pipeline

```mermaid
flowchart LR
  H[HTML] --> X[Extract sections]
  X --> C[Chunk pages]
  C --> J[Write chunks.json]
```

License: MIT

