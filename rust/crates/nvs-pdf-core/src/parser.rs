// Parser skeleton (to be implemented): xref, objects, streams
use anyhow::{anyhow, Result};
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::ops::Range;
use crate::filters;

#[derive(Debug, Clone)]
pub enum PdfValue {
    Null,
    Bool(bool),
    Int(i64),
    Real(f64),
    Name(String),
    String(Vec<u8>),
    HexString(Vec<u8>),
    Array(Vec<PdfValue>),
    Dict(BTreeMap<String, PdfValue>),
    Ref(u32, u16),
    Stream { dict: BTreeMap<String, PdfValue>, data: Vec<u8> },
}

#[derive(Debug, Clone)]
pub struct PdfDoc {
    data: Vec<u8>,
    // Map of (obj,gen) -> byte range [start..end] containing "obj ... endobj"
    objects: HashMap<(u32, u16), Range<usize>>,
    // Objects extracted from object streams (synthetic buffers per object)
    inline_objects: HashMap<(u32, u16), Vec<u8>>,
}

impl PdfDoc {
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        let mut objects = HashMap::new();
        let hay = data;
        let mut i = 0usize;
        while i + 6 < hay.len() {
            // Look for pattern: <num> <num> obj
            if is_digit(hay[i]) {
                let (obj, j1) = parse_uint(hay, i);
                if obj.is_none() { i += 1; continue; }
                let obj = obj.unwrap();
                let j1 = skip_ws(hay, j1);
                let (gen, j2) = parse_uint(hay, j1);
                if gen.is_none() { i += 1; continue; }
                let gen = gen.unwrap() as u16;
                let j2 = skip_ws(hay, j2);
                if hay.get(j2..j2+3) == Some(b"obj") {
                    // find endobj
                    if let Some(end) = find_token(hay, j2+3, b"endobj") {
                        objects.insert((obj as u32, gen), i..end+6);
                        i = end + 6;
                        continue;
                    }
                }
            }
            i += 1;
        }
        let mut doc = PdfDoc { data: hay.to_vec(), objects, inline_objects: HashMap::new() };
        // Expand object streams (ObjStm) to populate inline_objects
        doc.expand_object_streams()?;
        Ok(doc)
    }

    pub fn get_object_range(&self, obj: u32, gen: u16) -> Option<Range<usize>> {
        self.objects.get(&(obj, gen)).cloned()
    }

    pub fn get_object(&self, obj: u32, gen: u16) -> Result<PdfValue> {
        if let Some(buf) = self.inline_objects.get(&(obj, gen)) {
            return parse_indirect_object(buf);
        }
        let range = self.get_object_range(obj, gen).ok_or_else(|| anyhow!("object not found {} {}", obj, gen))?;
        parse_indirect_object(&self.data[range])
    }

    pub fn iter_objects(&self) -> impl Iterator<Item=((u32,u16), Range<usize>)> + '_ {
        self.objects.iter().map(|(k,v)| (*k, v.clone()))
    }
}

fn parse_indirect_object(bytes: &[u8]) -> Result<PdfValue> {
    // bytes start at "<obj> <gen> obj" ... "endobj"
    // find after 'obj'
    let mut i = 0;
    // skip leading numbers and 'obj'
    // crude: find first occurrence of "obj" then parse value from there+3
    if let Some(pos) = find_token(bytes, 0, b"obj") {
        i = pos + 3;
    }
    i = skip_ws(bytes, i);
    // detect stream
    // parse a value (likely dict)
    let (val, j) = parse_value(bytes, i)?;
    let j = skip_ws(bytes, j);
    if bytes.get(j..j+6) == Some(b"stream") {
        // find endstream
        // Stream data begins after newline following 'stream'
        let mut k = j + 6;
        // consume optional \r\n or \n
        if bytes.get(k) == Some(&b'\r') && bytes.get(k+1) == Some(&b'\n') { k += 2; }
        else if bytes.get(k) == Some(&b'\n') { k += 1; }
        // find endstream
        if let Some(end) = find_token(bytes, k, b"endstream") {
            let data = bytes[k..end].to_vec();
            if let PdfValue::Dict(dict) = val {
                return Ok(PdfValue::Stream { dict, data });
            } else {
                return Err(anyhow!("stream without dict"));
            }
        } else {
            return Err(anyhow!("unterminated stream"));
        }
    }
    Ok(val)
}

