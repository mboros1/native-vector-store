# Native Vector Store — Rust Tooling

This repository includes a Rust workspace under `rust/` that implements the next‑gen Native Vector Store: bundle format readers/writers, a packer CLI, a PDF text chunker, and supporting utilities.

If you’re here to build, test, or use the Rust code, start with this guide.

## Quick Start

- `cd src`
- Build workspace: `cargo build`
- Run tests: `cargo test`
- Lint: `cargo clippy --all-targets --all-features -- -D warnings`
- Format: `cargo fmt`

Rust stable is recommended (via `rustup default stable`).

## Workspace Layout

- `crates/nvs-core` — Core reader library for the bundle format (manifest parsing, mmap access, BM25/vector search – in progress).
- `crates/nvs-packer` — CLI to convert JSON docs with embeddings into a Native Vector Store bundle.
- `crates/nvs-pdf-core` — Fast-path, text‑only PDF utilities (low‑level parser/prober for future extraction paths).
- `crates/nvs-pdf` — Library for PDF extraction + chunking (TokenMonster tokenizer), with optional PDFium integration.
- `crates/tokenmonster` — Greedy tokenizer and embedded vocabulary used by the PDF chunker.

Workspace manifest: `rust/Cargo.toml`.

## Toolchain & Prerequisites

- Rust: latest stable toolchain (Edition 2021). Install with `rustup`.
- macOS/Linux are primary targets; Windows may work but isn’t validated yet.
- For PDF features (optional):
  - Bundled PDFium download requires `curl` and `tar` available on PATH.
  - System PDFium requires the library installed and discoverable (see below).

## Building Crates

- Entire workspace: `cargo build` (from `rust/`).
- Specific crate: `cargo build -p <crate>` (e.g., `cargo build -p nvs-packer`).
- Tests: `cargo test` or `cargo test -p <crate>`.
- Benchmarks (Criterion): `cargo bench -p nvs-core`.

Enable backtraces when debugging: `RUST_BACKTRACE=1 cargo test`.

## nvs-packer (CLI)

Pack JSON docs (with embeddings) into a Native Vector Store bundle.

Basic usage:

- `cargo run -p nvs-packer -- <input_dir> -o ./nvs-bundle`

Key flags:

- `--block-size <bytes>` (default: 131072)
- `--model <name>` (embeddings model recorded in manifest)
- `--quantize <f32|f16>` (vector dtype; default `f32`)
- `--compress <none|zstd>` (metadata block compression)
- `--zstd-level <1..22>` (when `--compress zstd`)
- `--meta-include-embeddings` (duplicate vectors in meta; off by default)
- `--fast-loader` and `--mmap-threshold <bytes>` (faster JSON ingest)
- `--bm25-buckets <N>` (parallel BM25 merge buckets; 0 = auto)

Input JSON can be a single object or an array of objects. Expected fields:

- `id` (string, optional)
- `text` or `content` (string)
- `metadata` (object) that contains an `embedding` array and any other fields

Example:

`cargo run -p nvs-packer -- ./samples/json -o ./out_bundle --model text-embedding-3-large --compress zstd --zstd-level 3`

Outputs a bundle directory with `manifest.json`, `meta.blocks`, `vectors.bin`, and indices.

## nvs-core (Library)

- Validates bundle structure and metadata, and provides reading/search capabilities (WIP for full vector/BM25 search APIs).
- Benchmarks live under `crates/nvs-core/benches`. Run with `cargo bench -p nvs-core`.

## PDF Extraction & Chunking

`nvs-pdf` provides a tokenizer‑aware PDF chunking pipeline. Two integration modes for PDFium:

- Bundled PDFium: `cargo build -p nvs-pdf --features pdfium-bundled`
  - Downloads platform‑specific PDFium via `curl` + `tar` during build.
  - Supported targets in the bundler: macOS (x64/arm64), Linux (x64/arm64).

- System PDFium: `cargo build -p nvs-pdf --features pdfium-system`
  - Install PDFium for your OS and make the library discoverable at runtime.
  - Common approaches:
    - Set `PDFIUM_LIB_DIR` to the directory containing `libpdfium.*` and ensure it is on your runtime library path (`DYLD_LIBRARY_PATH`/`LD_LIBRARY_PATH`).
    - Or follow `pdfium-render` crate docs for environment variables supported by that library.

High‑level example (library use):

```rust
use nvs_pdf::{parse_to_chunks, ChunkOptions, write_chunks_json};
use std::path::Path;

let opts = ChunkOptions { max_tokens: 512, ..Default::default() };
let pdf = Path::new("./document.pdf");
let chunks = parse_to_chunks(pdf, &opts)?;
write_chunks_json(pdf, &chunks, Path::new("./chunks.json"))?;
```

## Dev Tips

- Clippy: `cargo clippy --all-targets --all-features -- -D warnings`.
- Format: `cargo fmt`.
- SIMD: CPU features are detected at runtime via the `cpufeatures` crate.
- Large files: many crates use `memmap2` for zero‑copy reads; prefer release builds for perf: `cargo build -p <crate> --release`.

## Related Docs

- `documentation/MANIFEST_SPEC.md` — Bundle format.
- `documentation/RUST_PARITY.md` — Implementation parity notes.

## License

MIT (see `LICENSE`). Some crates offer dual‑license where noted in their `Cargo.toml`.

