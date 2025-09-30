# nvs-core

Native Vector Store — Rust core library for read-only vector + lexical search over packed bundles.

[![Crates.io](https://img.shields.io/crates/v/nvs-core.svg)](https://crates.io/crates/nvs-core)
[![Docs.rs](https://docs.rs/nvs-core/badge.svg)](https://docs.rs/nvs-core)

Features

- Compact on-disk bundle format with manifest and checksums
- Memory-mapped readers for vectors, metadata blocks, and BM25 index
- Fast vector, BM25, and hybrid search via `nvs_core::VectorStore`
- Structured document reads: `id`, `text`, `metadata` (JSON)

Install

Add to `Cargo.toml`:

```toml
[dependencies]
nvs-core = "0.1"
```

Quick start

```rust
use nvs_core::VectorStore;

// Open a bundle (directory containing manifest.json, vectors, meta.* etc.)
let store = VectorStore::open("/path/to/bundle")?;

// Hybrid search
let embedding: Vec<f32> = vec![0.0, 1.0, 0.0];
let q = "keywords";
let hits = store.search_hybrid(&embedding, q, 5, 0.6);

// Fetch full documents for the top-k
let ids: Vec<u32> = hits.iter().map(|(id, _)| *id).collect();
let docs = store.get_documents(&ids);
```

More

- Bundle format: https://github.com/martinboros/native-vector-store/blob/main/documentation/MANIFEST_SPEC.md
- CLI packer (build bundles): https://crates.io/crates/nvs-packer
- Lambda example (serverless
  query): https://github.com/martinboros/native-vector-store/tree/main/src/crates/nvs-lambda-sample

License: MIT
