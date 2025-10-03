# nvs-cli

Interactive CLI for Native Vector Store bundles: query, stats, REPL — plus a quick end‑to‑end demo.

Commands

- Query: run a single query and exit.
- Stats: print bundle statistics.
- Repl: interactive prompt; supports switching modes and weights.
- Quick: embed a chunks JSON with the local backend, pack a bundle, and run test queries.

Embedding backend

- Defaults to local CPU backend when NVS_EMBED_BACKEND=local (recommended for offline use).
- For OpenAI set NVS_EMBED_BACKEND=openai and pass --embed-model; requires OPENAI_API_KEY.

Examples

```
# Start REPL with local backend
NVS_EMBED_BACKEND=local nvs-cli --bundle ./out_bundle Repl

# One-shot query
NVS_EMBED_BACKEND=local nvs-cli Query --bundle ./out_bundle --mode hybrid --top 5 "gene expression"

# Quick: chunks.json -> bundle -> run queries
nvs-cli Quick \
  --chunks ./samples/json-rust/main.PMC12169792_chunks.json \
  --out ./.nvs-bundle-quick --quantize f16 --compress zstd --model gte-small-local
```

Quick pipeline (high-level)

```
flowchart LR
  C[Chunks JSON {text, meta}] -->|embed (local)| D[Docs JSON {text, metadata.embedding}]
  D -->|pack| B[(Bundle)]
  B --> Q[Vector/BM25/Hybrid queries]
```

License: MIT

