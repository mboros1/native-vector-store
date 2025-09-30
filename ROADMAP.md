# Tiny Search Engine — Roadmap

Goal: ship a local, self‑contained search stack that is fast to cold‑start, low‑RAM, highly parallel, and requires no external services. Target short documents (plugin titles/descriptions/tags) with a hybrid ranking: BM25 candidates + tiny embedding rerank + light popularity/quality signals.

## Corpus
- Use the provided NDJSON sample: `src/vsx.ndjson.zst` (zstd‑compressed Open VSX–style records).
- Fields of interest per item: `id (publisher.name)`, `displayName`, `description`, `tags`, `categories`, `downloadCount`, `reviewCount`, `averageRating?`, `verified`, `timestamp`, `targetPlatform`.

## Bundle & Schema
- Text for indexing/embedding: short, high‑signal concat
  - `"<displayName> — <description>. Tags: <t1, t2>. Categories: <c1, c2>."` (skip missing; cap length).
- Metadata to store per doc (JSON):
  - `publisher`, `name`, `displayName`, `description`, `tags`, `categories`,
    `downloadCount`, `reviewCount`, `averageRating?`, `verified`, `preRelease`, `preview`, `timestamp`, `targetPlatform`.
  - Derived: `popularity_log`, `age_days`, `has_tags`, `rating_present`.
- Vectors: f32 or f16 (prefer f16 for footprint).
- BM25: per‑field postings (title/description/tags/categories) with boosts and optional title positions for phrase/exact bonuses.

## Query Pipeline
1) Lexical BM25 candidates (top‑K, e.g., 200) with field boosts and exact/phrase bonuses.
2) Embedding rerank over candidates (tiny local model, cosine similarity).
3) Fusion: RRF or weighted sum `score = w_bm25 * bm25 + w_cos * cos + w_sig * quality`.

## Models
- Embeddings: local GTE‑small (files in `src/models/gte-small`), CPU‑friendly; truncation ~256 tokens.
- Sentiment: optional later; start with heuristic quality (no review text in corpus).

## Ranking Signals
- Exact/phrase match in title/tags (strong boost).
- Popularity: `log1p(downloadCount)`.
- Freshness: mild time decay from `timestamp`.
- Quality: `averageRating` when present; otherwise back‑off on `reviewCount` + popularity + `verified`.

## Performance
- Mmap bundle; SIMD dot; parallel scoring.
- Keep rerank set bounded (e.g., 200) to cap embedding compute.
- Goal: bundle + model within a few hundred MB; fast open.

## Packaging
- Single bundle directory with manifest + binary postings + vectors + meta blocks.
- Optional CLI/TUI (`nvs-cli`) and tiny bindings later.

---

## TODOs (Getting Started)

1) Converter: VSX NDJSON → packer docs
- Read `src/vsx.ndjson.zst` and produce `work/vsx-chunks.json` as an array of `{ text, meta }` where `meta` carries VSX fields + derived signals + `id`.
- Keep `text` short and title‑first; strip empties; length cap (e.g., 768 chars).

2) Embedder integration (offline)
- Use `nvs-embed` local GTE‑small to convert `work/vsx-chunks.json` → `work/vsx-docs.json` (packer‑ready): `{ id, text, metadata: { embedding, ... } }`.
- Concurrency defaults: batch 16, concurrency 8.

3) Bundle pack
- Run `nvs-packer` on `work/vsx-docs.json` directory.
- Flags: `--quantize f16`, `--compress zstd`, record model name in manifest.

4) BM25 field boosts + exact/phrase bonuses
- Extend packer to support per‑field postings (title/description/tags/categories) and optional title positions.
- Update scorer to apply boosts and bonuses; keep defaults configurable.

5) Rerank + fusion
- Add a re‑rank path in `nvs-core` (or a helper) to compute cosine for top‑K and fuse via RRF or weighted sum.
- Provide a CLI switch to disable rerank for ablations.

6) Quality heuristics
- Implement simple `quality_score`: combine `averageRating` (if present), `reviewCount` (bounded), `verified` boost, and `popularity_log`.
- Normalize into [0,1] for fusion.

7) Tiny eval harness
- Create a small query set and expected IDs; grid search `w_bm25`, `w_cos`, bonuses.

## Nice‑to‑haves (Next)
- Phrase queries via quotes; category/domain boosts.
- Optional dependency‑graph authority prior.
- Node/TS bindings.

---

## Notes
- Decompress sample quickly: `zstd -d -c src/vsx.ndjson.zst | head`.
- Local embed model discovery follows `src/models/gte-small` or env `NVS_LOCAL_EMBED_MODEL_DIR`.
