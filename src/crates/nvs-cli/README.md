# nvs-cli

Interactive CLI to open a Native Vector Store bundle and run vector/BM25/hybrid queries. Also includes a Quick helper to embed a chunks.json, pack a bundle, and run a few sample queries.

Usage
- REPL with a bundle: `nvs-cli --bundle ./out_bundle Repl`
- One-off query: `nvs-cli Query --bundle ./out_bundle --mode hybrid --top 5 "gene expression"`
- Quick flow: `nvs-cli Quick --chunks ./samples/json-rust/main.PMC12169792_chunks.json --out ./out_bundle/pmc12169792 --quantize f16 --compress zstd --model gte-small-local`

One-shot binary (embedded model)
- Build a single binary with the local GTE-small model embedded (no external files):
  - `cargo build -p nvs-cli --manifest-path src/Cargo.toml --features embed-model --release`
- Notes:
  - The binary will be ~70–75 MB larger (includes model.safetensors + tokenizer/config).
  - On first run, the embedded bytes are written to a small cache dir for memory mapping (default `$TMPDIR/nvs_embed_gte_small`; override with `NVS_EMBED_CACHE_DIR`).
  - No network or HF Hub access is needed. To allow auto-fetch instead of embedding, build with `--features hub-fetch` (and without `embed-model`).
