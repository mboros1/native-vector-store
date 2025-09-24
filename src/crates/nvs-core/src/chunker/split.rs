use super::pack::TmpChunk;
use super::TokenCounter;

pub fn split_oversized(
    chunks: Vec<TmpChunk>,
    max_tokens: usize,
    tokenizer: &dyn TokenCounter,
) -> Vec<TmpChunk> {
    let mut out = Vec::new();
    for ch in chunks.into_iter() {
        if ch.tokens <= max_tokens {
            out.push(ch);
            continue;
        }
        let mut cur = TmpChunk {
            text: String::new(),
            tokens: 0,
            start_page: ch.start_page,
            end_page: ch.end_page,
            has_major_heading: ch.has_major_heading,
            min_heading_level: ch.min_heading_level,
        };
        for line in ch.text.split('\n') {
            let tst = if cur.text.is_empty() {
                format!("{}\n", line)
            } else {
                format!("{}\n{}\n", cur.text.trim_end_matches('\n'), line)
            };
            let tokens = tokenizer.count_tokens(&tst);
            if !cur.text.is_empty() && tokens > max_tokens {
                // finalize current
                cur.tokens = tokenizer.count_tokens(&cur.text);
                out.push(cur);
                cur = TmpChunk {
                    text: String::new(),
                    tokens: 0,
                    start_page: ch.start_page,
                    end_page: ch.end_page,
                    has_major_heading: ch.has_major_heading,
                    min_heading_level: ch.min_heading_level,
                };
                cur.text = format!("{}\n", line);
                cur.tokens = tokenizer.count_tokens(&cur.text);
            } else {
                cur.text = tst;
                cur.tokens = tokens;
            }
        }
        if !cur.text.is_empty() {
            cur.tokens = tokenizer.count_tokens(&cur.text);
            out.push(cur);
        }
    }
    out
}
