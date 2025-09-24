use anyhow::Result;
use nvs_core::chunker::Chunk;
use std::path::Path;
pub fn write_chunks_json(pdf_path: &Path, chunks: &[Chunk], out_path: &Path) -> Result<()> {
    nvs_core::chunker::json::write_chunks_json_with_mimetype(
        pdf_path,
        "application/pdf",
        chunks,
        out_path,
    )
}