fn parse_value(bytes: &[u8], mut i: usize) -> Result<(PdfValue, usize)> {
    i = skip_ws(bytes, i);
    if i >= bytes.len() { return Err(anyhow!("eof")); }
    match bytes[i] {
        b'<' => {
            if i+1 < bytes.len() && bytes[i+1] == b'<' {
                // dict
                let (dict, j) = parse_dict(bytes, i+2)?;
                Ok((PdfValue::Dict(dict), j))
            } else {
                // hex string <...>
                let (v, j) = parse_hex_string(bytes, i+1)?;
                Ok((PdfValue::HexString(v), j))
            }
        }
        b'(' => parse_string(bytes, i+1).map(|(v,j)| (PdfValue::String(v), j)),
        b'/' => parse_name(bytes, i+1).map(|(n,j)| (PdfValue::Name(n), j)),
        b'[' => parse_array(bytes, i+1).map(|(arr,j)| (PdfValue::Array(arr), j)),
        b'+'|b'-'|b'.'|b'0'..=b'9' => parse_number_or_ref(bytes, i),
        b'n' if starts_with(bytes, i, b"null") => Ok((PdfValue::Null, i+4)),
        b't' if starts_with(bytes, i, b"true") => Ok((PdfValue::Bool(true), i+4)),
        b'f' if starts_with(bytes, i, b"false") => Ok((PdfValue::Bool(false), i+5)),
        _ => Err(anyhow!("unexpected token at {}", i)),
    }
}

fn parse_dict(bytes: &[u8], mut i: usize) -> Result<(BTreeMap<String, PdfValue>, usize)> {
    let mut map = BTreeMap::new();
    loop {
        i = skip_ws(bytes, i);
        if i+1 < bytes.len() && bytes[i] == b'>' && bytes[i+1] == b'>' {
            return Ok((map, i+2));
        }
        if bytes.get(i) != Some(&b'/') { return Err(anyhow!("dict key expected")); }
        let (key, j1) = parse_name(bytes, i+1)?;
        let (val, j2) = parse_value(bytes, j1)?;
        map.insert(key, val);
        i = j2;
    }
}

fn parse_array(bytes: &[u8], mut i: usize) -> Result<(Vec<PdfValue>, usize)> {
    let mut arr = Vec::new();
    loop {
        i = skip_ws(bytes, i);
        if i >= bytes.len() { return Err(anyhow!("unterminated array")); }
        if bytes[i] == b']' { return Ok((arr, i+1)); }
        let (val, j) = parse_value(bytes, i)?;
        arr.push(val);
        i = j;
    }
}

fn parse_name(bytes: &[u8], mut i: usize) -> Result<(String, usize)> {
    let start = i;
    while i < bytes.len() {
        let b = bytes[i];
        match b {
            b' ' | b'\t' | b'\r' | b'\n' | b'/' | b'>' | b'<' | b'[' | b']' | b'(' | b')' => break,
            _ => i+=1,
        }
    }
    let slice = &bytes[start..i];
    let s = String::from_utf8_lossy(slice).to_string();
    Ok((s, i))
}

fn parse_string(bytes: &[u8], mut i: usize) -> Result<(Vec<u8>, usize)> {
    // Very simple: does not handle nested parentheses or escapes fully.
    let mut out = Vec::new();
    let mut depth = 1;
    while i < bytes.len() {
        let b = bytes[i]; i+=1;
        match b {
            b'(' => { depth+=1; out.push(b); }
            b')' => { depth-=1; if depth==0 { break; } else { out.push(b); } }
            b'\\' => { if i < bytes.len() { out.push(bytes[i]); i+=1; } }
            _ => out.push(b),
        }
    }
    Ok((out, i))
}

