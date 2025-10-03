# nvs-pdf-core

Core PDF text extraction (Rust) with optional PDFium integration. Produces `(text, page_index)` with timing stats.

API

- `extract_text_pages_with_stats(path, page_limit, threads)` → `(Vec<(String, i32)>, ExtractStats)`
- `extract_text_pages(path, page_limit, threads)` → `Vec<(String, i32)>`
- With `pdfium` feature: `extract_text_pages_with_pdfium(pdfium, path, page_limit)`

Notes

- Pure Rust path: fast path using internal parser/streams/objects; set `threads` at orchestrator level.
- PDFium path (feature `pdfium`): binds via pdfium-render; supports system, lib-dir, or env overrides.

Example

```rust
let (pages, stats) = nvs_pdf_core::extract_text_pages_with_stats(std::path::Path::new("/path.pdf"), Some(5), 0)?;
assert!(!pages.is_empty());
```

License: MIT

