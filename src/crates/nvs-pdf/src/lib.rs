pub mod extract;
pub mod json;
pub mod orchestrator;
// pdfium binding handled within extractor for now

use anyhow::Result;
pub use nvs_core::chunker::ChunkerStats;
use std::path::Path;
use tokenmonster::GreedyTokenizer;

#[derive(Clone, Debug)]
pub struct PdfChunkOptions {
    pub max_tokens: usize,
    pub min_tokens: usize,
    pub overlap_tokens: usize,
    pub thread_count: usize, // 0 = auto
    pub page_limit: Option<usize>,
}

impl Default for PdfChunkOptions {
    fn default() -> Self {
        Self {
            max_tokens: 256,
            min_tokens: 150,
            overlap_tokens: 50,
            thread_count: 0,
            page_limit: None,
        }
    }
}

/// End-to-end chunking from a PDF path to chunk objects.
///
/// Example (no_run)
/// ```no_run
/// let path = std::path::Path::new("/path/to/file.pdf");
/// let chunks = nvs_pdf::parse_to_chunks(path, &nvs_pdf::PdfChunkOptions::default())?;
/// println!("{} chunks", chunks.len());
/// # anyhow::Ok(())
/// ```
pub fn parse_to_chunks(
    pdf_path: &Path,
    opts: &PdfChunkOptions,
) -> Result<Vec<nvs_core::chunker::Chunk>> {
    Ok(parse_to_chunks_with_stats(pdf_path, opts)?.0)
}

#[derive(Clone, Debug, Default)]
pub struct ChunkStats {
    pub pages: usize,
    pub chunks: usize,
    pub extract_ms: u128,
    pub extract_bind_open_ms: u128,
    pub extract_pages_ms: u128,
    pub chunk_ms: u128,
    pub total_ms: u128,
    // Breakdown of chunking stages
    pub annotate_ms: u128,
    pub group_ms: u128,
    pub pack_ms: u128,
    pub overlap_ms: u128,
    pub merge_ms: u128,
    pub split_ms: u128,
    pub final_ms: u128,
}

/// Like [`parse_to_chunks`], but also returns timing breakdowns.
///
/// Example (no_run)
/// ```no_run
/// let (chunks, stats) = nvs_pdf::parse_to_chunks_with_stats(std::path::Path::new("/path.pdf"), &nvs_pdf::PdfChunkOptions::default())?;
/// println!("chunks={} total_ms={}", chunks.len(), stats.total_ms);
/// # anyhow::Ok(())
/// ```
pub fn parse_to_chunks_with_stats(
    pdf_path: &Path,
    opts: &PdfChunkOptions,
) -> Result<(Vec<nvs_core::chunker::Chunk>, ChunkStats)> {
    use std::time::Instant;
    let t0 = Instant::now();
    let tokenizer = GreedyTokenizer::from_cl100k_bin();
    let (pages, estats) = extract::extract_text_pages_with_stats(pdf_path, opts.page_limit, 0)?;
    let (chunks, cstats) = nvs_core::chunker::chunk_pages_with_stats(
        &pages,
        &tokenizer,
        &nvs_core::chunker::ChunkOptions {
            max_tokens: opts.max_tokens,
            min_tokens: opts.min_tokens,
            overlap_tokens: opts.overlap_tokens,
        },
    );
    let t3 = Instant::now();
    let stats = ChunkStats {
        pages: pages.len(),
        chunks: chunks.len(),
        extract_ms: estats.total_ms,
        extract_bind_open_ms: estats.bind_open_ms,
        extract_pages_ms: estats.pages_ms,
        chunk_ms: cstats.total_ms,
        total_ms: (t3 - t0).as_millis(),
        annotate_ms: cstats.annotate_ms,
        group_ms: cstats.group_ms,
        pack_ms: cstats.pack_ms,
        overlap_ms: cstats.overlap_ms,
        merge_ms: cstats.merge_ms,
        split_ms: cstats.split_ms,
        final_ms: cstats.final_ms,
    };
    Ok((chunks, stats))
}

/// Write JSON array matching fast-pdf-parser chunk schema.
///
/// Example (no_run)
/// ```no_run
/// let path = std::path::Path::new("/path.pdf");
/// let out = std::path::Path::new("/tmp/chunks.json");
/// let chunks = nvs_pdf::parse_to_chunks(path, &nvs_pdf::PdfChunkOptions::default())?;
/// nvs_pdf::write_chunks_json(path, &chunks, out)?;
/// # anyhow::Ok(())
/// ```
pub fn write_chunks_json(
    pdf_path: &Path,
    chunks: &[nvs_core::chunker::Chunk],
    out_path: &Path,
) -> Result<()> {
    json::write_chunks_json(pdf_path, chunks, out_path)
}