fn parse_hex_string(bytes: &[u8], mut i: usize) -> Result<(Vec<u8>, usize)> {
    let start = i;
    while i < bytes.len() && bytes[i] != b'>' { i+=1; }
    let s = &bytes[start..i];
    // decode hex pairs ignoring whitespace
    let mut nibbles = Vec::new();
    for &b in s {
        match b {
            b'0'..=b'9' => nibbles.push(b - b'0'),
            b'a'..=b'f' => nibbles.push(10 + (b - b'a')),
            b'A'..=b'F' => nibbles.push(10 + (b - b'A')),
            _ => {},
        }
    }
    let mut out = Vec::with_capacity(nibbles.len()/2);
    let mut k = 0;
    while k + 1 < nibbles.len() {
        out.push((nibbles[k]<<4) | nibbles[k+1]);
        k+=2;
    }
    if k < nibbles.len() { out.push(nibbles[k] << 4); }
    Ok((out, if i < bytes.len() { i+1 } else { i }))
}

fn parse_number_or_ref(bytes: &[u8], i: usize) -> Result<(PdfValue, usize)> {
    // Try int/real or "n n R"
    let (n1, j1, is_real1) = parse_number(bytes, i)?;
    let j1s = skip_ws(bytes, j1);
    if let Ok((n2, j2, _)) = parse_number(bytes, j1s) {
        let j2s = skip_ws(bytes, j2);
        if starts_with(bytes, j2s, b"R") {
            return Ok((PdfValue::Ref(n1 as u32, n2 as u16), j2s+1));
        }
    }
    if is_real1 { Ok((PdfValue::Real(n1 as f64), j1)) } else { Ok((PdfValue::Int(n1), j1)) }
}

fn parse_number(bytes: &[u8], mut i: usize) -> Result<(i64, usize, bool)> {
    let start = i;
    if i < bytes.len() && (bytes[i]==b'+' || bytes[i]==b'-') { i+=1; }
    let mut is_real = false;
    while i < bytes.len() {
        match bytes[i] { b'0'..=b'9' => i+=1, b'.' => { is_real=true; i+=1; }, _ => break }
    }
    let s = std::str::from_utf8(&bytes[start..i]).map_err(|_| anyhow!("num utf8"))?;
    if is_real {
        let f: f64 = s.parse().map_err(|_| anyhow!("real parse"))?;
        Ok((f as i64, i, true))
    } else {
        let n: i64 = s.parse().map_err(|_| anyhow!("int parse"))?;
        Ok((n, i, false))
    }
}

fn skip_ws(bytes: &[u8], mut i: usize) -> usize { while i < bytes.len() && is_ws(bytes[i]) { i+=1; } i }
fn is_ws(b: u8) -> bool { matches!(b, b' ' | b'\t' | b'\r' | b'\n' | 0x0c | 0x00) }
fn is_digit(b: u8) -> bool { (b'0'..=b'9').contains(&b) }
fn starts_with(bytes: &[u8], i: usize, token: &[u8]) -> bool { bytes.get(i..i+token.len()) == Some(token) }
fn find_token(bytes: &[u8], mut i: usize, token: &[u8]) -> Option<usize> {
    while i + token.len() <= bytes.len() {
        if &bytes[i..i+token.len()] == token { return Some(i); }
        i+=1;
    }
    None
}

// Future: high-level helpers to resolve catalog->pages->page->contents and decode streams using filters.
// Minimal page collector and content extraction for fast-path

fn as_name(v: &PdfValue) -> Option<&str> { if let PdfValue::Name(ref s) = v { Some(s.as_str()) } else { None } }
fn as_dict(v: &PdfValue) -> Option<&BTreeMap<String, PdfValue>> { if let PdfValue::Dict(ref d) = v { Some(d) } else { None } }
fn as_array(v: &PdfValue) -> Option<&Vec<PdfValue>> { if let PdfValue::Array(ref a) = v { Some(a) } else { None } }

fn resolve<'a>(doc: &'a PdfDoc, v: &'a PdfValue, depth: usize) -> Result<PdfValue> {
    if depth > 8 { return Err(anyhow!("ref recursion")); }
    if let PdfValue::Ref(obj, gen) = v { return doc.get_object(*obj, *gen); }
    Ok(v.clone())
}

pub fn collect_page_object_ids(doc: &PdfDoc) -> Vec<(u32,u16)> {
    let mut pages = Vec::new();
    for (&(obj,gen), range) in doc.objects.iter() {
        if let Ok(val) = parse_indirect_object(&doc.data[range.clone()]) {
            if let Some(d) = as_dict(&val) {
                if let Some(typ) = d.get("Type").and_then(|v| as_name(v)) {
                    if typ == "Page" { pages.push((obj,gen)); }
                }
            }
        }
    }
    pages
}

