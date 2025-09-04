use crate::chunker::Chunk;
use anyhow::Result;
use serde_json::{json, Value};
use std::fs::File;
use std::io::Write;
use std::path::Path;
use xxhash_rust::xxh64::xxh64;

pub fn write_chunks_json(pdf_path: &Path, chunks: &[Chunk], out_path: &Path) -> Result<()> {
    // Stable 64-bit hash of path string (not contents) for parity/stability
    let filename = pdf_path.file_name().and_then(|s| s.to_str()).unwrap_or("").to_string();
    let path_str = pdf_path.to_string_lossy();
    let binary_hash = xxh64(path_str.as_bytes(), 0) as i64;

    let total = chunks.len();
    let mut arr: Vec<Value> = Vec::with_capacity(total);
    for (i, c) in chunks.iter().enumerate() {
        let meta = json!({
            "schema_name": "docling_core.transforms.chunker.DocMeta",
            "version": "1.0.0",
            "start_page": c.start_page,
            "end_page": c.end_page,
            "page_count": (c.end_page - c.start_page + 1).max(0),
            "chunk_index": i as i64,
            "total_chunks": total as i64,
            "token_count": c.token_count as i64,
            "has_major_heading": c.has_major_heading,
            "min_heading_level": c.min_heading_level,
            "origin": {
                "mimetype": "application/pdf",
                "binary_hash": binary_hash,
                "filename": filename,
                "uri": serde_json::Value::Null,
            },
            "doc_items": [],
            "headings": [],
            "captions": serde_json::Value::Null,
        });
        let obj = json!({
            "text": c.text,
            "meta": meta,
        });
        arr.push(obj);
    }

    let mut f = File::create(out_path)?;
    let s = serde_json::to_string_pretty(&arr)?;
    f.write_all(s.as_bytes())?;
    Ok(())
}

