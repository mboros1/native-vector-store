# Rust Parity Guide

This document maps the C++ implementation to a prospective Rust rewrite with a focus on ownership, type semantics, and IO boundaries.

## High-Level Architecture

- Packer (C++: `NVSPacker`) → Rust: `packer` crate/binary
  - Pure write-only pipeline over owned data loaded by `DocumentLoader`.
  - Writes:
    - `vectors.f32|f16`: aligned rows (64‑byte padded)
    - `doclen.u32`: per-doc token counts
    - `lexicon.bin`, `postings.bin`, `terms.dict`: BM25 index
    - `meta.blocks`, `meta.idx`: doc-aligned metadata blocks + index
    - `manifest.json` and `checksums.sha256`
  - Rust equivalents:
    - `memmap2` optional for verifying output (tests), `std::fs::File` + buffered write for generation.
    - Use `byteorder`/`bytemuck`‐like patterns for binary encoding.

- Reader (C++: `VectorStoreV2`) → Rust: `vector_store_v2` crate/lib
  - Read-only, memory-mapped (`MMapFile` in C++; `memmap2::Mmap` in Rust)
  - Decodes:
    - `vectors.f32|f16` rows
    - `doclen.u32`
    - lexicon + postings (delta-encoded)
    - block headers + doc index
  - Rust types:
    - `Mmap` over files; small newtype wrappers to guarantee alignment/endianness.
    - `SearchResult` as owned strings for id/text/metadata JSON; vector search API returns `Vec<SearchResult>`.

- Tokenizer (C++: `SimpleTokenizer`) → Rust: `tokenizer` module
  - UTF‑8 codepoint scanning; preserve non‑ASCII words; ASCII in‑word punct: `' - / &`.
  - Ellipsis and EOL period handling; ASCII contractions retained.
  - Rust: implement via `as_bytes()` iteration and UTF‑8 decode; avoid regex; return `Vec<String>`.

- Loader (C++: `DocumentLoader`) → Rust: `loader` module
  - Loads directory of JSON; establishes dimensions, builds BM25 stats; strings owned.
  - Rust: `serde_json` streaming for large files; small files read into `String`; produce owned `Vec<Document>` and stats.

## Ownership & Lifetime Model

- Packer: inputs owned; outputs written; no shared state.
- Reader/VectorStoreV2:
  - Store owns file descriptors and mmaps; pointers valid while store is open.
  - `get_document()` returns fresh owned `String`s; search returns owned `Vec<SearchResult>`.
  - Rust: `struct VectorStore { mmaps: Vec<Mmap>, slices: *const T .. }` with lifetimes hidden; exposed API clones/copies user-facing strings.

- Tokenizer: stateless (except a flag); inputs borrowed & outputs owned.
- Loader: produces owned `Document` and stats; no references into input buffers.

## File Format Parity

- `manifest.json`: same schema; include `files.meta.block_size` for reader sanity.
- `meta.blocks` header: `u32 block_count` then `block_count` headers of 4x`u32` {block_id, uncompressed_size, doc_count, padding}.
- `meta.idx`: per-doc index entries 4x`u32` {block_id, offset_in_block, doc_size, padding}.
- `lexicon.bin`: contiguous array of {`u64` offset, `u32` length, `u32` df}.
- `postings.bin`: delta‐encoded {`u32` delta, `u32` tf} repeated per term.

## Error Handling & Invariants

- Reader rejects:
  - `block_count == 0`
  - inconsistent block sizing (header describes blocks but no payload)
  - `offset_in_block + doc_size > block_size`
  - missing files
  - Rust: use `Result<_, Error>`; keep error enums for diagnostics.

- Packer validates:
  - consistent `dim` across docs; establishes from first doc when not provided
  - non-negative lengths; document not split across blocks

## Threading & Performance

- Vector search: parallel dot product; per-thread heaps; merge at end.
  - Rust: Rayon for parallel iteration; small `BinaryHeap` per thread; k-way merge.
- Loader: parallel file reading (optional) + tokenization; Rust: Rayon or async for IO.

## Testing Guidance

- Keep E2E tests: pack → open → get_document/search.
- Validate BM25 df/postings; block header scanning; checksums formatting.
- Tokenizer Unicode tests: Latin‑1, Cyrillic; ellipsis; punctuation; URLs/emails.

## Module Mapping Summary

- C++ `src/vector_store_v2.{h,cpp}` → Rust `vector_store_v2/src/lib.rs`
- C++ `src/nvs_pack.cpp` → Rust `packer/src/main.rs` or lib + bin
- C++ `src/document_loader.{h,cpp}` → Rust `loader/src/lib.rs`
- C++ `src/simple_tokenizer.{h,cpp}` → Rust `tokenizer/src/lib.rs`
- C++ `src/mmap_file.h` → Rust uses `memmap2` directly