fn get_stream_data_with_filters(dict: &BTreeMap<String,PdfValue>, data: Vec<u8>) -> Result<Vec<u8>> {
    // Apply /Filter chain if present
    let mut out = data;
    if let Some(filter) = dict.get("Filter") {
        match filter {
            PdfValue::Name(n) => { out = apply_filter(n, out)?; }
            PdfValue::Array(arr) => {
                let mut cur = out;
                for f in arr {
                    if let Some(n) = as_name(f) { cur = apply_filter(n, cur)?; } else { return Err(anyhow!("filter name expected")); }
                }
                out = cur;
            }
            _ => {}
        }
    }
    Ok(out)
}

fn apply_filter(name: &str, data: Vec<u8>) -> Result<Vec<u8>> {
    match name {
        "FlateDecode" => filters::decode_flate(&data),
        "ASCII85Decode" => filters::decode_ascii85(&data),
        "ASCIIHexDecode" => filters::decode_asciihex(&data),
        "RunLengthDecode" => filters::decode_runlength(&data),
        other => Err(anyhow!("unsupported filter {}", other)),
    }
}

fn append_bytes_as_text(out: &mut String, s: &[u8]) {
    match String::from_utf8(s.to_vec()) {
        Ok(t) => out.push_str(&t),
        Err(_) => { for &b in s { out.push(b as char); } }
    }
}

pub fn extract_page_text(doc: &PdfDoc, page: (u32,u16)) -> Result<String> {
    // Find page dict
    let val = doc.get_object(page.0, page.1)?;
    let dict = if let Some(d) = as_dict(&val) { d } else { return Err(anyhow!("page not dict")); };
    // Build XObject map from Resources
    let mut xobjects: BTreeMap<String, PdfValue> = BTreeMap::new();
    if let Some(res) = dict.get("Resources").and_then(|v| as_dict(v)) {
        if let Some(xobj) = res.get("XObject").and_then(|v| as_dict(v)) {
            for (k, v) in xobj { xobjects.insert(k.clone(), v.clone()); }
        }
    }
    // Collect and decode page content streams
    let contents = dict.get("Contents").ok_or_else(|| anyhow!("no Contents"))?;
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
    let text = interpret_text_with_resources(doc, &xobjects, &buffers)?;
    Ok(text)
}

// Minimal tokenizer + interpreter
#[derive(Debug)]
enum Tok {
    Op(String),
    Name(String),
    Str(Vec<u8>),
    Num(f64),
    ArrStart,
    ArrEnd,
}

