use anyhow::Result;
use std::collections::BTreeMap;
use crate::fonts::map_bytes_with_tounicode_or_base;
use crate::objects::{as_dict, as_name, is_alpha, parse_name, parse_number, parse_string, resolve, skip_ws, PdfDoc, PdfValue};
use crate::resources::{collect_resources, FontInfo};
use crate::interpret::{append_bytes_as_text, interpret_contents};
use crate::metrics::MetricKey;
use crate::streams::get_stream_data_with_filters;
use std::time::Instant;
use crate::normalize::normalize_page_text;

// Resolve and decode the page's Contents into a single concatenated buffer
fn resolve_contents(doc: &PdfDoc, contents: &PdfValue) -> Result<Vec<u8>> {
    let mut buffers: Vec<u8> = Vec::new();
    match contents {
        PdfValue::Stream { dict, data } => {
            let dec = get_stream_data_with_filters(dict, data.clone())?;
            buffers.extend_from_slice(&dec);
        }
        PdfValue::Array(arr) => {
            for v in arr {
                let vv = resolve(doc, v, 0)?;
                if let PdfValue::Stream{ dict, data } = vv {
                    let dec = get_stream_data_with_filters(&dict, data)?;
                    buffers.extend_from_slice(&dec);
                }
            }
        }
        PdfValue::Ref(obj, gen) => {
            let vv = doc.get_object(*obj, *gen)?;
            if let PdfValue::Stream{ dict, data } = vv {
                let dec = get_stream_data_with_filters(&dict, data)?;
                buffers.extend_from_slice(&dec);
            }
        }
        _ => {}
    }
    Ok(buffers)
}

pub fn extract_page_text(doc: &PdfDoc, page: (u32,u16)) -> Result<String> {
    let t_page = Instant::now();
    let val = doc.get_object(page.0, page.1)?;
    let dict = if let Some(d) = as_dict(&val) { d } else { anyhow::bail!("page not dict"); };
    let res = crate::measure!(MetricKey::Resources, { collect_resources(doc, dict) })?;
    let contents = match dict.get("Contents") {
        Some(v) => v,
        None => return Ok(String::new()), // treat pages without content as empty text, not an error
    };
    let buffers = crate::measure!(MetricKey::Streams, { resolve_contents(doc, contents) })?;
    let text = crate::measure!(MetricKey::Interpret, { interpret_contents(doc, &res, &buffers) })?;
    let norm = crate::measure!(MetricKey::Normalize, { normalize_page_text(&text) });
    crate::stats::add_page_total_duration(t_page.elapsed().as_nanos() as u128);
    Ok(norm)
}

#[derive(Clone, Default, serde::Serialize)]
pub struct PageTextDebug {
    pub has_contents: bool,
    pub filters: Vec<String>,
    pub predictors: Vec<i64>,
    pub fonts_total: usize,
    pub fonts_with_tounicode: usize,
    pub decode_ok: bool,
    pub interpret_ok: bool,
    pub notes: Vec<String>,
}

// debug extraction removed; replaced by crate::debug module

fn get_predictor(dp: &PdfValue) -> Option<i64> {
    match dp {
        PdfValue::Dict(d) => d.get("Predictor").and_then(|v| match v { PdfValue::Int(i)=>Some(*i), PdfValue::Real(f)=>Some(*f as i64), _=>None }),
        PdfValue::Array(arr) => {
            for v in arr { if let Some(i)=get_predictor(v) { return Some(i); } }
            None
        }
        _=>None,
    }
}
// Tokenizer and interpreter for content streams
#[derive(Debug)] enum Tok { Op(String), Name(String), Str(Vec<u8>), Num(f64), ArrStart, ArrEnd }

