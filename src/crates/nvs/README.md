# nvs

Autopilot CLI that chunks sources (PDF/HTML), embeds locally, and packs a bundle — end to end.

Usage

```
nvs <INPUT> [--out BUNDLE_DIR] [--work DIR] [--quantize f32|f16] [--compress none|zstd] [--model NAME]
```

- If INPUT is a directory with PDFs/HTML, runs: chunk → embed → pack.
- If INPUT already contains docs.json (embedded), skips chunk+embed and packs directly.
- By default uses local CPU embedder (NVS_EMBED_BACKEND=local). For OpenAI, set NVS_EMBED_BACKEND=openai and OPENAI_API_KEY.

Pipeline

```
flowchart TD
  A[PDF/HTML] --> C[Chunk]
  C --> E[Embed (local)] --> D[Docs JSON]
  D --> P[Pack (nvs-packer)] --> B[(Bundle)]
  B --> V[Verify Open]
```

Examples

```
# Local embedding and zstd metadata compression
nvs ./samples/pdf-dir --out ./.nvs-bundle --compress zstd --quantize f16 --model gte-small-local

# Pack existing docs (skip chunk+embed)
nvs ./work/docs --out ./.nvs-bundle --compress zstd
```

License: MIT

