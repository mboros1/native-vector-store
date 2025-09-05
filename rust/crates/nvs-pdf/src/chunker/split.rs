use super::{Chunk, TokenCounter};

pub fn split_oversized(chunks: Vec<Chunk>, max_tokens: usize, tokenizer: &dyn TokenCounter) -> Vec<Chunk> {
    fn split_line_by_tokens(line: &str, max_tokens: usize, tokenizer: &dyn TokenCounter) -> Vec<String> {
        let mut parts: Vec<String> = Vec::new();
        let words: Vec<&str> = line.split_whitespace().collect();
        if words.is_empty() { return vec![String::new()]; }
        // Precompute token counts of words for reuse
        let word_tok: Vec<usize> = words.iter().map(|w| tokenizer.count_tokens(w)).collect();
        let mut cur = String::new();
        let mut cur_tok = 0usize;
        for (wi, w) in words.iter().enumerate() {
            let wtok = word_tok[wi];
            let sep_tok = if cur.is_empty() { 0 } else { 1 }; // approximate one token for a space
            if cur_tok + sep_tok + wtok > max_tokens {
                if !cur.is_empty() {
                    parts.push(cur);
                    cur = String::new();
                    cur_tok = 0;
                }
                if wtok > max_tokens {
                    // Split long word at char boundaries greedily
                    let mut acc = String::new();
                    for ch in w.chars() {
                        let mut candidate = acc.clone();
                        candidate.push(ch);
                        let c = tokenizer.count_tokens(&candidate);
                        if c > max_tokens && !acc.is_empty() {
                            parts.push(acc);
                            acc = ch.to_string();
                        } else {
                            acc = candidate;
                        }
                    }
                    if !acc.is_empty() { parts.push(acc); }
                    continue;
                }
            }
            if cur.is_empty() {
                cur.push_str(w);
                cur_tok = wtok;
            } else {
                cur.push(' ');
                cur.push_str(w);
                cur_tok += 1 + wtok;
            }
            if wi + 1 == words.len() {
                parts.push(cur.clone());
                cur.clear();
                cur_tok = 0;
            }
        }
        if parts.is_empty() { parts.push(cur); }
        parts
    }

    let mut out = Vec::new();
    for c in chunks {
        if c.token_count <= max_tokens {
            out.push(c);
            continue;
        }
        let mut current = Chunk { text: String::new(), token_count: 0, start_page: c.start_page, end_page: c.end_page, has_major_heading: c.has_major_heading, min_heading_level: c.min_heading_level };
        for line in c.text.split('\n') {
            let t = tokenizer.count_tokens(line);
            if t > max_tokens {
                let pieces = split_line_by_tokens(line, max_tokens, tokenizer);
                for p in pieces {
                    let tp = tokenizer.count_tokens(&p);
                    if !current.text.is_empty() && current.token_count + tp > max_tokens {
                        if current.token_count > 0 { out.push(current); }
                        current = Chunk { text: String::new(), token_count: 0, start_page: c.start_page, end_page: c.end_page, has_major_heading: false, min_heading_level: i32::MAX };
                    }
                    current.text.push_str(&p);
                    current.text.push('\n');
                    current.token_count += tp;
                }
                continue;
            }

            if !current.text.is_empty() && current.token_count + t > max_tokens {
                out.push(current);
                current = Chunk { text: String::new(), token_count: 0, start_page: c.start_page, end_page: c.end_page, has_major_heading: false, min_heading_level: i32::MAX };
            }
            current.text.push_str(line);
            current.text.push('\n');
            current.token_count += t;
        }
        if !current.text.is_empty() { out.push(current); }
    }
    out
}

