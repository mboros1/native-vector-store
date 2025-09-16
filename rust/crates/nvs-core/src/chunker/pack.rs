use super::SemanticUnit;

#[derive(Clone, Debug)]
pub struct TmpChunk {
    pub text: String,
    pub tokens: usize,
    pub start_page: i32,
    pub end_page: i32,
    pub has_major_heading: bool,
    pub min_heading_level: i32,
}

pub fn pack_initial_chunks(units: &[SemanticUnit], max_tokens: usize) -> Vec<TmpChunk> {
    let mut out: Vec<TmpChunk> = Vec::new();
    let mut cur = TmpChunk {
        text: String::new(),
        tokens: 0,
        start_page: -1,
        end_page: -1,
        has_major_heading: false,
        min_heading_level: i32::MAX,
    };
    for u in units {
        let t = u.text();
        let new_tokens = cur.tokens + u.total_tokens;
        if !cur.text.is_empty() && new_tokens > max_tokens {
            out.push(cur);
            cur = TmpChunk {
                text: String::new(),
                tokens: 0,
                start_page: -1,
                end_page: -1,
                has_major_heading: false,
                min_heading_level: i32::MAX,
            };
        }
        if cur.start_page < 0 {
            cur.start_page = *u.pages.iter().next().unwrap_or(&0);
        }
        if let Some(last) = u.pages.iter().next_back() {
            cur.end_page = *last;
        }
        cur.text.push_str(&t);
        cur.tokens += u.total_tokens;
        if u.has_major_heading {
            cur.has_major_heading = true;
            cur.min_heading_level = cur.min_heading_level.min(u.min_heading_level);
        }
    }
    if !cur.text.is_empty() {
        out.push(cur);
    }
    out
}
