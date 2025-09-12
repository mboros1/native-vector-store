use super::Chunk;
use anyhow::Result;
use serde_json::{json, Value};
use std::fs::File;
use std::io::Write;
use std::path::Path;
use xxhash_rust::xxh64::xxh64;

// Shared JSON writer for Docling-style chunk output.
// mimetype should be e.g. "application/pdf" or "text/html".
pub fn write_chunks_json_with_mimetype(
    doc_path: &Path,
    mimetype: &str,
    chunks: &[Chunk],
    out_path: &Path,
) -> Result<()> {
    let filename = doc_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();
    let path_str = doc_path.to_string_lossy();
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
    let doc_page_count: i64 = if doc_max_page >= doc_min_page {
        (doc_max_page - doc_min_page + 1) as i64
    } else {
        0
    };

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
                "mimetype": mimetype,
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn make_chunk(text: &str, tokens: usize, sp: i32, ep: i32) -> Chunk {
        Chunk {
            text: text.to_string(),
            token_count: tokens,
            start_page: sp,
            end_page: ep,
            has_major_heading: false,
            min_heading_level: 0,
        }
    }

    #[test]
    fn writes_expected_schema_and_fields() {
        let dir = tempdir().unwrap();
        let out_path = dir.path().join("out.json");
        // The doc path only contributes name/hash; it need not exist
        let doc_path = dir.path().join("sample.html");

        let chunks = vec![
            make_chunk("Hello", 2, 2, 2),
            make_chunk("World", 3, 3, 4),
        ];

        write_chunks_json_with_mimetype(&doc_path, "text/html", &chunks, &out_path).unwrap();

        let data = std::fs::read_to_string(&out_path).unwrap();
        let v: Value = serde_json::from_str(&data).unwrap();
        assert!(v.is_array());
        let arr = v.as_array().unwrap();
        assert_eq!(arr.len(), 2);

        // page_count should be max(end_page) - min(start_page) + 1 = 4 - 2 + 1 = 3
        let expected_page_count = 3i64;

        for (i, item) in arr.iter().enumerate() {
            let obj = item.as_object().unwrap();
            assert_eq!(obj.get("text").unwrap().as_str().unwrap(), if i == 0 { "Hello" } else { "World" });
            let meta = obj.get("meta").unwrap().as_object().unwrap();
            assert_eq!(meta.get("schema_name").unwrap().as_str().unwrap(), "docling_core.transforms.chunker.DocMeta");
            assert_eq!(meta.get("version").unwrap().as_str().unwrap(), "1.0.0");
            assert_eq!(meta.get("chunk_index").unwrap().as_i64().unwrap(), i as i64);
            assert_eq!(meta.get("total_chunks").unwrap().as_i64().unwrap(), 2);
            assert_eq!(meta.get("page_count").unwrap().as_i64().unwrap(), expected_page_count);
            let origin = meta.get("origin").unwrap().as_object().unwrap();
            assert_eq!(origin.get("mimetype").unwrap().as_str().unwrap(), "text/html");
            assert_eq!(origin.get("filename").unwrap().as_str().unwrap(), "sample.html");
            // Verify binary_hash is as expected for the provided path string
            let path_str = doc_path.to_string_lossy();
            let expected_hash = xxh64(path_str.as_bytes(), 0) as i64;
            assert_eq!(origin.get("binary_hash").unwrap().as_i64().unwrap(), expected_hash);
        }
    }
}
