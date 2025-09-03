# Case Study: Tokenizer and BM25 Packer Improvements

This document summarizes the iterative improvements we made to the Rust packer and tokenizer, along with validation, benchmarking, and outcomes compared to the previous C++ implementation.

## Overview

- Goal: Improve data quality (tokenization/normalization), ingestion speed, and BM25 index build time; remove duplication; add diagnostics and tests; and simplify/accelerate the final write path.
- Scope: Rust packer (`nvs-packer`) and core tokenizer (`nvs-core`), with comparability tools to reconcile differences with the C++ packer.

## Baseline (before)

- JSON ingest: single-threaded `serde_json` parse, per-file `String` allocations.
- Tokenization: permissive; numeric and punctuation-heavy tokens admitted to BM25; case inconsistencies; artifacts from PDF/OCR (soft hyphens, form feed, line-break hyphens) leaked into terms.
- BM25 build: per-doc TF in parallel, then single-threaded global merge and output. `bm25_write` dominated wall time.
- No receipts; hard to diff which files were processed across implementations.

## Iterations & Highlights

### 1) Receipts & C++ parity checks

- Added `receipts.txt` to both packers (Rust and C++) listing `<path>\t<doc_count>` sorted alphabetically.
- Added `scripts/diff_receipts.js` to compare bundles’ receipts and highlight missing/changed files.
- Made C++ loader recurse and warn on empty outputs; revealed large doc count differences and missing files due to loader assumptions.

### 2) Fast JSON loader (Rust)

Step-by-step design and changes:

- Adaptive IO thresholds
  - For each `*.json` file:
    - If size ≤ N (default 5 MB, configurable `--mmap-threshold`), memory-map with `memmap2::Mmap`.
    - Else, read fully into a reusable `Vec<u8>` buffer (reduces mmap overhead for very large files).
  - Rationale: mmapping small files avoids copies; buffered reads avoid mmapping overhead and permit reuse.

- Producer–consumer pipeline
  - Producer walks the tree (depth-1 or deeper), classifies and loads each file (mmap or Vec), then enqueues jobs.
  - Bounded channel (`crossbeam-channel`) prevents memory blow-up when files outpace consumers.
  - Consumers: `N = available_parallelism()` threads.
    - Each thread holds a thread-local `Vec<Doc>` and (in the fast path) a thread-local receipts vector.
  - Receipts: each consumer records `(filename, docs_parsed)` to support later diffs.

- Streaming parse of arrays (no `Vec<Value>`)
  - Use `serde_json::Deserializer::from_slice` and a custom Visitor:
    - If root is an array, iterate with `next_element::<InputDocRaw>()` and process each element immediately.
    - If root is an object, parse it once and process as a single document.
  - Avoids building an in-memory DOM and drastically reduces peak allocations.

- Input schema and extraction
  - We accept `{ id?, text? | content?, metadata.embedding }`. We extract only fields we need:
    - The embedding vector (required), text/content, and id (optional, auto-hashed fallback).
  - Any other metadata is kept as a Serde map (optional) and preserved for meta.blocks JSON.

- Fail-fast and warnings
  - Files that parse but yield zero valid docs are counted in receipts and (in the C++ path) logged with a warning.
  - Invalid docs (missing embedding, dimension mismatch) are skipped with structured errors printed once.

- Determinism & order
  - Document order remains stable per file; per-thread vectors are merged at the end.
  - We don’t sort globally; output determinism is driven by file traversal order and stable per-thread merges.

- Bench integration
  - `scripts/bench_packer.js` supports `--fast-loader` and `--mmap-threshold` to toggle the path and tune thresholds.

- Outcome
  - On the sample corpus, the `read` stage dropped from ~0.8–0.9 s to ~0.1–0.2 s.
  - Lower peak memory, better parallel utilization, no large intermediate `Vec<Value>`.

### 3) Tokenization & normalization

Step-by-step changes and rationale:

