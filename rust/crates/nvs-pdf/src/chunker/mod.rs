// Chunker orchestrator and traits

use crate::ChunkOptions;
use std::time::Instant;

mod annotate;
mod group;
mod pack;
mod overlap;
mod merge;
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

impl TokenCounter for tokenmonster::GreedyTokenizer {
    fn count_tokens(&self, text: &str) -> usize { self.count_tokens(text) }
}

// Allow passing a global Lazy<GreedyTokenizer> directly (used by CLI bins)
impl TokenCounter for once_cell::sync::Lazy<tokenmonster::GreedyTokenizer> {
    fn count_tokens(&self, text: &str) -> usize {
        // Call the underlying GreedyTokenizer implementation
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

pub fn chunk_pages(pages: &[(String, i32)], tokenizer: &dyn TokenCounter, opts: &ChunkOptions) -> Vec<Chunk> {
    if pages.is_empty() { return Vec::new(); }

    let annotated = annotate_lines(pages, tokenizer);
    let semantic_units = group_semantic_units(&annotated);
    let mut chunks = pack_initial_chunks(&semantic_units, opts.max_tokens);
    add_overlap(&mut chunks, opts.overlap_tokens, tokenizer);
    chunks = merge_small_chunks(chunks, opts.min_tokens, opts.max_tokens);
    chunks = split_oversized(chunks, opts.max_tokens, tokenizer);
    chunks = final_merge(chunks, opts.min_tokens, opts.max_tokens);
    chunks
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

pub fn chunk_pages_with_stats(pages: &[(String, i32)], tokenizer: &dyn TokenCounter, opts: &ChunkOptions) -> (Vec<Chunk>, ChunkerStats) {
    let t0 = Instant::now();
    let ta = Instant::now();
    let annotated = annotate_lines(pages, tokenizer);
    let ta_ms = ta.elapsed().as_millis();

    let tg = Instant::now();
    let semantic_units = group_semantic_units(&annotated);
    let tg_ms = tg.elapsed().as_millis();

    let tp = Instant::now();
    let mut chunks = pack_initial_chunks(&semantic_units, opts.max_tokens);
    let tp_ms = tp.elapsed().as_millis();

    let to = Instant::now();
    add_overlap(&mut chunks, opts.overlap_tokens, tokenizer);
    let to_ms = to.elapsed().as_millis();

    let tm = Instant::now();
    chunks = merge_small_chunks(chunks, opts.min_tokens, opts.max_tokens);
    let tm_ms = tm.elapsed().as_millis();

    let ts = Instant::now();
    chunks = split_oversized(chunks, opts.max_tokens, tokenizer);
    let ts_ms = ts.elapsed().as_millis();

    let tf = Instant::now();
    chunks = final_merge(chunks, opts.min_tokens, opts.max_tokens);
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
