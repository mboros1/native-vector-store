# Manifest Specification (nvs.v1)

The manifest declares the bundle format and file locations. The reader validates these invariants before opening.

## Schema

```
{
  "format": "nvs.v1",
  "endianness": "little",                 // all binary files use little-endian integers
  "created_at": "2025-01-01T00:00:00Z",   // optional
  "num_docs": <u64>,
  "dim": <u64>,
  "embedding": {
    "model": "<string>",
    "dtype": "f32" | "f16"
  },
  "bm25": {
    "avgdl": <f64>,
    "k1": <f64>,
    "b": <f64>
  },
  "files": {
    "vectors": { "path": "vectors.f32", "dtype": "f32", "rows": <u64>, "cols": <u64>, "row_alignment": 64 },
    "doclen":  { "path": "doclen.u32",  "dtype": "u32", "rows": <u64> },
    "lexicon": { "path": "lexicon.bin" },
    "postings":{ "path": "postings.bin" },
    "terms":   { "path": "terms.dict" },
    "meta_idx":{ "path": "meta.idx", "schema": "u32 block_id, u32 offset, u32 doc_size, u32 reserved0" },
    "meta":    { "path": "meta.blocks", "block_size": <u64>, "doc_aligned": true }
  }
}
```

## Invariants

- `format == "nvs.v1"`
- `num_docs >= 0`, `dim > 0`.
- `embedding.dtype in {"f32","f16"}`.
- `files.vectors.rows == num_docs`, `files.vectors.cols == dim`.
- `files.doclen.rows == num_docs`.
- `files.meta.block_size > 0`.
- Reader computes derived `block_size` from `meta.blocks` layout and rejects if it does not match the manifest `block_size`.
- `meta.idx` entry count must be exactly `num_docs`.

## File Layouts

Below is a concise description of each binary file and its role. All integers are little‑endian. Vector rows are padded to the alignment declared in the manifest (default 64 bytes).

- `vectors.*`
  - `num_docs` rows, each `dim * sizeof(dtype)` bytes, padded to `files.vectors.row_alignment`. `dtype` is `f32` or `f16`.
  - Row alignment enables efficient SIMD scans; readers widen `f16` rows to `f32` in‑register when scoring.

- `doclen.u32`
  - Dense `u32` array of length `num_docs` holding document token counts after tokenization. Used in BM25 normalization alongside `bm25.avgdl`/`k1`/`b` from the manifest.

- `terms.dict`
  - Term dictionary as repeated `[u32 len][bytes…]` entries in lexical order. The term’s position implies its term ID and aligns 1:1 with entries in `lexicon.bin`.

- `lexicon.bin`
  - Array of 16‑byte entries `{ u64 offset, u32 length, u32 df }` parallel to `terms.dict`.
  - `offset` indexes into `postings.bin`; `length` is the number of postings for the term; `df` is document frequency.

- `postings.bin`
  - Concatenated inverted lists. For each term, `length` pairs `{ u32 delta_docid, u32 term_freq }` are stored.
  - Readers reconstruct absolute doc IDs via prefix sum over deltas and compute BM25 scores using `doclen.u32` and manifest BM25 params.

- `meta.blocks`
  - Header: magic (8 bytes) `NVSMETA\x01`, then `u32 block_count`, followed by `block_count` headers of 16 bytes `{ comp_size, decomp_size, doc_count, codec }`.
  - Payload: `block_count` fixed‑size blocks of `files.meta.block_size` bytes. If `codec=1` (zstd), the first `comp_size` bytes are compressed data; the rest is padding. `decomp_size` is the valid unpadded data in the block (sum of records).
  - Each record within a (decompressed) block is `[u32 id_len][id][u32 text_len][text][u32 meta_len][metadata_json]`.

- `meta.idx`
  - Header: magic (8 bytes) `NVSIDX\x01`.
  - Payload: `num_docs` entries, each 16 bytes `{ u32 block_id, u32 offset_in_block, u32 doc_size, u32 reserved0 }`, mapping logical doc IDs to their record location in `meta.blocks`.

- `checksums.xxhash64`
  - One line per file: `<16-hex-digits>␠␠<filename>` using xxhash64 for quick integrity verification.

## Reader Rules

- Reject if any required file is missing or empty.
- Reject if `block_count == 0`.
- Reject if computed `derived_block_size != files.meta.block_size` when provided.
- Reject if any `offset_in_block + doc_size > block_size`.
- `get_document(doc_id)` returns `(id, text, metadata_json)` as strings; `get_document_value(doc_id)` returns parsed JSON to surface errors early.

## Binary Conventions
- Endianness: little-endian for all integer fields across all files.
- Alignment: vector rows aligned to `files.vectors.row_alignment` bytes (default 64).
- Magic/version: `meta.idx` and `meta.blocks` begin with an 8-byte magic+version tag as above.