- Preprocessing (text-level)
  - `preprocess_bm25(text)`: a cheap normalization pass before tokenization:
    - Convert CR, FF (form feed, shows as ^L), TAB to spaces.
    - Remove soft hyphen U+00AD, zero-width space, BOM.
    - Dehyphenate "hyphen + whitespace + line break": `High-\nquality` → `High quality`.
    - Collapse whitespace runs to single spaces.
  - Why: prevents spurious tokens like `High-`, `bar^L`, `and^L`, and avoids panics from control characters.

- One-pass BM25 tokenization
  - Implemented `tokenize_bm25_into()`: single scan builds tokens into a small buffer, lowercases ASCII in place, and flushes into the TF map with normalization; no intermediate `Vec<String>`.
  - Reduces allocations and improves cache behavior compared to split → post-process → insert.

- Normalization (token-level)
  - `bm25_normalize_token(token) -> Option<String>`:
    - Trim leading/trailing punctuation (including `&`, `'`, quotes, etc.).
    - Strip possessives 's and ’s via `char_indices` (Unicode-safe; fixes prior panics).
    - Drop: numeric-only tokens (allowing +-. , /), `utm_*` tracking keys, triple-hyphen runs, AA-like all-caps sequences ≥ 10.
    - Keep: biomedical patterns like `il-6`, `p53`, `covid-19`.
  - Lowercase + stopwords: applied once per normalized token; pack-time and query-time now use the same rules.

- Query-time alignment
  - BM25 query path now calls `preprocess_bm25` + `bm25_normalize_token` so query tokens match index terms.

- Examples (before → after)
  - `&Chibnall` → `chibnall`
  - `manufacturer’s` → `manufacturer`
  - `High-\nquality` → `high quality`
  - `-0.03` → dropped; `utm_campaign` → dropped; `---ABC` → dropped
  - `il-6`, `p53`, `covid-19` → preserved

- Unit tests added
  - Confirm preprocessing dehyphenates and removes control characters.
  - Confirm normalization strips possessives (ASCII and Unicode) without panics.
  - Confirm numeric/url/sequence/triple-hyphen tokens are dropped; biomedical short patterns kept.

- Impact (sample corpus)
  - Unique terms: ~290k → ~199k; postings: ~2.53M → ~1.66M; bundle: ~111.9 MB → ~102.8 MB.
  - Reduced noise improves both pack-time and search-time overheads.

Outcomes (sample corpus):

- Unique terms: ~290k → ~199k.
- Postings: ~2.53M → ~1.66M.
- Bundle size: ~111.9 MB → ~102.8 MB.
- Crashes from Unicode apostrophes eliminated.

### 4) BM25 pipeline refactors

Step-by-step improvements:

1) Phase 1 — tokenize + TF maps
   - Replaced split-based path with `tokenize_bm25_into()` (one pass, lowercase in-place, stream normalized tokens to TF map). Improves CPU/cache and reduces allocations.

2) Phase 2 — per-thread postings
   - Partition doc IDs into `chunks = available_parallelism()` contiguous ranges.
   - Each thread builds a local map: term → `Vec<(doc_id, tf)>` in increasing `doc_id` order (naturally sorted postings).
   - This avoids a global concurrent map and sets up a cheap k-way merge.

3) Phase 3 — parallel bucketed merge
   - Terms are partitioned into `B` buckets by hash; each bucket merges per-thread vectors for its terms and produces:
     - A sorted term list, a contiguous postings buffer, and a parallel lexicon (offset_rel, length, df) for the bucket.
   - Buckets are built in parallel; no global locks.

4) Final write (formerly `bm25_write` hot path)
   - Removed per-iteration locks: move `BucketOut`s into a plain `Vec` for the single-threaded final merge.
   - Reserved capacities:
     - postings_final.reserve(total_post_bytes)
     - lexicon_final.reserve(total_terms × 16)
   - Streamed `terms.dict` with `BufWriter` (no `Vec<String>` of terms).
   - Coalesced copies: if consecutive terms originate from the same bucket at adjacent offsets, accumulate a run and copy once.
   - Internally timed `assemble` (final merge + copies) vs `io` (file writes) to confirm CPU dominates.
   - Reported `bm25_write = assemble + io` for compatibility, but sub-timers guided the optimization.

