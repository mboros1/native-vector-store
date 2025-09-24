Local GTE-small model files

Place the following files in this directory to enable fully offline embedding with the local CPU backend:

- tokenizer.json
- config.json
- model.safetensors

These are from the Hugging Face repo: thenlper/gte-small
https://huggingface.co/thenlper/gte-small

Once present, the embed CLI and autopilot will discover them automatically.

Git LFS

The model.safetensors file is large. Track it with Git LFS so your repo stays lean:

1. Install Git LFS once: git lfs install
2. Track safetensors: git lfs track "*.safetensors"
3. Add and commit files: git add rust/models/gte-small/* && git commit -m "Add local gte-small model"

You can also set NVS_LOCAL_EMBED_MODEL_DIR to point to a custom directory instead of using this path.

