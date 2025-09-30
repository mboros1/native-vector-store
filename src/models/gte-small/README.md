GTE-small (local embedding model)

This repository already includes the files needed to run the GTE-small model fully offline on CPU. The model comes from the Hugging Face repository thenlper/gte-small and produces 384‑dimensional sentence embeddings suitable for semantic search and hybrid ranking.

What’s included here
- tokenizer.json
- tokenizer_config.json
- config.json
- model.safetensors

How it’s used
- The `nvs-embed` crate’s local backend automatically discovers this directory (one of several well‑known locations) and loads the model at runtime.
- The embed CLI and autopilot use that backend to generate embeddings without network access.
- If local files are not found, the backend falls back to the Hugging Face Hub cache for `thenlper/gte-small`.

Model details (brief)
- Architecture: BERT‑style encoder with mean pooling and L2 normalization.
- Embedding size: 384 floats per text.
- Typical max input length used here: 256 tokens.

Configuration
- To use a different on‑disk location, set `NVS_LOCAL_EMBED_MODEL_DIR` to a directory containing `tokenizer.json`, `config.json`, and `model.safetensors`.

Reference
thenlper/gte-small — https://huggingface.co/thenlper/gte-small
