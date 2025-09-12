use anyhow::Result;
use std::path::Path;
use nvs_core::chunker::Chunk;

pub fn write_chunks_json(html_path: &Path, chunks: &[Chunk], out_path: &Path) -> Result<()> {
    nvs_core::chunker::json::write_chunks_json_with_mimetype(html_path, "text/html", chunks, out_path)
}
