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

- `vectors.*`:
  - `num_docs` rows, each `dim * sizeof(dtype)` bytes, padded to `files.vectors.row_alignment` bytes (default 64). DType is `f32` or `f16`.

- `doclen.u32`:
  - `num_docs` elements (`u32`).

- `terms.dict`:
  - Repeated: `<u32 len>` + `len` bytes raw term string.

- `lexicon.bin`:
  - Array of `{ u64 offset, u32 length, u32 df }` (no padding between entries), in the same order as terms in `terms.dict`.

- `postings.bin`:
  - For each term: `length` pairs of `{ u32 delta_docid, u32 term_freq }`. `docid` reconstructed via prefix sum of deltas.

- `meta.blocks`:
  - Magic (8 bytes): `NVSMETA\x01`.
  - Header: `u32 block_count`.
  - Block headers: `block_count` entries of 4x`u32` `{ comp_size, decomp_size, doc_count, codec }`, where:
    - `codec`: 0 = none, 1 = zstd
    - If `codec = 1`, `comp_size` is the actual compressed size before padding; if `codec = 0`, `comp_size == decomp_size`.
    - `decomp_size` = valid unpadded bytes for the block (sum of packed records), `<= block_size`.
  - Payload: `block_count` blocks, each of size exactly `files.meta.block_size` bytes; compressed payloads are zero-padded to this size.
  - Document record: `[u32 id_len][id][u32 text_len][text][u32 meta_len][metadata_json]` (concatenated).

- `meta.idx`:
  - Magic (8 bytes): `NVSIDX\0\x01`.
  - Payload: array of 4x`u32` entries `{ block_id, offset_in_block, doc_size, reserved0 }` for each `doc_id` in `[0..num_docs)`.

- `checksums.xxhash64`:
  - One line per bundle file: `<16-hex-digits>␠␠<filename>` using xxhash64.

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
