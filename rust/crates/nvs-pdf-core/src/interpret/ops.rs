use anyhow::Result;
use std::collections::BTreeMap;
use crate::objects::{as_name, resolve, PdfDoc, PdfValue};
use crate::resources::FontInfo;
use super::lexer::{Tok, Tokenizer};
use super::state::TextState;
use super::emit::emit_mapped_text;
use super::xobject::resolve_form_and_merge_resources;

pub fn interpret_text_with_resources(
    doc: &PdfDoc,
    xobjects: &BTreeMap<String, PdfValue>,
    fonts: &BTreeMap<String, FontInfo>,
    content: &[u8],
) -> Result<String> {
    interpret_text_with_resources_depth(doc, xobjects, fonts, content, 0)
}

fn interpret_text_with_resources_depth(
    doc: &PdfDoc,
    xobjects: &BTreeMap<String, PdfValue>,
    fonts: &BTreeMap<String, FontInfo>,
    content: &[u8],
    depth: usize,
) -> Result<String> {
    let mut out = String::new();
    let mut toks = Tokenizer::new(content);
    let mut pending_name: Option<String> = None;
    let mut last_nums: Vec<f64> = Vec::new();
    let mut current_font: Option<String> = None;
    let mut ts = TextState::default();

    while let Some(tok) = toks.next()? {
        match tok {
            Tok::Op(op) => match op.as_str() {
                "BI" => {
                    // Skip inline image dict until ID
                    loop {
                        if let Some(next) = toks.next()? { if let Tok::Op(ref idop) = next { if idop == "ID" { break; } } } else { break; }
                    }
                    toks.skip_inline_image_after_id();
                    last_nums.clear();
                }
                "BT" => { ts.tm_e = 0.0; ts.tm_f = 0.0; ts.prev_y = None; last_nums.clear(); }
                "T*" => { out.push('\n'); last_nums.clear(); }
                "ET" => { if !out.ends_with('\n') { out.push('\n'); } last_nums.clear(); }
                "Tm" => {
                    if last_nums.len() >= 6 {
                        let f = last_nums[last_nums.len()-1];
                        let e = last_nums[last_nums.len()-2];
                        if let Some(prev) = ts.prev_y {
                            let dy = f - prev;
                            let thresh = if ts.leading.abs() > 0.0 { 0.8 * ts.leading.abs() } else { 0.5 * ts.font_size.max(1.0) };
                            if dy.abs() >= thresh { out.push('\n'); }
                        }
                        ts.tm_e = e; ts.tm_f = f; ts.prev_y = Some(f);
                    }
                    last_nums.clear();
                }
                "Td" | "TD" => {
                    if last_nums.len() >= 2 {
                        let dx = last_nums[last_nums.len()-2];
                        let dy = last_nums[last_nums.len()-1];
                        ts.tm_e += dx; ts.tm_f += dy;
                        let thresh_nl = if ts.leading.abs() > 0.0 { 0.8 * ts.leading.abs() } else { 0.5 * ts.font_size.max(1.0) };
                        let sp_thresh = 0.4 * ts.font_size * ts.h_scale;
                        if dy.abs() >= thresh_nl { out.push('\n'); ts.prev_y = Some(ts.tm_f); }
                        else if dx > sp_thresh { out.push(' '); }
                    }
                    last_nums.clear();
                }
                "TL" => { if let Some(tl) = last_nums.last().copied() { ts.leading = tl; } last_nums.clear(); }
                "Tw" => { if let Some(w) = last_nums.last().copied() { ts.word_spacing = w; } last_nums.clear(); }
                "Tc" => { if let Some(c) = last_nums.last().copied() { ts.char_spacing = c; } last_nums.clear(); }
                "Tz" => { if let Some(z) = last_nums.last().copied() { ts.h_scale = (z / 100.0).max(0.01); } last_nums.clear(); }
                "Tj" => { last_nums.clear(); }
                "TJ" => { last_nums.clear(); }
                "Do" => {
                    if let Some(nm) = pending_name.take() {
                        if depth > 8 { last_nums.clear(); continue; }
                        if let Some(xv) = xobjects.get(&nm) {
                            let rv = resolve(doc, xv, 0).unwrap_or_else(|_| xv.clone());
                            if let PdfValue::Stream { dict, data } = rv {
                                if as_name(dict.get("Subtype").unwrap_or(&PdfValue::Null)) == Some("Form") {
                                    if let Some((dec, sub_xobjs, sub_fonts)) = resolve_form_and_merge_resources(doc, &dict, data, xobjects, fonts) {
                                        let sub = interpret_text_with_resources_depth(doc, &sub_xobjs, &sub_fonts, &dec, depth + 1)?;
                                        if !sub.is_empty() { out.push_str(&sub); if !out.ends_with('\n') { out.push('\n'); } }
                                    }
                                }
                            }
                        }
                    }
                    last_nums.clear();
                }
                "Tf" => {
                    if let Some(nm) = pending_name.take() { current_font = Some(nm); }
                    if let Some(sz) = last_nums.last().copied() {
                        ts.font_size = sz.abs().max(0.1);
                        if ts.leading == 0.0 { ts.leading = 1.2 * ts.font_size; }
                    }
                    last_nums.clear();
                }
                _ => { last_nums.clear(); }
            },
            Tok::Name(n) => { pending_name = Some(n); }
            Tok::Str(s) => {
                if let Some(next) = toks.peek_op()? {
                    if next == "Tj" {
                        emit_mapped_text(&mut out, &s, &current_font, fonts);
                        toks.consume_op();
                    } else if next == "'" {
                        if !out.ends_with('\n') { out.push('\n'); }
                        emit_mapped_text(&mut out, &s, &current_font, fonts);
                        toks.consume_op();
                        last_nums.clear();
                    } else if next == "\"" {
                        if last_nums.len() >= 2 {
                            let aw = last_nums[last_nums.len()-2];
                            let ac = last_nums[last_nums.len()-1];
                            ts.word_spacing = aw; ts.char_spacing = ac;
                        }
                        if !out.ends_with('\n') { out.push('\n'); }
                        emit_mapped_text(&mut out, &s, &current_font, fonts);
                        toks.consume_op();
                        last_nums.clear();
                    }
                }
            }
            Tok::Num(n) => { last_nums.push(n); }
            Tok::ArrStart => {
                let mut arr_text = String::new();
                while let Some(t) = toks.next()? {
                    match t {
                        Tok::ArrEnd => break,
                        Tok::Str(s) => emit_mapped_text(&mut arr_text, &s, &current_font, fonts),
                        Tok::Num(n) => { if n <= -100.0 { if !arr_text.ends_with(' ') { arr_text.push(' '); } } }
                        _ => {}
                    }
                }
                if let Some(next) = toks.peek_op()? { if next == "TJ" { out.push_str(&arr_text); toks.consume_op(); } }
            }
            Tok::ArrEnd => {}
        }
    }
    Ok(out)
}

