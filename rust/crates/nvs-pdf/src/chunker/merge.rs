use super::Chunk;

pub fn merge_small_chunks(chunks: Vec<Chunk>, min_tokens: usize, max_tokens: usize) -> Vec<Chunk> {
    if chunks.is_empty() { return chunks; }
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < chunks.len() {
        let mut cur = chunks[i].clone();
        while cur.token_count < min_tokens && i + 1 < chunks.len() {
            let next = &chunks[i + 1];
            let combined = cur.token_count + next.token_count;
            let mut allow = combined <= max_tokens;
            if !allow && combined <= (max_tokens as f32 * 1.1) as usize && next.token_count < min_tokens / 2 {
                allow = true;
            }
            if next.has_major_heading && next.min_heading_level <= 2 && cur.token_count >= min_tokens / 2 {
                allow = false;
            }
            if !allow { break; }
            cur.text.push_str(&next.text);
            cur.token_count = combined;
            cur.end_page = next.end_page;
            if next.has_major_heading { cur.has_major_heading = true; cur.min_heading_level = cur.min_heading_level.min(next.min_heading_level); }
            i += 1;
        }
        out.push(cur);
        i += 1;
    }
    out
}

pub fn final_merge(chunks: Vec<Chunk>, min_tokens: usize, max_tokens: usize) -> Vec<Chunk> {
    if chunks.is_empty() { return chunks; }
    let mut out: Vec<Chunk> = Vec::new();
    let mut i = 0usize;
    while i < chunks.len() {
        let mut cur = chunks[i].clone();
        while cur.token_count < min_tokens && i + 1 < chunks.len() {
            let next = &chunks[i + 1];
            let combined = cur.token_count + next.token_count;
            if combined <= max_tokens {
                cur.text.push_str(&next.text);
                cur.token_count = combined;
                cur.end_page = next.end_page;
                if next.has_major_heading { cur.has_major_heading = true; cur.min_heading_level = cur.min_heading_level.min(next.min_heading_level); }
                i += 1;
            } else { break; }
        }
        if cur.token_count < min_tokens && !out.is_empty() {
            let prev = out.last_mut().unwrap();
            let combined = prev.token_count + cur.token_count;
            if combined <= max_tokens {
                prev.text.push_str(&cur.text);
                prev.token_count = combined;
                prev.end_page = cur.end_page;
                if cur.has_major_heading { prev.has_major_heading = true; prev.min_heading_level = prev.min_heading_level.min(cur.min_heading_level); }
                i += 1; continue;
            }
        }
        out.push(cur);
        i += 1;
    }
    out
}

