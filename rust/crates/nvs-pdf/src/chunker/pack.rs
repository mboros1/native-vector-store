use super::{Chunk, SemanticUnit};

pub fn pack_initial_chunks(units: &[SemanticUnit], max_tokens: usize) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    let mut cur = Chunk { text: String::new(), token_count: 0, start_page: -1, end_page: -1, has_major_heading: false, min_heading_level: i32::MAX };
    for u in units {
        if cur.token_count > 0 && cur.token_count + u.total_tokens > max_tokens {
            chunks.push(cur);
            cur = Chunk { text: String::new(), token_count: 0, start_page: -1, end_page: -1, has_major_heading: false, min_heading_level: i32::MAX };
        }
        if cur.start_page == -1 { cur.start_page = u.pages.0; }
        cur.end_page = u.pages.1;
        if u.has_major_heading { cur.has_major_heading = true; cur.min_heading_level = cur.min_heading_level.min(u.min_heading_level); }
        for l in &u.lines { cur.text.push_str(&l.text); cur.text.push('\n'); }
        cur.token_count += u.total_tokens;
    }
    if cur.token_count > 0 { chunks.push(cur); }
    chunks
}

