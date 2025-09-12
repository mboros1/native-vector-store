pub mod extract;
pub mod json;

use anyhow::Result;
use std::path::Path;
use tokenmonster::GreedyTokenizer;

#[derive(Clone, Debug)]
pub struct HtmlChunkOptions {
    pub max_tokens: usize,
    pub min_tokens: usize,
    pub overlap_tokens: usize,
    pub section_limit: Option<usize>,
}

impl Default for HtmlChunkOptions {
    fn default() -> Self {
        Self { max_tokens: 512, min_tokens: 150, overlap_tokens: 50, section_limit: None }
    }
}

/// Parse an HTML file and chunk its sections using the shared chunker.
///
/// Example (no_run)
/// ```no_run
/// let path = std::path::Path::new("/path/to/file.html");
/// let chunks = nvs_html::parse_to_chunks(path, &nvs_html::HtmlChunkOptions::default())?;
/// println!("chunks={} first_len={}", chunks.len(), chunks.get(0).map(|c| c.token_count).unwrap_or(0));
/// # anyhow::Ok(())
/// ```
pub fn parse_to_chunks(html_path: &Path, opts: &HtmlChunkOptions) -> Result<Vec<nvs_core::chunker::Chunk>> {
    Ok(parse_to_chunks_with_stats(html_path, opts)?.0)
}

#[derive(Clone, Debug, Default)]
pub struct ChunkStats {
    pub sections: usize,
    pub chunks: usize,
    pub extract_ms: u128,
    pub chunk_ms: u128,
    pub total_ms: u128,
    pub annotate_ms: u128,
    pub group_ms: u128,
    pub pack_ms: u128,
    pub overlap_ms: u128,
    pub merge_ms: u128,
    pub split_ms: u128,
    pub final_ms: u128,
}

/// Like [`parse_to_chunks`], but also returns timing breakdowns of the pipeline.
///
/// Example (no_run)
/// ```no_run
/// let path = std::path::Path::new("/path/to/file.html");
/// let (chunks, stats) = nvs_html::parse_to_chunks_with_stats(path, &nvs_html::HtmlChunkOptions::default())?;
/// println!("chunks={} total_ms={}", chunks.len(), stats.total_ms);
/// # anyhow::Ok(())
/// ```
pub fn parse_to_chunks_with_stats(html_path: &Path, opts: &HtmlChunkOptions) -> Result<(Vec<nvs_core::chunker::Chunk>, ChunkStats)> {
    use std::time::Instant;
    let t0 = Instant::now();
    let tokenizer = GreedyTokenizer::from_cl100k_bin();
    let (sections, estats) = nvs_html_core::fast_extract_sections_with_stats(html_path, opts.section_limit)?;
    let (chunks, cstats) = nvs_core::chunker::chunk_pages_with_stats(&sections, &tokenizer, &nvs_core::chunker::ChunkOptions {
        max_tokens: opts.max_tokens,
        min_tokens: opts.min_tokens,
        overlap_tokens: opts.overlap_tokens,
    });
    let t3 = Instant::now();
    let stats = ChunkStats {
        sections: sections.len(),
        chunks: chunks.len(),
        extract_ms: estats.total_ms,
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

/// Write chunks to a JSON file with `mimetype:"text/html"` in the header.
///
/// Example (no_run)
/// ```no_run
/// let path = std::path::Path::new("/path/to/file.html");
/// let out = std::path::Path::new("/tmp/out.json");
/// let chunks = nvs_html::parse_to_chunks(path, &nvs_html::HtmlChunkOptions::default())?;
/// nvs_html::write_chunks_json(path, &chunks, out)?;
/// # anyhow::Ok(())
/// ```
pub fn write_chunks_json(html_path: &Path, chunks: &[nvs_core::chunker::Chunk], out_path: &Path) -> Result<()> {
    json::write_chunks_json(html_path, chunks, out_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn parse_and_write_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let html_path = dir.path().join("sample.html");
        fs::write(&html_path, "<html><body><h1>Title</h1><p>Hello world.</p></body></html>").unwrap();
        let opts = HtmlChunkOptions { max_tokens: 64, min_tokens: 1, overlap_tokens: 0, section_limit: None };
        let (chunks, _stats) = parse_to_chunks_with_stats(&html_path, &opts).unwrap();
        assert!(!chunks.is_empty());
        let out = dir.path().join("out.json");
        write_chunks_json(&html_path, &chunks, &out).unwrap();
        let data = fs::read_to_string(&out).unwrap();
        let v: serde_json::Value = serde_json::from_str(&data).unwrap();
        assert!(v.is_array());
    }
}
