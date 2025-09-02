# Native Vector Store – Rust Workspace

This workspace hosts the Rust rewrite alongside the existing C++ implementation.

Crates:
- crates/nvs-core: Core reader library (manifest parsing, mmap files, BM25/vector search – WIP)

See documentation/MANIFEST_SPEC.md and documentation/RUST_PARITY.md in the repo root for format and parity notes.

Build & test:
- From `rust/`: `cargo build`, `cargo test` (requires network to fetch deps on first run).

Status:
- Reader skeleton implemented with manifest + bundle-file validation:
  - Validates `files.meta.block_size` vs derived block size in `meta.blocks`.
  - Validates `meta.idx` entry count matches `manifest.num_docs`.
- Search APIs and full mmap-backed reading are planned next.
