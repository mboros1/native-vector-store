# nvs-core

Native Vector Store — Rust core library.

What it provides:
- Bundle manifest parsing and validation
- Memory-mapped readers for vectors, metadata blocks, BM25 index
- Vector, BM25, and hybrid search via `nvs_core::VectorStore`

Quick start

```rust
use nvs_core::VectorStore;

let store = VectorStore::open("/path/to/bundle")?;
let hits = store.search_bm25("alpha beta", 10);
```

See `documentation/MANIFEST_SPEC.md` in the repository for the bundle format.

License: MIT

