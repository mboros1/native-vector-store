use anyhow::Result;
use serde_json::{json, Value};
use std::fs::File;
use std::io::Write;
use std::path::Path;
use nvs_core::chunker::Chunk;
use xxhash_rust::xxh64::xxh64;

pub fn write_chunks_json(html_path: &Path, chunks: &[Chunk], out_path: &Path) -> Result<()> {
    let filename = html_path.file_name().and_then(|s| s.to_str()).unwrap_or("").to_string();
    let path_str = html_path.to_string_lossy();
    let binary_hash = xxh64(path_str.as_bytes(), 0) as i64;

    let total = chunks.len();
    let (doc_min_page, doc_max_page) = if total == 0 {
        (0i32, -1i32)
    } else {
        let mut min_p = i32::MAX;
        let mut max_p = i32::MIN;
        for c in chunks {
            min_p = min_p.min(c.start_page);
            max_p = max_p.max(c.end_page);
        }
        (min_p, max_p)
    };
    let doc_page_count: i64 = if doc_max_page >= doc_min_page { (doc_max_page - doc_min_page + 1) as i64 } else { 0 };
    let mut arr: Vec<Value> = Vec::with_capacity(total);
    for (i, c) in chunks.iter().enumerate() {
        let meta = json!({
            "schema_name": "docling_core.transforms.chunker.DocMeta",
            "version": "1.0.0",
            "start_page": c.start_page,
            "end_page": c.end_page,
            "page_count": doc_page_count,
            "chunk_index": i as i64,
            "total_chunks": total as i64,
            "token_count": c.token_count as i64,
            "has_major_heading": c.has_major_heading,
            "min_heading_level": c.min_heading_level,
            "origin": {
                "mimetype": "text/html",
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

