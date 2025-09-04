pub mod extract;
pub mod chunker;
pub mod json;

use anyhow::Result;
use std::path::Path;
use tokenmonster::GreedyTokenizer;

#[derive(Clone, Debug)]
pub struct ChunkOptions {
    pub max_tokens: usize,
    pub min_tokens: usize,
    pub overlap_tokens: usize,
    pub thread_count: usize, // 0 = auto
    pub page_limit: Option<usize>,
}

impl Default for ChunkOptions {
    fn default() -> Self {
        Self {
            max_tokens: 512,
            min_tokens: 150,
            overlap_tokens: 50,
            thread_count: 0,
            page_limit: None,
        }
    }
}

// Convenience: end-to-end chunking from a PDF path to chunk objects
pub fn parse_to_chunks(pdf_path: &Path, opts: &ChunkOptions) -> Result<Vec<chunker::Chunk>> {
    let tokenizer = GreedyTokenizer::from_cl100k_bin();
    let pages = extract::extract_text_pages(pdf_path, opts.page_limit, opts.thread_count)?;
    let chunks = chunker::chunk_pages(&pages, &tokenizer, opts);
    Ok(chunks)
}

// Convenience: write JSON array matching fast-pdf-parser chunker output
pub fn write_chunks_json(pdf_path: &Path, chunks: &[chunker::Chunk], out_path: &Path) -> Result<()> {
    json::write_chunks_json(pdf_path, chunks, out_path)
}

