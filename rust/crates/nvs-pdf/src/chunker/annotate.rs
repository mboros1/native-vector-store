use super::{TokenCounter};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum LineType { Normal, MajorHeading, MinorHeading, ListItem, Blank, Code }

#[derive(Clone, Debug)]
pub struct AnnotatedLine {
    pub text: String,
    pub kind: LineType,
    pub tokens: usize,
    pub page: i32,
    pub heading_level: i32,
}

pub fn annotate_lines(pages: &[(String, i32)], tokenizer: &dyn TokenCounter) -> Vec<AnnotatedLine> {
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

