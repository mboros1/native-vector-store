// Shared chunker: line annotation -> semantic grouping -> packing -> overlap -> merge/split
// This module was moved from nvs-pdf to nvs-core to be reused by HTML and PDF pipelines.

use std::time::Instant;

mod annotate;
mod group;
mod merge;
mod overlap;
mod pack;
mod split;

pub use annotate::{annotate_lines, AnnotatedLine, LineType};
pub use group::{group_semantic_units, SemanticUnit};
pub use merge::{final_merge, merge_small_chunks};
pub use overlap::add_overlap;
pub use pack::pack_initial_chunks;
pub use split::split_oversized;

// Abstraction: token counting, to swap implementations later.
pub trait TokenCounter {
    fn count_tokens(&self, text: &str) -> usize;
}

// Implement TokenCounter for the GreedyTokenizer from the tokenmonster crate
impl TokenCounter for tokenmonster::GreedyTokenizer {
    fn count_tokens(&self, text: &str) -> usize {
        self.count_tokens(text)
    }
}

// Allow passing a global Lazy<GreedyTokenizer> directly (used by CLI bins)
impl TokenCounter for once_cell::sync::Lazy<tokenmonster::GreedyTokenizer> {
    fn count_tokens(&self, text: &str) -> usize {
        tokenmonster::GreedyTokenizer::count_tokens(&*self, text)
    }
}

#[derive(Clone, Debug)]
pub struct Chunk {
    pub text: String,
    pub token_count: usize,
    pub start_page: i32,
    pub end_page: i32,
    pub has_major_heading: bool,
    pub min_heading_level: i32,
}

#[derive(Clone, Debug)]
pub struct ChunkOptions {
    pub max_tokens: usize,
    pub min_tokens: usize,
    pub overlap_tokens: usize,
}

impl Default for ChunkOptions {
    fn default() -> Self {
        Self {
            max_tokens: 256,
            min_tokens: 150,
            overlap_tokens: 50,
        }
    }
}

/// Chunk a list of (text, page_index) into semantic chunks.
///
/// Example
/// ```
/// use nvs_core::chunker::{chunk_pages, ChunkOptions};
/// let pages = vec![
///     ("# Title\nIntro paragraph.".to_string(), 0),
///     ("## Section\nSome content here.".to_string(), 1),
/// ];
/// let tok = tokenmonster::GreedyTokenizer::from_cl100k_bin();
/// let opts = ChunkOptions { max_tokens: 64, min_tokens: 8, overlap_tokens: 4 };
/// let chunks = chunk_pages(&pages, &tok, &opts);
/// assert!(!chunks.is_empty());
/// assert!(chunks[0].token_count > 0);
/// ```
pub fn chunk_pages(
    pages: &[(String, i32)],
    tokenizer: &dyn TokenCounter,
    opts: &ChunkOptions,
) -> Vec<Chunk> {
    if pages.is_empty() {
        return Vec::new();
    }
    // Pre-filter: strip repeated headers/footers that appear on most pages.
    let pages = strip_repeated_headers_footers(pages);
    let annotated = annotate_lines(&pages, tokenizer);
    let semantic_units = group_semantic_units(&annotated);
    let mut tmp = pack_initial_chunks(&semantic_units, opts.max_tokens);
    add_overlap(&mut tmp, opts.overlap_tokens, tokenizer);
    tmp = merge_small_chunks(tmp, opts.min_tokens, opts.max_tokens);
    tmp = split_oversized(tmp, opts.max_tokens, tokenizer);
    let chunks = final_merge(tmp, opts.min_tokens, opts.max_tokens);
    chunks
}