fn interpret_text_with_resources(doc: &PdfDoc, xobjects: &BTreeMap<String, PdfValue>, content: &[u8]) -> Result<String> {
    let mut out = String::new();
    let mut toks = Tokenizer::new(content);
    let mut pending_name: Option<String> = None;
    let mut last_nums: Vec<f64> = Vec::new();
    while let Some(tok) = toks.next()? {
        match tok {
            Tok::Op(op) => {
                match op.as_str() {
                    "T*" | "ET" => { out.push('\n'); last_nums.clear(); }
                    "Td" | "TD" => {
                        // Heuristic: if dy significant, newline; if dx large positive, space
                        if last_nums.len() >= 2 {
                            let dx = last_nums[last_nums.len()-2];
                            let dy = last_nums[last_nums.len()-1];
                            if dy.abs() > 1.0 { out.push('\n'); }
                            else if dx > 50.0 { out.push(' '); }
                        }
                        last_nums.clear();
                    }
                    "Tj" => { /* handled when we see preceding Str */ last_nums.clear(); }
                    "TJ" => { /* handled in array case */ last_nums.clear(); }
                    "Do" => {
                        if let Some(nm) = pending_name.take() {
                            if let Some(xv) = xobjects.get(&nm) {
                                let rv = resolve(doc, xv, 0).unwrap_or_else(|_| xv.clone());
                                if let PdfValue::Stream{ dict, data } = rv {
                                    if dict.get("Subtype").and_then(|v| as_name(v)) == Some("Form") {
                                        let dec = get_stream_data_with_filters(&dict, data.clone())?;
                                        // Build nested resources
                                        let mut sub_xobjs = xobjects.clone();
                                        if let Some(res) = dict.get("Resources").and_then(|v| as_dict(v)) {
                                            if let Some(xd) = res.get("XObject").and_then(|v| as_dict(v)) {
                                                for (k,v) in xd { let rv = resolve(doc, v, 0).unwrap_or_else(|_| v.clone()); sub_xobjs.insert(k.clone(), rv); }
                                            }
                                        }
                                        let sub = interpret_text_with_resources(doc, &sub_xobjs, &dec)?;
                                        if !sub.is_empty() { out.push_str(&sub); }
                                    }
                                }
                            }
                        }
                        last_nums.clear();
                    }
                    _ => { last_nums.clear(); }
                }
            }
            Tok::Name(n) => { pending_name = Some(n); }
            Tok::Str(s) => {
                // Peek next op to see if Tj
                if let Some(next) = toks.peek_op()? {
                    if next == "Tj" { append_bytes_as_text(&mut out, &s); /* no trailing space here */ toks.consume_op(); }
                }
            }
            Tok::Num(n) => { last_nums.push(n); }
            Tok::ArrStart => {
                // Stream through array items to build a single run, with spaces on large negative kerning
                let mut arr_text = String::new();
                while let Some(t) = toks.next()? {
                    match t {
                        Tok::ArrEnd => break,
                        Tok::Str(s) => { append_bytes_as_text(&mut arr_text, &s); },
                        Tok::Num(n) => { if n <= -120.0 { if !arr_text.ends_with(' ') { arr_text.push(' '); } } },
                        _ => {}
                    }
                }
                if let Some(next) = toks.peek_op()? {
                    if next == "TJ" { out.push_str(&arr_text); toks.consume_op(); }
                }
            }
            Tok::ArrEnd => {}
        }
    }
    Ok(out)
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
            b'+'|b'-'|b'.'|b'0'..=b'9' => { let (n,j,_) = parse_number(self.b, self.i)?; self.i=j; Ok(Some(Tok::Num(n as f64))) }
            _ => {
                // operator (letters)
                let start = self.i; let mut j = start;
                while j < self.b.len() && is_alpha(self.b[j]) { j+=1; }
                if j>start { let op = String::from_utf8_lossy(&self.b[start..j]).to_string(); self.i=j; Ok(Some(Tok::Op(op))) }
                else { self.i+=1; Ok(self.next()?) }
            }
        }
    }
    fn peek_op(&mut self) -> Result<Option<String>> {
        let save = self.i; let tok = self.next()?; self.i = save;
        if let Some(Tok::Op(op)) = tok { self.peeked_op = Some(op.clone()); Ok(Some(op)) } else { Ok(None) }
    }
    fn consume_op(&mut self) { if let Some(op) = self.peeked_op.take() { let _ = op; self.skip_ws(); let _ = self.next(); } }
}

fn is_alpha(b: u8) -> bool { (b'a'..=b'z').contains(&b) || (b'A'..=b'Z').contains(&b) || b'*'==b }

// Helpers
fn parse_uint(bytes: &[u8], mut i: usize) -> (Option<i64>, usize) {
    let start = i;
    while i < bytes.len() && is_digit(bytes[i]) { i+=1; }
    if i == start { return (None, i); }
    let s = std::str::from_utf8(&bytes[start..i]).ok();
    if let Some(ss) = s { if let Ok(n) = ss.parse::<i64>() { return (Some(n), i); } }
    (None, i)
}

