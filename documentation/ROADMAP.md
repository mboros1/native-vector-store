# Roadmap: Action Items

This roadmap lists the prioritized actions to deliver a SQLite‑like embedded hybrid vector store with Rust as the core, Node and Python bindings, serverless support, and a cohesive CLI.


1) Unified CLI
- New top‑level `nvs` binary aggregating:
  - `chunk pdf|html`, `embed openai`, `pack`, `query`, `stats`, `verify` (optional `serve`).
- Reuse existing crates under the hood; keep current CLIs as advanced tools.
- Update REPL CLI to have `chunk`, `embed`, `pack` commands
- Add an all in one command around that takes raw pdf/html documents and runs each step to produce a final binary pack

2) CI Overhaul
- Add Rust CI (fmt, clippy, tests, benches) and a release pipeline to publish npm/PyPI (and crates.io if desired).

3) Node Binding (Rust, napi-rs)
- Create an `nvs-node` crate using `napi-rs` exposing:
  - `class VectorStore { constructor(path), size(), dimensions(), searchVector(), searchBm25(), searchHybrid(), getDocument(id) }`.
- Packaging:
  - Produce prebuilds for macOS (x64/arm64), Linux (x64/arm64; gnu, optional musl), Windows x64.
  - Build Linux with x86-64-v3 (AVX2) and no AVX512 for Lambda.
- Cleanup:
  - Remove `binding.gyp`/C++ vestiges; align `package.json` and `lib/index.d.ts` with the new API.

4) Python Binding (pyo3 + maturin)
- New `nvs-py` crate exposing the same API surface via PyO3.
- Optional NumPy support for zero‑copy Float32 vectors.
- Packaging with `maturin` (manylinux/musllinux, macOS universal2, Windows x64) and publish wheels to PyPI.

5) Serverless Readiness
- Provide minimal Lambda examples for Node (napi prebuild) and Python (wheel), using the Rust reader.
- Document cold‑start behavior and recommended memory/CPU settings.
- Ship Lambda‑compatible Linux artifacts (x86-64-v3, no AVX512).

6) CD Overhaul
- Replace node‑gyp prebuild workflows with `napi-rs` prebuild pipelines.
- Add Python wheel builds (maturin/cibuildwheel) for all targets.

7) Documentation Refresh
- Update top‑level `README.md` with cross‑language quickstarts and positioning.
- Refresh `USAGE.md` and site docs to reflect Rust‑backed bindings (remove C++ guidance).
- Reconcile manifest spec with current code (f16 support, compression options).
- Add a serverless guide and a versioning/compatibility policy.

8) Examples and Samples
- Node and Python examples for opening a bundle and running hybrid search.
- End‑to‑end example: pdf/html → chunk → embed → pack → query (via unified CLI).
- Simple HTTP server example for local dev.

9) Optional Enhancements
- Single‑file bundle container (alongside current directory layout).
- Public C header + shared library for broader language FFI.
- `nvs serve` mode in CLI (basic HTTP) for quick local integration.

