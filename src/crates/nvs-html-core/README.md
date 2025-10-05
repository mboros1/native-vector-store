# nvs-html-core

Low‑level HTML section extraction: mmap/bytes → (text, section_index) with timing breakdowns.

API

- `fast_extract_sections_with_stats(path, limit)` → `(Vec<(String, i32)>, HtmlExtractBreakdown)`
- `fast_extract_sections_from_bytes_with_stats(bytes, limit)` → same

Example

```rust
let path = std::path::Path::new("/path/file.html");
let (sections, stats) = nvs_html_core::fast_extract_sections_with_stats(path, Some(10))?;
assert!(sections.len() <= 10);
```

Integration

- Used by `nvs-html` orchestrator which then calls the shared chunker in `nvs-core`.

License: MIT