Results (sample corpus)

- `bm25_write`: ~0.8–1.2 s → ~70 ms after the changes.
- BM25 total: ~1.5 s → ~0.36 s with tokenizer and write improvements.
- Writing ~20–30 MB is fast; the bottleneck was always CPU: term comparisons, slice selection, and many tiny copies.

Why it works

- Per-thread postings give naturally sorted inputs.
- Buckets localize work; removing locks and coalescing copies minimizes overhead.
- Reserving and streaming avoid costly reallocations and term clones.

Potential next steps

- Binary heap for bucket-head selection (O(T log B)) — a small additional win now that B is small and locks are gone.
- `--min-df=N` to prune ultra-rare terms, shrinking both lexicon and postings further.
- Parallelize final assembly further by precomputing offsets and chunking the copy phase if needed.

### 5) Diagnostics & tests

- Tokenizer unit tests added:
  - Dehyphenation & control char handling.
  - Possessive stripping (ASCII + Unicode) without panics.
  - Dropping numeric-only / `utm_*` / triple-hyphen / AA-sequences.
  - Preserving biomedical patterns; trimming leading punctuation.
- BM25 e2e test (`bm25_clean.rs`) added:
  - Builds a tiny bundle with real-world artifacts and asserts terms are normalized and noisy tokens excluded.

## Benchmarks (indicative)

- Ingest (read): ~0.8–0.9 s → ~0.1 s (fast loader).
- Tokenize (bm25_tokenize): ~0.1–0.24 s (after fast path and filters), replacing a heavier multi-pass path.
- Merge (bm25_merge): ~0.06–0.09 s (parallel buckets).
- Write (bm25_write): ~0.8–1.2 s → ~0.07 s (coalesced, reserved, streamed).
- Total BM25: ~1.5 s → ~0.35 s.
- Bundle size: ~111.9 MB → ~102.8 MB.

Note: Numbers above depend on CPU, buckets, and corpus shape; they reflect typical runs on the provided sample.

## Lessons Learned

- Data quality directly impacts performance: filtering numeric/punct tokens reduces unique terms and postings volume, speeding both pack and search.
- IO rarely the bottleneck: the final write is dominated by CPU-bound merging/copying; big wins came from algorithmic changes, not async/IO tweaks.
- Streaming over building collections: both for JSON parsing and term writing, streaming avoids large intermediate allocations and churn.
- Deterministic order and single ownership simplify optimizations: per-thread maps + bucketed merge make it easy to reserve, coalesce, and stream.

## What’s left / Next steps

- Optional: `--min-df N` to drop ultra-rare terms for even smaller/faster indexes.
- Optional: switch O(B) head scan to a binary heap (O(T log B)) — likely a small win now that locks and clones are gone.
- Reader improvements: mmap `lexicon.bin`/`postings.bin`/`doclen.u32` and apply `madvise` hints for query-time speed.
- Additional tests: more adversarial inputs (e.g., deep Unicode, pathological punctuation) and benchmarks by dimension and corpus shape.

## Tools & Files Added

- `scripts/diff_receipts.js`: diffs `receipts.txt` across bundles and reports changed/missing files and doc deltas.
- Receipts in both packers (Rust & C++): `receipts.txt` in bundle root.
- Tests:
  - `nvs-core` tokenizer tests (bm25 normalization & default tokenizer).
  - `nvs-packer/tests/bm25_clean.rs` e2e test for BM25 term cleanliness.

## Summary

We achieved substantial wins by improving tokenization (quality + robustness), accelerating JSON ingest, and refactoring the BM25 write path. The end result: cleaner indexes, faster builds, smaller bundles, and a more reliable, test-backed packer pipeline.
