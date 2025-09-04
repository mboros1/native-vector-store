use super::{Chunk, TokenCounter};

pub fn add_overlap(chunks: &mut [Chunk], overlap_tokens: usize, tokenizer: &dyn TokenCounter) {
    if overlap_tokens == 0 { return; }
    for i in 1..chunks.len() {
        let prev = &chunks[i - 1].text;
        let tail_chars = overlap_tokens.saturating_mul(5);
        let take = prev.len().min(tail_chars);
        // Ensure start at a UTF-8 char boundary
        let mut start = prev.len().saturating_sub(take);
        while start < prev.len() && !prev.is_char_boundary(start) { start += 1; }
        let mut overlap = prev[start..].to_string();
        while tokenizer.count_tokens(&overlap) > overlap_tokens && overlap.len() > 10 {
            // Trim from the front in small slices to approach target without scanning full text
            let mut step = 10.min(overlap.len());
            while step < overlap.len() && !overlap.is_char_boundary(step) { step += 1; }
            overlap.drain(..step);
        }
        let delta = tokenizer.count_tokens(&overlap);
        chunks[i].text = format!("{}{}", overlap, chunks[i].text);
        chunks[i].token_count = chunks[i].token_count.saturating_add(delta);
    }
}

