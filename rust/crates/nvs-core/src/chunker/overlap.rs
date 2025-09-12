use super::{TokenCounter};
use super::pack::TmpChunk;

pub fn add_overlap(chunks: &mut [TmpChunk], overlap_tokens: usize, tokenizer: &dyn TokenCounter) {
    if overlap_tokens == 0 { return; }
    for i in 1..chunks.len() {
        let prev_text = &chunks[i - 1].text;
        if prev_text.is_empty() { continue; }

        // Heuristic: aim for up to ~5 chars per token from the tail, but ensure char boundaries
        let max_tail_chars = overlap_tokens.saturating_mul(5).max(8);
        // Build char boundary index vector (byte offsets)
        let mut char_pos: Vec<usize> = prev_text.char_indices().map(|(idx, _)| idx).collect();
        char_pos.push(prev_text.len()); // end sentinel for safe slicing
        let total_chars = char_pos.len() - 1; // exclude sentinel
        let start_char_idx = total_chars.saturating_sub(max_tail_chars);
        let mut start_byte = char_pos[start_char_idx];

        // Initial candidate overlap (safe slice)
        let mut overlap = prev_text[start_byte..].to_string();
        // If it’s too many tokens, advance start forward by a few chars at a time
        let mut advance_chars = 5usize; // step size to trim from the front in chars
        let mut start_idx = start_char_idx;
        while tokenizer.count_tokens(&overlap) > overlap_tokens {
            if start_idx + advance_chars >= total_chars {
                // Can't advance further without emptying; break to avoid panic
                break;
            }
            start_idx += advance_chars;
            start_byte = char_pos[start_idx];
            overlap = prev_text[start_byte..].to_string();
            // adapt step down if extremely short string remains
            if overlap.len() < 32 && advance_chars > 1 { advance_chars = 1; }
        }

        // Prepend overlap explicitly to chunk text to influence token_count downstream
        chunks[i].text = format!("{}{}", overlap, chunks[i].text);
        chunks[i].tokens = tokenizer.count_tokens(&chunks[i].text);
    }
}
