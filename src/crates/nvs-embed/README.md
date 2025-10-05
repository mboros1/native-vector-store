# nvs-embed

Embedding utilities and backends for turning chunked text into dense vectors.

Backends

- Local GTE-small backend (CPU-only) using candle + tokenizers.
  - Default feature: `local-embed` (enabled).
  - Model files: discovered under `src/models/gte-small/` or via `NVS_LOCAL_EMBED_MODEL_DIR`.
- OpenAI backend for remote embeddings (requires `OPENAI_API_KEY`).

API overview

- Create a backend
  - Local: `let be = LocalGTEBackendBuilder::new().build()?;`
  - OpenAI: `let be = OpenAIBackend::builder("text-embedding-3-small").build()?;`
- Batch embed texts: `backend.embed_batch(&["text a", "text b"]).await?` → `Vec<Vec<f32>>`
- Chunk embedding helpers:
  - `embed_chunks_file(input_chunks.json, output_docs.json, opts)`
  - `embed_chunks_dir(input_dir, output_dir, opts)`

Example (embed chunks file → docs.json)

```rust
use std::sync::Arc;
use nvs_embed::{EmbedOptions, LocalGTEBackendBuilder};

let backend = LocalGTEBackendBuilder::new().build()?;
let opts = EmbedOptions { concurrency: 8, batch_size: 16, file_concurrency: 1, total_concurrency: 8 };
tokio::runtime::Runtime::new()?.block_on(async move {
    nvs_embed::embed_chunks_file(
        Arc::new(backend),
        std::path::Path::new("./chunks.json"),
        std::path::Path::new("./docs.json"),
        &opts,
    ).await
})?;
```

Notes

- Output docs have shape `{ id?, text, metadata: { embedding: [f32], ... } }`, ready for the packer.
- The local backend truncates to a reasonable max token length for speed; adjust in the builder as needed.

License: MIT