fn interpret_text_with_resources_depth(doc: &PdfDoc, xobjects: &BTreeMap<String, PdfValue>, fonts: &BTreeMap<String, FontInfo>, content: &[u8], depth: usize) -> Result<String> {
    let mut out = String::new();
    let mut toks = Tokenizer::new(content);
    let mut pending_name: Option<String> = None;
    let mut last_nums: Vec<f64> = Vec::new();
    let mut current_font: Option<String> = None;
    #[derive(Clone, Copy)] struct TextState { font_size: f64, char_spacing: f64, word_spacing: f64, h_scale: f64, leading: f64, tm_e: f64, tm_f: f64, prev_y: Option<f64> }
    impl Default for TextState { fn default() -> Self { Self { font_size: 12.0, char_spacing: 0.0, word_spacing: 0.0, h_scale: 1.0, leading: 0.0, tm_e: 0.0, tm_f: 0.0, prev_y: None } } }
    let mut ts = TextState::default();
    while let Some(tok) = toks.next()? {
        match tok {
            Tok::Op(op) => match op.as_str() {
                // Inline image: BI ... ID <data> EI — skip dictionary + data
                "BI" => {
                    // Consume tokens until we see ID, then skip raw bytes until EI
                    loop {
                        if let Some(next) = toks.next()? {
                            if let Tok::Op(ref idop) = next { if idop == "ID" { break; } }
                            continue;
                        } else { break; }
                    }
                    toks.skip_inline_image_after_id();
                    last_nums.clear();
                }
                // Begin text object: reset matrices
                "BT" => { ts.tm_e = 0.0; ts.tm_f = 0.0; ts.prev_y = None; last_nums.clear(); }
                "T*" => { out.push('\n'); last_nums.clear(); }
                // Treat ET as a logical line break to separate blocks (e.g., headers vs body)
                "ET" => { if !out.ends_with('\n') { out.push('\n'); } last_nums.clear(); }
                // Set text matrix and line matrix
                "Tm" => {
                    if last_nums.len() >= 6 {
                        let f = last_nums[last_nums.len()-1];
                        let e = last_nums[last_nums.len()-2];
                        if let Some(prev) = ts.prev_y { let dy = f - prev; let thresh = if ts.leading.abs() > 0.0 { 0.8 * ts.leading.abs() } else { 0.5 * ts.font_size.max(1.0) }; if dy.abs() >= thresh { out.push('\n'); } }
                        ts.tm_e = e; ts.tm_f = f; ts.prev_y = Some(f);
                    }
                    last_nums.clear();
                }
                "Td" | "TD" => {
                    if last_nums.len() >= 2 {
                        let dx = last_nums[last_nums.len()-2];
                        let dy = last_nums[last_nums.len()-1];
                        // Update matrix translation
                        ts.tm_e += dx; ts.tm_f += dy;
                        let thresh_nl = if ts.leading.abs() > 0.0 { 0.8 * ts.leading.abs() } else { 0.5 * ts.font_size.max(1.0) };
                        let sp_thresh = 0.4 * ts.font_size * ts.h_scale;
                        if dy.abs() >= thresh_nl { out.push('\n'); ts.prev_y = Some(ts.tm_f); }
                        else if dx > sp_thresh { out.push(' '); }
                    }
                    last_nums.clear();
                }
                // Text leading
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
                            if let PdfValue::Stream{ dict, data } = rv {
                                if dict.get("Subtype").and_then(|v| as_name(v)) == Some("Form") {
                                    let dec = get_stream_data_with_filters(&dict, data.clone())?;
                                    let mut sub_xobjs = xobjects.clone();
                                    let mut sub_fonts = fonts.clone();
                                    if let Some(res) = dict.get("Resources").and_then(|v| as_dict(v)) {
                                        if let Some(xd) = res.get("XObject").and_then(|v| as_dict(v)) { for (k,v) in xd { let rv = resolve(doc, v, 0).unwrap_or_else(|_| v.clone()); sub_xobjs.insert(k.clone(), rv); } }
                                        if let Some(fdict) = res.get("Font").and_then(|v| as_dict(v)) {
                                            for (name, fv) in fdict {
                                                let rf = resolve(doc, fv, 0).unwrap_or_else(|_| fv.clone());
                                                if let Some(fd) = as_dict(&rf) {
                                                    let mut fi = FontInfo::default();
                                                    if let Some(enc_name) = fd.get("Encoding").and_then(|v| as_name(v)).map(|s| s.to_string()) { fi.base_encoding = Some(enc_name); }
                                                    if let Some(tu) = fd.get("ToUnicode") {
                                                        let rf2 = resolve(doc, tu, 0).unwrap_or_else(|_| tu.clone());
                        if let PdfValue::Stream{ dict: sdict, data } = rf2 {
                            if let Ok(dec) = crate::streams::get_stream_data_with_filters(&sdict, data) {
                                let tf = Instant::now();
                                fi.to_unicode = Some(crate::fonts::parse_tounicode_cmap(&dec));
                                crate::stats::add_fonts_duration(tf.elapsed().as_nanos() as u128);
                            }
                        }
                                                    }
                                                    sub_fonts.insert(name.clone(), fi);
                                                }
                                            }
                                        }
                                    }
                                    let ti = Instant::now();
                                    let sub = interpret_text_with_resources_depth(doc, &sub_xobjs, &sub_fonts, &dec, depth+1)?;
                                    crate::stats::add_interpret_duration(ti.elapsed().as_millis());
                                    if !sub.is_empty() { out.push_str(&sub); if !out.ends_with('\n') { out.push('\n'); } }
                                }
                            }
                        }
                    }
                    last_nums.clear();
                }
                "Tf" => { if let Some(nm) = pending_name.take() { current_font = Some(nm); } if let Some(sz) = last_nums.last().copied() { ts.font_size = sz.abs().max(0.1); if ts.leading == 0.0 { ts.leading = 1.2 * ts.font_size; } } last_nums.clear(); }
                _ => { last_nums.clear(); }
            },
            Tok::Name(n) => { pending_name = Some(n); }
            Tok::Str(s) => {
                if let Some(next) = toks.peek_op()? {
                    if next == "Tj" {
                        if let Some(ref fname) = current_font {
                            if let Some(fi) = fonts.get(fname) {
                                let mapped = map_bytes_with_tounicode_or_base(fi.to_unicode.as_ref(), fi.base_encoding.as_deref(), &s);
                                out.push_str(&mapped);
                            } else { append_bytes_as_text(&mut out, &s); }
                        } else { append_bytes_as_text(&mut out, &s); }
                        toks.consume_op();
                    } else if next == "'" { // apostrophe operator: newline + show
                        if !out.ends_with('\n') { out.push('\n'); }
                        if let Some(ref fname) = current_font { if let Some(fi) = fonts.get(fname) { let mapped = map_bytes_with_tounicode_or_base(fi.to_unicode.as_ref(), fi.base_encoding.as_deref(), &s); out.push_str(&mapped); } else { append_bytes_as_text(&mut out, &s); } }
                        else { append_bytes_as_text(&mut out, &s); }
                        toks.consume_op(); last_nums.clear();
                    } else if next == "\"" { // double-quote operator: set Tw/Tc from last_nums, newline, show
                        if last_nums.len() >= 2 {
                            let aw = last_nums[last_nums.len()-2];
                            let ac = last_nums[last_nums.len()-1];
                            ts.word_spacing = aw; ts.char_spacing = ac;
                        }
                        if !out.ends_with('\n') { out.push('\n'); }
                        if let Some(ref fname) = current_font { if let Some(fi) = fonts.get(fname) { let mapped = map_bytes_with_tounicode_or_base(fi.to_unicode.as_ref(), fi.base_encoding.as_deref(), &s); out.push_str(&mapped); } else { append_bytes_as_text(&mut out, &s); } }
                        else { append_bytes_as_text(&mut out, &s); }
                        toks.consume_op(); last_nums.clear();
                    }
                }
            }
            Tok::Num(n) => { last_nums.push(n); }
            Tok::ArrStart => {
                let mut arr_text = String::new();
                while let Some(t) = toks.next()? {
                    match t {
                        Tok::ArrEnd => break,
                        Tok::Str(s) => {
                            if let Some(ref fname) = current_font {
                                if let Some(fi) = fonts.get(fname) {
                                    let mapped = map_bytes_with_tounicode_or_base(fi.to_unicode.as_ref(), fi.base_encoding.as_deref(), &s);
                                    arr_text.push_str(&mapped);
                                } else { append_bytes_as_text(&mut arr_text, &s); }
                            } else { append_bytes_as_text(&mut arr_text, &s); }
                        },
                        Tok::Num(n) => { if n <= -100.0 { if !arr_text.ends_with(' ') { arr_text.push(' '); } } },
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

fn interpret_text_with_resources(doc: &PdfDoc, xobjects: &BTreeMap<String, PdfValue>, fonts: &BTreeMap<String, FontInfo>, content: &[u8]) -> Result<String> {
    interpret_text_with_resources_depth(doc, xobjects, fonts, content, 0)
}

struct Tokenizer<'a> { b: &'a [u8], i: usize, peeked_op: Option<String> }
impl<'a> Tokenizer<'a> {
    fn new(b: &'a [u8]) -> Self { Self { b, i: 0, peeked_op: None } }
    fn skip_ws(&mut self) { self.i = skip_ws(self.b, self.i); }
    fn next(&mut self) -> Result<Option<Tok>> {
        self.skip_ws(); if self.i >= self.b.len() { return Ok(None); }
        let c = self.b[self.i];
        match c {
            b'(' => { let (s,j) = parse_string(self.b, self.i+1)?; self.i = j; Ok(Some(Tok::Str(s))) }
            b'[' => { self.i+=1; Ok(Some(Tok::ArrStart)) }
            b']' => { self.i+=1; Ok(Some(Tok::ArrEnd)) }
            b'/' => { let (n,j) = parse_name(self.b, self.i+1)?; self.i=j; Ok(Some(Tok::Name(n))) }
            b'\'' => { self.i+=1; Ok(Some(Tok::Op("'".to_string()))) }
            b'"' => { self.i+=1; Ok(Some(Tok::Op("\"".to_string()))) }
            b'+'|b'-'|b'.'|b'0'..=b'9' => {
                match parse_number(self.b, self.i) {
                    Ok((n,j,_)) => { self.i=j; Ok(Some(Tok::Num(n as f64))) },
                    Err(_) => { // recover from malformed numeric (e.g., lone '-' before operator); skip one byte and continue
                        self.i += 1; self.next()
                    }
                }
            }
            _ => {
                let start = self.i; let mut j = start; while j < self.b.len() && is_alpha(self.b[j]) { j+=1; }
                if j>start { let op = String::from_utf8_lossy(&self.b[start..j]).to_string(); self.i=j; Ok(Some(Tok::Op(op))) }
                else { self.i+=1; Ok(self.next()?) }
            }
        }
    }
    fn peek_op(&mut self) -> Result<Option<String>> { let save=self.i; let tok=self.next()?; self.i=save; if let Some(Tok::Op(op))=tok { self.peeked_op=Some(op.clone()); Ok(Some(op)) } else { Ok(None) } }
    fn consume_op(&mut self) { if let Some(op)=self.peeked_op.take() { let _ = op; self.skip_ws(); let _ = self.next(); } }
    fn skip_inline_image_after_id(&mut self) {
        // Skip one whitespace after ID if present
        if self.i < self.b.len() && crate::objects::is_ws(self.b[self.i]) { self.i += 1; }
        // Scan until we find EI delimited by whitespace
        while self.i + 1 < self.b.len() {
            let prev = if self.i == 0 { b' ' } else { self.b[self.i - 1] };
            if self.b[self.i] == b'E' && self.b[self.i + 1] == b'I' {
                let next = if self.i + 2 < self.b.len() { self.b[self.i + 2] } else { b' ' };
                if crate::objects::is_ws(prev) && (crate::objects::is_ws(next) || !is_alpha(next)) {
                    self.i += 2; break;
                }
            }
            self.i += 1;
        }
    }
}

// normalize_page_text moved to crate::normalize
