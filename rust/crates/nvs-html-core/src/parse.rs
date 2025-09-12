use html5ever::tendril::TendrilSink;
use html5ever::parse_document;
use markup5ever_rcdom::RcDom;
use std::default::Default;
use std::io::Cursor;

pub type Dom = RcDom;

pub fn parse_html_to_dom(html: &str) -> Dom {
    parse_document(RcDom::default(), Default::default())
        .from_utf8()
        .read_from(&mut Cursor::new(html.as_bytes()))
        .expect("parse html")
}

pub fn extract_sections(dom: &Dom) -> Vec<String> {
use markup5ever_rcdom::{Handle, NodeData};
    fn walk(acc: &mut Vec<String>, cur: &mut String, handle: &Handle, in_body: &mut bool) {
        match &handle.data {
            NodeData::Document => {
                for c in handle.children.borrow().iter() { walk(acc, cur, c, in_body); }
            }
            NodeData::Element { ref name, .. } => {
                let tag = &name.local;
                if tag.as_ref().eq_ignore_ascii_case("body") { *in_body = true; }
                let is_h1_h2 = tag.as_ref().eq_ignore_ascii_case("h1") || tag.as_ref().eq_ignore_ascii_case("h2");
                if is_h1_h2 && !cur.trim().is_empty() {
                    acc.push(std::mem::take(cur));
                }
                for c in handle.children.borrow().iter() { walk(acc, cur, c, in_body); }
                if tag.as_ref().eq_ignore_ascii_case("body") { *in_body = false; }
            }
            NodeData::Text { ref contents } => {
                if *in_body {
                    let s = contents.borrow();
                    let t = s.trim();
                    if !t.is_empty() {
                        cur.push_str(t);
                        cur.push('\n');
                    }
                }
            }
            _ => {}
        }
    }

    let mut acc: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut in_body = false;
    walk(&mut acc, &mut cur, &dom.document, &mut in_body);
    if !cur.trim().is_empty() {
        acc.push(cur);
    }
    // Filter empty sections
    acc.into_iter().filter(|s| !s.trim().is_empty()).collect()
}
