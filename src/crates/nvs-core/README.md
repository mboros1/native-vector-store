# nvs-core

Native Vector Store — Rust core library for read-only vector + lexical search over packed bundles.

[![Crates.io](https://img.shields.io/crates/v/nvs-core.svg)](https://crates.io/crates/nvs-core)
[![Docs.rs](https://docs.rs/nvs-core/badge.svg)](https://docs.rs/nvs-core)

Features

- Compact on-disk bundle format with manifest and checksums
- Memory-mapped readers for vectors, metadata blocks, and BM25 index
- Fast vector, BM25, and hybrid search via `nvs_core::VectorStore`
- Structured document reads: `id`, `text`, `metadata` (JSON)
- Binary invariants enforced at open time:
  - Endianness is little (`manifest.endianness = "little"`)
  - Vector rows are padded to `files.vectors.row_alignment` (default 64 bytes)
  - Magic headers: `meta.idx` starts with `NVSIDX\x01`, `meta.blocks` starts with `NVSMETA\x01`
  - `meta.idx` entry = `{ u32 block_id, u32 offset, u32 doc_size, u32 reserved0 }`
  - `meta.blocks` header entries = `{ u32 comp_size, u32 decomp_size, u32 doc_count, u32 codec }`

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
let store = VectorStore::open("/path/to/bundle") ?;

// Hybrid search
let embedding: Vec<f32> = vec![0.0, 1.0, 0.0];
let q = "keywords";
let hits = store.search_hybrid( & embedding, q, 5, 0.6);

// Fetch full documents for the top-k
let ids: Vec<u32> = hits.iter().map( | (id, _) | * id).collect();
let docs = store.get_documents( & ids);

// Prefer parsed JSON metadata directly
let (id, text, meta) = store.bundle().get_document_value(ids[0]).unwrap();
println!("id={} meta_keys={}", id, meta.as_object().map(|m| m.len()).unwrap_or(0));
```

More

- Bundle format: https://github.com/martinboros/native-vector-store/blob/main/documentation/MANIFEST_SPEC.md
- CLI packer (build bundles): https://crates.io/crates/nvs-packer
- Lambda example (serverless
  query): https://github.com/martinboros/native-vector-store/tree/main/src/crates/nvs-lambda-sample

Bundle overview

```mermaid
flowchart TD
  M[manifest.json] -->|files.vectors| V[vectors.f32/f16 (row-aligned)]
  M -->|files.meta_idx| I[meta.idx (NVSIDX\x01)]
  M -->|files.meta| B[meta.blocks (NVSMETA\x01)]
  M -->|files.terms| T[terms.dict]
  M -->|files.lexicon| L[lexicon.bin]
  M -->|files.postings| P[postings.bin]
  M -->|files.doclen| D[doclen.u32]
```

License: MIT