impl PdfDoc {
    fn expand_object_streams(&mut self) -> Result<()> {
        // Iterate copy of keys to avoid borrow issues
        let keys: Vec<_> = self.objects.keys().cloned().collect();
        for (obj, gen) in keys {
            if let Some(range) = self.objects.get(&(obj, gen)).cloned() {
                if let Ok(PdfValue::Stream{ dict, data }) = parse_indirect_object(&self.data[range]) {
                    if dict.get("Type").and_then(|v| as_name(v)) == Some("ObjStm") {
                        // Decode stream
                        let decoded = get_stream_data_with_filters(&dict, data)?;
                        // Parse N and First
                        let n = dict.get("N").and_then(|v| match v { PdfValue::Int(i) => Some(*i as usize), _=>None }).ok_or_else(|| anyhow!("ObjStm missing N"))?;
                        let first = dict.get("First").and_then(|v| match v { PdfValue::Int(i) => Some(*i as usize), _=>None }).ok_or_else(|| anyhow!("ObjStm missing First"))?;
                        if first > decoded.len() { continue; }
                        // Header is ASCII: pairs of (objnum offset)
                        let header = &decoded[..first];
                        let mut nums: Vec<usize> = Vec::with_capacity(n*2);
                        let mut i = 0usize; while i < header.len() {
                            while i < header.len() && !header[i].is_ascii_digit() && header[i] != b'-' { i+=1; }
                            if i >= header.len() { break; }
                            let start = i; i+=1; while i < header.len() && header[i].is_ascii_digit() { i+=1; }
                            if let Ok(s) = std::str::from_utf8(&header[start..i]) { if let Ok(v) = s.parse::<isize>() { nums.push(v as usize); } }
                        }
                        if nums.len() < n*2 { continue; }
                        let body = &decoded[first..];
                        for k in 0..n {
                            let objnum = nums[2*k] as u32;
                            let off = nums[2*k+1];
                            let off_next = if k+1 < n { nums[2*(k+1)+1] } else { body.len() };
                            if off >= body.len() || off_next > body.len() || off >= off_next { continue; }
                            let slice = &body[off..off_next];
                            // Fabricate an indirect object buffer
                            let mut buf = Vec::with_capacity(slice.len() + 32);
                            buf.extend_from_slice(format!("{} 0 obj\n", objnum).as_bytes());
                            buf.extend_from_slice(slice);
                            buf.extend_from_slice(b"\nendobj\n");
                            self.inline_objects.insert((objnum, 0u16), buf);
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

// Page tree traversal (Catalog -> Pages -> Kids)
pub fn collect_pages_via_tree(doc: &PdfDoc) -> Result<Vec<(u32,u16)>> {
    // Find Catalog
    let mut catalog_ref: Option<(u32,u16)> = None;
    for (&(obj,gen), range) in doc.objects.iter() {
        if let Ok(val) = parse_indirect_object(&doc.data[range.clone()]) {
            if let Some(d) = as_dict(&val) {
                if d.get("Type").and_then(|v| as_name(v)) == Some("Catalog") {
                    catalog_ref = Some((obj,gen)); break;
                }
            }
        }
    }
    let (cat_obj, cat_gen) = catalog_ref.ok_or_else(|| anyhow!("Catalog not found"))?;
    let catalog = doc.get_object(cat_obj, cat_gen)?;
    let cat_dict = as_dict(&catalog).ok_or_else(|| anyhow!("Catalog not dict"))?;
    let pages_val = cat_dict.get("Pages").ok_or_else(|| anyhow!("Catalog missing Pages"))?;
    let mut out = Vec::new();
    traverse_pages_node(doc, pages_val, &mut out, 0)?;
    Ok(out)
}

fn traverse_pages_node(doc: &PdfDoc, node: &PdfValue, out: &mut Vec<(u32,u16)>, depth: usize) -> Result<()> {
    if depth > 64 { return Err(anyhow!("pages tree too deep")); }
    // Expect Ref to Pages/Page dicts
    let val = match node { PdfValue::Ref(o,g) => doc.get_object(*o,*g)?, _ => node.clone() };
    let d = as_dict(&val).ok_or_else(|| anyhow!("pages node not dict"))?;
    match d.get("Type").and_then(|v| as_name(v)) {
        Some("Pages") => {
            if let Some(kids) = d.get("Kids").and_then(|v| as_array(v)) {
                for kid in kids {
                    if let PdfValue::Ref(ko,kg) = kid { traverse_pages_node(doc, kid, out, depth+1)?; }
                }
            }
            Ok(())
        }
        Some("Page") => {
            // We need its Ref; retrieve from Parent Kids traversal: if we got here via a Ref, node was a Ref
            if let PdfValue::Ref(o,g) = node { out.push((*o,*g)); }
            Ok(())
        }
        _ => Ok(()),
    }
}
