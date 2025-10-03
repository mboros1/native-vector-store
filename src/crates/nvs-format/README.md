# nvs-format

Shared on‑disk format constants and helpers used by the reader (nvs-core) and writer (nvs-packer).

Scope

- Magic headers: `META_IDX_MAGIC`, `META_BLOCKS_MAGIC`.
- Types: `MetaIdxEntry`, `VectorLayout`, `DType`.
- Helpers: `row_stride_bytes(cols, dtype, align)`.

Binary conventions

- Endianness: little for all integer fields.
- Alignment: vector rows padded to `files.vectors.row_alignment` bytes (default 64).
- Magic:
  - `meta.idx` begins with `NVSIDX\x01`.
  - `meta.blocks` begins with `NVSMETA\x01`.
- `meta.idx` entry: `{ u32 block_id, u32 offset_in_block, u32 doc_size, u32 reserved0 }`.
- `meta.blocks` header entry: `{ u32 comp_size, u32 decomp_size, u32 doc_count, u32 codec }` with `codec` in {0=none, 1=zstd}.

Overview

```
flowchart LR
  MI[meta.idx (NVSIDX\x01)] -->|entries| Off[Offsets]
  MB[meta.blocks (NVSMETA\x01)] -->|headers| H[Per‑block headers]
  MB -->|payload| R[Length‑prefixed records]
```

License: MIT

