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
    Ok(parse_to_chunks_with_stats(pdf_path, opts)?.0)
}

#[derive(Clone, Debug, Default)]
pub struct ChunkStats {
    pub pages: usize,
    pub chunks: usize,
    pub extract_ms: u128,
    pub chunk_ms: u128,
    pub total_ms: u128,
}

pub fn parse_to_chunks_with_stats(pdf_path: &Path, opts: &ChunkOptions) -> Result<(Vec<chunker::Chunk>, ChunkStats)> {
    use std::time::Instant;
    let t0 = Instant::now();
    let tokenizer = GreedyTokenizer::from_cl100k_bin();
    let t1 = Instant::now();
    let pages = extract::extract_text_pages(pdf_path, opts.page_limit, opts.thread_count)?;
    let t2 = Instant::now();
    let chunks = chunker::chunk_pages(&pages, &tokenizer, opts);
    let t3 = Instant::now();
    let stats = ChunkStats {
        pages: pages.len(),
        chunks: chunks.len(),
        extract_ms: (t2 - t1).as_millis(),
        chunk_ms: (t3 - t2).as_millis(),
        total_ms: (t3 - t0).as_millis(),
    };
    Ok((chunks, stats))
}

// Convenience: write JSON array matching fast-pdf-parser chunker output
pub fn write_chunks_json(pdf_path: &Path, chunks: &[chunker::Chunk], out_path: &Path) -> Result<()> {
    json::write_chunks_json(pdf_path, chunks, out_path)
}
