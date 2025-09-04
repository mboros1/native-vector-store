use super::{AnnotatedLine, LineType};

#[derive(Clone, Debug)]
pub struct SemanticUnit {
    pub lines: Vec<AnnotatedLine>,
    pub total_tokens: usize,
    pub pages: (i32, i32),
    pub has_major_heading: bool,
    pub min_heading_level: i32,
}

pub fn group_semantic_units(lines: &[AnnotatedLine]) -> Vec<SemanticUnit> {
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