// Heuristic: remove identical header/footer lines that repeat across a large
// fraction of pages (e.g., journal footers, page numbers). This improves RAG
// quality by reducing noise and duplication.
fn strip_repeated_headers_footers(pages: &[(String, i32)]) -> Vec<(String, i32)> {
    use std::collections::HashMap;
    let mut first_map: HashMap<String, usize> = HashMap::new();
    let mut last_map: HashMap<String, usize> = HashMap::new();
    let mut heads: Vec<Option<String>> = Vec::with_capacity(pages.len());
    let mut foots: Vec<Option<String>> = Vec::with_capacity(pages.len());
    for (txt, _p) in pages {
        let mut lines: Vec<&str> = txt.split('\n').collect();
        // find first non-empty
        let first = lines.iter().find(|l| !l.trim().is_empty()).map(|s| s.trim());
        // find last non-empty
        let last = lines.iter().rev().find(|l| !l.trim().is_empty()).map(|s| s.trim());
        heads.push(first.map(|s| s.to_string()));
        foots.push(last.map(|s| s.to_string()));
        if let Some(h) = &heads.last().unwrap() {
            if h.len() > 3 { *first_map.entry(h.clone()).or_insert(0) += 1; }
        }
        if let Some(f) = &foots.last().unwrap() {
            if f.len() > 3 { *last_map.entry(f.clone()).or_insert(0) += 1; }
        }
    }
    let n = pages.len().max(1);
    let threshold = (n as f32 * 0.6).ceil() as usize; // 60% of pages
    let common_head: Option<String> = first_map
        .into_iter()
        .filter(|(s, c)| *c >= threshold && !is_mostly_numeric(s))
        .max_by_key(|(_, c)| *c)
        .map(|(s, _)| s);
    let common_foot: Option<String> = last_map
        .into_iter()
        .filter(|(s, c)| *c >= threshold && !is_mostly_numeric(s))
        .max_by_key(|(_, c)| *c)
        .map(|(s, _)| s);

    let mut out = Vec::with_capacity(pages.len());
    for ((txt, p), h) in pages.iter().zip(heads.into_iter()) {
        let f = foots.remove(0);
        let mut lines: Vec<&str> = txt.split('\n').collect();
        if let (Some(ch), Some(hs)) = (&common_head, &h) {
            if hs == ch { if let Some(idx) = lines.iter().position(|l| l.trim() == hs) { lines.remove(idx); } }
        }
        if let (Some(cf), Some(fs)) = (&common_foot, &f) {
            if fs == cf { if let Some(idx) = lines.iter().rposition(|l| l.trim() == fs) { lines.remove(idx); } }
        }
        out.push((lines.join("\n"), *p));
    }
    out
}

fn is_mostly_numeric(s: &str) -> bool {
    let trimmed = s.trim();
    if trimmed.is_empty() { return true; }
    let digits = trimmed.chars().filter(|c| c.is_ascii_digit()).count();
    digits * 2 >= trimmed.chars().count() // >= 50% digits
}

#[derive(Clone, Copy, Default, Debug)]
pub struct ChunkerStats {
    pub annotate_ms: u128,
    pub group_ms: u128,
    pub pack_ms: u128,
    pub overlap_ms: u128,
    pub merge_ms: u128,
    pub split_ms: u128,
    pub final_ms: u128,
    pub total_ms: u128,
}

/// Like [`chunk_pages`], but also returns timing breakdowns of each stage.
///
/// Example
/// ```
/// use nvs_core::chunker::{chunk_pages_with_stats, ChunkOptions};
/// let pages = vec![("# H\nBody".to_string(), 0)];
/// let tok = tokenmonster::GreedyTokenizer::from_cl100k_bin();
/// let opts = ChunkOptions { max_tokens: 64, min_tokens: 8, overlap_tokens: 0 };
/// let (chunks, stats) = chunk_pages_with_stats(&pages, &tok, &opts);
/// assert_eq!(chunks.len() > 0, true);
/// assert!(stats.total_ms >= 0);
/// ```
pub fn chunk_pages_with_stats(
    pages: &[(String, i32)],
    tokenizer: &dyn TokenCounter,
    opts: &ChunkOptions,
) -> (Vec<Chunk>, ChunkerStats) {
    let t0 = Instant::now();
    let ta = Instant::now();
    let annotated = annotate_lines(pages, tokenizer);
    let ta_ms = ta.elapsed().as_millis();

    let tg = Instant::now();
    let semantic_units = group_semantic_units(&annotated);
    let tg_ms = tg.elapsed().as_millis();

    let tp = Instant::now();
    let mut tmp = pack_initial_chunks(&semantic_units, opts.max_tokens);
    let tp_ms = tp.elapsed().as_millis();

    let to = Instant::now();
    add_overlap(&mut tmp, opts.overlap_tokens, tokenizer);
    let to_ms = to.elapsed().as_millis();

    let tm = Instant::now();
    tmp = merge_small_chunks(tmp, opts.min_tokens, opts.max_tokens);
    let tm_ms = tm.elapsed().as_millis();

    let ts = Instant::now();
    tmp = split_oversized(tmp, opts.max_tokens, tokenizer);
    let ts_ms = ts.elapsed().as_millis();

    let tf = Instant::now();
    let chunks = final_merge(tmp, opts.min_tokens, opts.max_tokens);
    let tf_ms = tf.elapsed().as_millis();

    let stats = ChunkerStats {
        annotate_ms: ta_ms,
        group_ms: tg_ms,
        pack_ms: tp_ms,
        overlap_ms: to_ms,
        merge_ms: tm_ms,
        split_ms: ts_ms,
        final_ms: tf_ms,
        total_ms: t0.elapsed().as_millis(),
    };
    (chunks, stats)
}

pub mod json;
