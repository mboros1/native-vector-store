use super::TokenCounter;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineType {
    Normal,
    MajorHeading,
    MinorHeading,
    ListItem,
    Blank,
    CodeBlock,
}

#[derive(Clone, Debug)]
pub struct AnnotatedLine {
    pub text: String,
    pub line_type: LineType,
    pub tokens: usize,
    pub page: i32,
    pub heading_level: i32,
}

fn detect_line_type(line: &str) -> (LineType, i32) {
    let s = line.trim();
    if s.is_empty() { return (LineType::Blank, 0); }
    // markdown-style headings
    if let Some(stripped) = s.strip_prefix('#') {
        let mut level = 1;
        let mut rest = stripped;
        while let Some(r) = rest.strip_prefix('#') { level += 1; rest = r; }
        if level <= 2 { return (LineType::MajorHeading, level as i32); }
        return (LineType::MinorHeading, level as i32);
    }
    // list items
    if s.starts_with('-') || s.starts_with('*') || s.starts_with('+') {
        return (LineType::ListItem, 0);
    }
    if s.chars().all(|c| c == '`') { return (LineType::CodeBlock, 0); }
    (LineType::Normal, 0)
}

pub fn annotate_lines(pages: &[(String, i32)], tokenizer: &dyn TokenCounter) -> Vec<AnnotatedLine> {
    let mut out = Vec::new();
    for (text, page) in pages {
        for line in text.split('\n') {
            let (lt, lvl) = detect_line_type(line);
            let tokens = tokenizer.count_tokens(line);
            out.push(AnnotatedLine {
                text: line.to_string(),
                line_type: lt,
                tokens,
                page: *page,
                heading_level: lvl,
            });
        }
    }
    out
}
