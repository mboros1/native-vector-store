use super::{AnnotatedLine, LineType};
use std::collections::BTreeSet;

#[derive(Clone, Debug)]
pub struct SemanticUnit {
    pub lines: Vec<AnnotatedLine>,
    pub total_tokens: usize,
    pub pages: BTreeSet<i32>,
    pub has_major_heading: bool,
    pub min_heading_level: i32,
}

impl Default for SemanticUnit {
    fn default() -> Self {
        Self {
            lines: Vec::new(),
            total_tokens: 0,
            pages: BTreeSet::new(),
            has_major_heading: false,
            min_heading_level: i32::MAX,
        }
    }
}

impl SemanticUnit {
    pub fn add(&mut self, l: AnnotatedLine) {
        if matches!(l.line_type, LineType::MajorHeading) {
            self.has_major_heading = true;
            self.min_heading_level = self.min_heading_level.min(l.heading_level);
        }
        self.total_tokens += l.tokens;
        self.pages.insert(l.page);
        self.lines.push(l);
    }
    pub fn text(&self) -> String {
        let mut s = String::new();
        for l in &self.lines {
            s.push_str(&l.text);
            s.push('\n');
        }
        s
    }
}

pub fn group_semantic_units(lines: &[AnnotatedLine]) -> Vec<SemanticUnit> {
    let mut out: Vec<SemanticUnit> = Vec::new();
    let mut cur = SemanticUnit::default();
    for (i, l) in lines.iter().cloned().enumerate() {
        let mut break_here = false;
        match l.line_type {
            LineType::MajorHeading | LineType::MinorHeading => break_here = !cur.lines.is_empty(),
            LineType::Blank => {
                if let Some(nxt) = lines.get(i + 1) {
                    if matches!(
                        nxt.line_type,
                        LineType::MajorHeading | LineType::MinorHeading
                    ) {
                        break_here = !cur.lines.is_empty();
                    }
                }
            }
            _ => {}
        }
        if break_here {
            out.push(cur);
            cur = SemanticUnit::default();
        }
        if !(l.line_type == LineType::Blank && cur.lines.is_empty()) {
            cur.add(l);
        }
    }
    if !cur.lines.is_empty() {
        out.push(cur);
    }
    out
}
