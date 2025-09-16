use super::pack::TmpChunk;

pub fn merge_small_chunks(
    chunks: Vec<TmpChunk>,
    min_tokens: usize,
    max_tokens: usize,
) -> Vec<TmpChunk> {
    if chunks.is_empty() {
        return chunks;
    }
    let mut out: Vec<TmpChunk> = Vec::new();
    let mut i = 0usize;
    while i < chunks.len() {
        let mut cur = chunks[i].clone();
        while cur.tokens < min_tokens && i + 1 < chunks.len() {
            let next = &chunks[i + 1];
            let combined_tokens = cur.tokens + next.tokens;
            let allow = if combined_tokens <= max_tokens {
                true
            } else {
                false
            };
            if !allow {
                break;
            }
            cur.text.push_str(&next.text);
            cur.tokens = combined_tokens;
            cur.end_page = next.end_page;
            if next.has_major_heading {
                cur.has_major_heading = true;
                cur.min_heading_level = cur.min_heading_level.min(next.min_heading_level);
            }
            i += 1;
        }
        out.push(cur);
        i += 1;
    }
    out
}

pub fn final_merge(
    chunks: Vec<TmpChunk>,
    min_tokens: usize,
    max_tokens: usize,
) -> Vec<super::Chunk> {
    if chunks.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<TmpChunk> = Vec::new();
    let mut i = 0usize;
    while i < chunks.len() {
        let mut cur = chunks[i].clone();
        while cur.tokens < min_tokens && i + 1 < chunks.len() {
            let next = &chunks[i + 1];
            let combined_tokens = cur.tokens + next.tokens;
            if combined_tokens <= max_tokens {
                cur.text.push_str(&next.text);
                cur.tokens = combined_tokens;
                cur.end_page = next.end_page;
                if next.has_major_heading {
                    cur.has_major_heading = true;
                    cur.min_heading_level = cur.min_heading_level.min(next.min_heading_level);
                }
                i += 1;
            } else {
                break;
            }
        }
        if cur.tokens < min_tokens {
            if let Some(prev) = out.last_mut() {
                let combined = prev.tokens + cur.tokens;
                if combined <= max_tokens {
                    prev.text.push_str(&cur.text);
                    prev.tokens = combined;
                    prev.end_page = cur.end_page;
                    if cur.has_major_heading {
                        prev.has_major_heading = true;
                        prev.min_heading_level = prev.min_heading_level.min(cur.min_heading_level);
                    }
                    i += 1;
                    continue;
                }
            }
        }
        out.push(cur);
        i += 1;
    }
    out.into_iter()
        .map(|tc| super::Chunk {
            text: tc.text,
            token_count: tc.tokens,
            start_page: tc.start_page,
            end_page: tc.end_page,
            has_major_heading: tc.has_major_heading,
            min_heading_level: if tc.min_heading_level == i32::MAX {
                0
            } else {
                tc.min_heading_level
            },
        })
        .collect()
}
