// minimal path: no external helpers

use crate::ChunkOptions;
use tokenmonster::GreedyTokenizer;
use std::time::Instant;

#[derive(Clone, Debug)]
pub struct Chunk {
    pub text: String,
    pub token_count: usize,
    pub start_page: i32,
    pub end_page: i32,
    pub has_major_heading: bool,
    pub min_heading_level: i32,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum LineType { Normal, MajorHeading, MinorHeading, ListItem, Blank, Code }

#[derive(Clone, Debug)]
struct AnnotatedLine {
    text: String,
    kind: LineType,
    tokens: usize,
    page: i32,
    heading_level: i32,
}

pub fn chunk_pages(pages: &[(String, i32)], tokenizer: &GreedyTokenizer, opts: &ChunkOptions) -> Vec<Chunk> {
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

pub fn chunk_pages_with_stats(pages: &[(String, i32)], tokenizer: &GreedyTokenizer, opts: &ChunkOptions) -> (Vec<Chunk>, ChunkerStats) {
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

fn annotate_lines(pages: &[(String, i32)], tokenizer: &GreedyTokenizer) -> Vec<AnnotatedLine> {
    let mut out = Vec::new();
    for (page_text, page) in pages.iter() {
        for line in page_text.split('\n') {
            let (kind, lvl) = detect_line_type(line);
            let tokens = tokenizer.count_tokens(line);
            out.push(AnnotatedLine {
                text: line.to_owned(),
                kind,
                tokens,
                page: *page,
                heading_level: lvl,
            });
        }
    }
    out
}

fn detect_line_type(line: &str) -> (LineType, i32) {
    // Blank
    if line.bytes().all(|b| b.is_ascii_whitespace()) { return (LineType::Blank, 0); }

    // Headings: leading '#'s followed by space
    let bytes = line.as_bytes();
    let mut i = 0usize; while i < bytes.len() && bytes[i] == b'#' { i += 1; }
    if i > 0 {
        if i <= 6 && (i < bytes.len()) && bytes[i] == b' ' { return (if i <= 2 { LineType::MajorHeading } else { LineType::MinorHeading }, i as i32); }
    }

    // List items: "- ", "* ", digit+". ", common bullets
    let s = line.trim_start();
    if s.starts_with("- ") || s.starts_with("* ") || s.starts_with("• ") { return (LineType::ListItem, 0); }
    // digit+". "
    let mut di = 0usize; let sb = s.as_bytes();
    while di < sb.len() && sb[di].is_ascii_digit() { di += 1; }
    if di > 0 && di + 1 < sb.len() && sb[di] == b'.' && sb[di+1] == b' ' { return (LineType::ListItem, 0); }

    // Code block heuristic: contains ``` or starts with 2+ spaces (indented)
    if s.contains("```") { return (LineType::Code, 0); }
    if line.starts_with("  ") { return (LineType::Code, 0); }

    (LineType::Normal, 0)
}

#[derive(Clone, Debug)]
struct SemanticUnit {
    lines: Vec<AnnotatedLine>,
    total_tokens: usize,
    pages: (i32, i32),
    has_major_heading: bool,
    min_heading_level: i32,
}

fn group_semantic_units(lines: &[AnnotatedLine]) -> Vec<SemanticUnit> {
    let mut units = Vec::new();
    let mut cur: Option<SemanticUnit> = None;
    for l in lines {
        if cur.is_none() {
            cur = Some(SemanticUnit {
                lines: Vec::new(),
                total_tokens: 0,
                pages: (l.page, l.page),
                has_major_heading: false,
                min_heading_level: i32::MAX,
            });
        }
        let c = cur.as_mut().unwrap();
        c.pages.0 = c.pages.0.min(l.page);
        c.pages.1 = c.pages.1.max(l.page);
        if l.kind == LineType::MajorHeading { c.has_major_heading = true; c.min_heading_level = c.min_heading_level.min(l.heading_level); }
        c.total_tokens += l.tokens;
        c.lines.push(l.clone());

        // Break on blank to avoid gluing disparate blocks
        if l.kind == LineType::Blank {
            units.push(c.clone());
            cur = None;
        }
    }
    if let Some(c) = cur { if !c.lines.is_empty() { units.push(c); } }
    units
}

fn pack_initial_chunks(units: &[SemanticUnit], max_tokens: usize) -> Vec<Chunk> {
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

fn add_overlap(chunks: &mut [Chunk], overlap_tokens: usize, tokenizer: &GreedyTokenizer) {
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
            // drain at a char boundary
            while step < overlap.len() && !overlap.is_char_boundary(step) { step += 1; }
            overlap.drain(..step);
        }
        // Prepend overlap to current chunk text, adjust count
        let delta = tokenizer.count_tokens(&overlap);
        chunks[i].text = format!("{}{}", overlap, chunks[i].text);
        chunks[i].token_count = chunks[i].token_count.saturating_add(delta);
    }
}

fn merge_small_chunks(chunks: Vec<Chunk>, min_tokens: usize, max_tokens: usize) -> Vec<Chunk> {
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

fn split_oversized(chunks: Vec<Chunk>, max_tokens: usize, tokenizer: &GreedyTokenizer) -> Vec<Chunk> {
    fn split_line_by_tokens(line: &str, max_tokens: usize, tokenizer: &GreedyTokenizer) -> Vec<String> {
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
            if cur.is_empty() { cur.push_str(w); cur_tok = wtok; }
            else { cur.push(' '); cur.push_str(w); cur_tok += 1 + wtok; }
            if wi + 1 == words.len() { parts.push(cur.clone()); cur.clear(); cur_tok = 0; }
        }
        if parts.is_empty() { parts.push(cur); }
        parts
    }

    let mut out = Vec::new();
    for c in chunks {
        if c.token_count <= max_tokens { out.push(c); continue; }
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

fn final_merge(chunks: Vec<Chunk>, min_tokens: usize, max_tokens: usize) -> Vec<Chunk> {
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
