use anyhow::{anyhow, Result};
use std::collections::BTreeMap;
use std::collections::BTreeMap as OrderedMap;
use std::ops::Range;

// Core PDF value model
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
    pub(crate) data: Vec<u8>,
    pub(crate) objects: OrderedMap<(u32, u16), Range<usize>>, // deterministic order
    pub(crate) inline_objects: OrderedMap<(u32, u16), Vec<u8>>, // expanded ObjStm
}

impl PdfDoc {
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        let mut objects: OrderedMap<(u32, u16), Range<usize>> = OrderedMap::new();
        let hay = data;
        let mut i = 0usize;
        while i + 6 < hay.len() {
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
                    if let Some(end) = find_token(hay, j2+3, b"endobj") {
                        objects.insert((obj as u32, gen), i..end+6);
                        i = end + 6;
                        continue;
                    }
                }
            }
            i += 1;
        }
        let mut doc = PdfDoc { data: hay.to_vec(), objects, inline_objects: OrderedMap::new() };
        // Populate additional object ranges from any XRef streams
        doc.populate_from_xref_streams();
        // Also try parsing from startxref pointers (handles files with no textual obj scan hits)
        doc.populate_from_startxref();
        doc.expand_object_streams()?;
        Ok(doc)
    }

    pub fn get_object_range(&self, obj: u32, gen: u16) -> Option<Range<usize>> { self.objects.get(&(obj, gen)).cloned() }

    pub fn get_object(&self, obj: u32, gen: u16) -> Result<PdfValue> {
        if let Some(buf) = self.inline_objects.get(&(obj, gen)) { return parse_indirect_object(buf); }
        let range = self.get_object_range(obj, gen).ok_or_else(|| anyhow!("object not found {} {}", obj, gen))?;
        parse_indirect_object(&self.data[range])
    }

    pub fn iter_objects(&self) -> impl Iterator<Item=((u32,u16), Range<usize>)> + '_ { self.objects.iter().map(|(k,v)| (*k, v.clone())) }

    pub(crate) fn expand_object_streams(&mut self) -> Result<()> {
        let keys: Vec<_> = self.objects.keys().cloned().collect();
        for (obj, gen) in keys {
            if let Some(range) = self.objects.get(&(obj, gen)).cloned() {
                if let Ok(PdfValue::Stream{ dict, data }) = self.parse_object_at_range(range.clone()) {
                    if dict.get("Type").and_then(|v| as_name(v)) == Some("ObjStm") {
                        // Try primary decode
                        let decoded = match crate::streams::get_stream_data_with_filters(&dict, data.clone()) {
                            Ok(v) => v,
                            Err(_e1) => {
                                // Try scan-based slice
                                if let Ok(PdfValue::Stream{ dict: d2, data: dbytes }) = self.parse_stream_at_range_scan(range.clone()) {
                                    match crate::streams::get_stream_data_with_filters(&d2, dbytes.clone()) {
                                        Ok(v2) => v2,
                                        Err(_e2) => {
                                            // Brute force around declared Length
                                            if let Some(PdfValue::Int(len)) = dict.get("Length") {
                                                if let Some(v3) = self.bruteforce_flate_slice(range.clone(), *len as usize, &dict) { v3 }
                                                else { return Err(anyhow!("ObjStm decode failed after brute-force")); }
                                            } else {
                                                return Err(anyhow!("ObjStm decode failed and no Length to brute-force"));
                                            }
                                        }
                                    }
                                } else {
                                    return Err(anyhow!("ObjStm decode failed and scan fallback unavailable"));
                                }
                            }
                        };
                        let n = dict.get("N").and_then(|v| match v { PdfValue::Int(i) => Some(*i as usize), _=>None }).ok_or_else(|| anyhow!("ObjStm missing N"))?;
                        let first = dict.get("First").and_then(|v| match v { PdfValue::Int(i) => Some(*i as usize), _=>None }).ok_or_else(|| anyhow!("ObjStm missing First"))?;
                        if first > decoded.len() { continue; }
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

    fn populate_from_xref_streams(&mut self) {
        // Iterate a snapshot of current objects; new ones will be inserted as discovered
        let keys: Vec<_> = self.objects.keys().cloned().collect();
        for (obj, gen) in keys {
            if let Some(range) = self.objects.get(&(obj, gen)).cloned() {
                if let Ok(PdfValue::Stream{ dict, data }) = self.parse_object_at_range(range.clone()) {
                    if dict.get("Type").and_then(|v| as_name(v)) == Some("XRef") {
                        if let Ok(decoded) = crate::streams::get_stream_data_with_filters(&dict, data) {
                            self.parse_xref_stream_entries(&dict, &decoded);
                        }
                    }
                }
            }
        }
    }

    fn parse_object_at_range(&self, bytes_range: Range<usize>) -> Result<PdfValue> {
        let bytes = &self.data[bytes_range.clone()];
        // Find 'obj'
        let mut i = 0usize;
        if let Some(pos) = find_token(bytes, 0, b"obj") { i = pos + 3; }
        i = skip_ws(bytes, i);
        let (val, j) = parse_value(bytes, i)?;
        let j = skip_ws(bytes, j);
        if bytes.get(j..j+6) == Some(b"stream") {
            let mut k = j + 6;
            if bytes.get(k) == Some(&b'\r') && bytes.get(k+1) == Some(&b'\n') { k += 2; }
            else if bytes.get(k) == Some(&b'\n') { k += 1; }
            if let PdfValue::Dict(ref d) = val {
                if let Some(len) = self.resolve_length(d) {
                    let end = k.saturating_add(len);
                    if end <= bytes.len() {
                        let data = bytes[k..end].to_vec();
                        return Ok(PdfValue::Stream { dict: d.clone(), data });
                    }
                }
            }
            // fallback to find 'endstream'
            if let Some(end) = find_token(bytes, k, b"endstream") {
                let data = bytes[k..end].to_vec();
                if let PdfValue::Dict(dict) = val { return Ok(PdfValue::Stream { dict, data }); } else { return Err(anyhow!("stream without dict")); }
            } else { return Err(anyhow!("unterminated stream")); }
        }
        Ok(val)
    }

    fn parse_stream_at_range_scan(&self, bytes_range: Range<usize>) -> Result<PdfValue> {
        let bytes = &self.data[bytes_range.clone()];
        let mut i = 0usize;
        if let Some(pos) = find_token(bytes, 0, b"obj") { i = pos + 3; }
        i = skip_ws(bytes, i);
        let (val, j) = parse_value(bytes, i)?;
        let j = skip_ws(bytes, j);
        if bytes.get(j..j+6) == Some(b"stream") {
            let mut k = j + 6;
            if bytes.get(k) == Some(&b'\r') && bytes.get(k+1) == Some(&b'\n') { k += 2; }
            else if bytes.get(k) == Some(&b'\n') { k += 1; }
            // Find the endobj and select the last endstream before it
            if let Some(endobj) = find_token(bytes, k, b"endobj") {
                let mut search = k;
                let mut last_es = None;
                while let Some(es) = find_token(bytes, search, b"endstream") {
                    if es >= endobj { break; }
                    last_es = Some(es); search = es + 9;
                }
                if let Some(end) = last_es {
                    let data = bytes[k..end].to_vec();
                    if let PdfValue::Dict(dict) = val { return Ok(PdfValue::Stream { dict, data }); }
                    else { return Err(anyhow!("scan stream without dict")); }
                }
            }
            // Fallback to first endstream if endobj not found
            if let Some(end) = find_token(bytes, k, b"endstream") {
                let data = bytes[k..end].to_vec();
                if let PdfValue::Dict(dict) = val { return Ok(PdfValue::Stream { dict, data }); }
                else { return Err(anyhow!("scan stream without dict")); }
            }
            return Err(anyhow!("scan stream: endstream/endobj not found"));
        }
        Ok(val)
    }

    fn bruteforce_flate_slice(&self, bytes_range: Range<usize>, declared_len: usize, dict: &BTreeMap<String, PdfValue>) -> Option<Vec<u8>> {
        let bytes = &self.data[bytes_range.clone()];
        // find stream start
        let mut i = 0usize; if let Some(pos) = find_token(bytes, 0, b"obj") { i = pos + 3; }
        i = skip_ws(bytes, i);
        let (_val, j) = parse_value(bytes, i).ok()?; let j = skip_ws(bytes, j);
        if bytes.get(j..j+6) != Some(b"stream") { return None; }
        let mut k = j + 6; if bytes.get(k) == Some(&b'\r') && bytes.get(k+1) == Some(&b'\n') { k += 2; } else if bytes.get(k) == Some(&b'\n') { k += 1; }
        // Try end positions around declared_len within +/- 128
        let start_end = k + declared_len;
        let mut deltas: Vec<isize> = (-128..=128).step_by(1).collect();
        // Try exact first
        deltas.sort_by_key(|d| d.abs());
        for d in deltas {
            let end = if d.is_negative() { start_end.saturating_sub(d.unsigned_abs() as usize) } else { start_end.saturating_add(d as usize) };
            if end <= bytes.len() && end > k + 8 {
                let slice = &bytes[k..end];
                // Try flate tolerant directly for performance
                if let Ok(v) = crate::filters::decode_flate_tolerant(slice) { return Some(v); }
            }
        }
        None
    }

    fn resolve_length(&self, dict: &BTreeMap<String, PdfValue>) -> Option<usize> {
        match dict.get("Length")? {
            PdfValue::Int(i) => Some(*i as usize),
            PdfValue::Real(f) => Some(*f as usize),
            PdfValue::Ref(o,g) => {
                if let Ok(v) = self.get_object(*o, *g) {
                    match v { PdfValue::Int(i)=>Some(i as usize), PdfValue::Real(f)=>Some(f as usize), _=>None }
                } else { None }
            }
            _ => None,
        }
    }

    fn parse_xref_stream_entries(&mut self, dict: &BTreeMap<String, PdfValue>, data: &[u8]) {
        // Parse W array (three integers)
        let w = match dict.get("W").and_then(|v| as_array(v)) {
            Some(arr) if arr.len() >= 3 => arr.iter().take(3).map(|v| match v { PdfValue::Int(i)=>*i as usize, PdfValue::Real(f)=>(*f as i64) as usize, _=>0 }).collect::<Vec<_>>(),
            _ => return,
        };
        let w0 = *w.get(0).unwrap_or(&0); let w1 = *w.get(1).unwrap_or(&0); let w2 = *w.get(2).unwrap_or(&0);
        // Index array: pairs of (start, count); default [0 Size]
        let mut indexes: Vec<(usize, usize)> = Vec::new();
        if let Some(idx) = dict.get("Index").and_then(|v| as_array(v)) {
            let mut it = idx.iter();
            while let (Some(a), Some(b)) = (it.next(), it.next()) {
                let start = match a { PdfValue::Int(i)=>*i as usize, PdfValue::Real(f)=>*f as usize, _=>0 };
                let count = match b { PdfValue::Int(i)=>*i as usize, PdfValue::Real(f)=>*f as usize, _=>0 };
                if count>0 { indexes.push((start, count)); }
            }
        } else if let Some(PdfValue::Int(sz)) = dict.get("Size") { indexes.push((0, *sz as usize)); }
        if indexes.is_empty() { return; }
        // Walk entries
        let mut off = 0usize;
        for (start, count) in indexes {
            for i in 0..count {
                if off + w0 + w1 + w2 > data.len() { return; }
                let t = if w0==0 { 1u64 } else { read_uint_be(&data[off..off+w0]) }; off += w0;
                let f1 = if w1==0 { 0u64 } else { read_uint_be(&data[off..off+w1]) }; off += w1;
                let f2 = if w2==0 { 0u64 } else { read_uint_be(&data[off..off+w2]) }; off += w2;
                let objnum = (start + i) as u32;
                match t {
                    0 => { /* free */ }
                    1 => { // uncompressed object at offset f1, gen f2
                        let offset = f1 as usize; let gen = f2 as u16;
                        self.add_object_range_if_missing(objnum, gen, offset);
                    }
                    2 => { // compressed object in object stream f1 at index f2
                        // Ensure the object stream itself is present; will be expanded later
                        let objstm = f1 as u32; self.add_object_range_if_any(objstm);
                    }
                    _ => {}
                }
            }
        }
    }

    fn add_object_range_if_any(&mut self, obj: u32) {
        // If object exists, do nothing; otherwise try to locate via textual scan (best effort)
        if self.objects.keys().any(|(o,_g)| *o == obj) { return; }
        // best effort: search for pattern "<obj> 0 obj" from start
        let needle = format!("{} 0 obj", obj);
        if let Some(pos) = find_token(&self.data, 0, needle.as_bytes()) {
            if let Some(end) = find_token(&self.data, pos, b"endobj") { self.objects.insert((obj, 0u16), pos..end+6); }
        }
    }

    fn add_object_range_if_missing(&mut self, obj: u32, gen: u16, offset: usize) {
        if self.objects.contains_key(&(obj, gen)) { return; }
        if offset >= self.data.len() { return; }
        // Verify this looks like an object header at offset
        // Scan forward slightly to find "obj"
        let mut i = offset;
        let end = self.data.len();
        // Skip whitespace
        while i < end && is_ws(self.data[i]) { i+=1; }
        // Find endobj
        if let Some(endpos) = find_token(&self.data, i, b"endobj") {
            self.objects.insert((obj, gen), i..endpos+6);
        }
    }
}

fn read_uint_be(slice: &[u8]) -> u64 { let mut v=0u64; for &b in slice { v = (v<<8) | (b as u64); } v }

impl PdfDoc {
    fn populate_from_startxref(&mut self) {
        // Find last 'startxref' in file
        let mut pos = None;
        let mut i = self.data.len().saturating_sub(9);
        while i > 0 {
            if self.data[i..].starts_with(b"startxref") { pos = Some(i); break; }
            i -= 1;
        }
        let Some(start) = pos else { return };
        // Parse number after startxref
        let mut j = start + b"startxref".len();
        while j < self.data.len() && is_ws(self.data[j]) { j+=1; }
        let num_start = j; while j < self.data.len() && self.data[j].is_ascii_digit() { j+=1; }
        if j == num_start { return; }
        let off_str = String::from_utf8_lossy(&self.data[num_start..j]).to_string();
        let Ok(off) = off_str.parse::<usize>() else { return };
        let mut visited = std::collections::BTreeSet::new();
        let mut queue = vec![off];
        while let Some(offset) = queue.pop() {
            if !visited.insert(offset) { continue; }
            if offset >= self.data.len() { continue; }
            if let Ok(val) = parse_indirect_object(&self.data[offset..]) {
                if let PdfValue::Stream{ dict, data } = val {
                    if dict.get("Type").and_then(|v| as_name(v)) == Some("XRef") {
                        if let Ok(decoded) = crate::streams::get_stream_data_with_filters(&dict, data) {
                            self.parse_xref_stream_entries(&dict, &decoded);
                            // Follow Prev/XRefStm if present
                            if let Some(PdfValue::Int(prev)) = dict.get("Prev") { queue.push(*prev as usize); }
                            if let Some(PdfValue::Int(xrs)) = dict.get("XRefStm") { queue.push(*xrs as usize); }
                        }
                    }
                }
            }
        }
    }
}

// Parsing primitives reused across modules
pub(crate) fn parse_indirect_object(bytes: &[u8]) -> Result<PdfValue> {
    let mut i = 0;
    if let Some(pos) = find_token(bytes, 0, b"obj") { i = pos + 3; }
    i = skip_ws(bytes, i);
    let (val, j) = parse_value(bytes, i)?;
    let j = skip_ws(bytes, j);
    if bytes.get(j..j+6) == Some(b"stream") {
        let mut k = j + 6;
        if bytes.get(k) == Some(&b'\r') && bytes.get(k+1) == Some(&b'\n') { k += 2; }
        else if bytes.get(k) == Some(&b'\n') { k += 1; }
        // Use declared Length if present
        if let PdfValue::Dict(ref d) = val {
            if let Some(PdfValue::Int(len)) = d.get("Length") {
                let end = k.saturating_add(*len as usize);
                if end <= bytes.len() {
                    let data = bytes[k..end].to_vec();
                    let dict = d.clone();
                    return Ok(PdfValue::Stream { dict, data });
                }
            }
        }
        if let Some(end) = find_token(bytes, k, b"endstream") {
            let data = bytes[k..end].to_vec();
            if let PdfValue::Dict(dict) = val { return Ok(PdfValue::Stream { dict, data }); } else { return Err(anyhow!("stream without dict")); }
        } else { return Err(anyhow!("unterminated stream")); }
    }
    Ok(val)
}

pub(crate) fn parse_value(bytes: &[u8], mut i: usize) -> Result<(PdfValue, usize)> {
    i = skip_ws(bytes, i);
    if i >= bytes.len() { return Err(anyhow!("eof")); }
    match bytes[i] {
        b'<' => {
            if i+1 < bytes.len() && bytes[i+1] == b'<' { let (dict, j) = parse_dict(bytes, i+2)?; Ok((PdfValue::Dict(dict), j)) }
            else { let (v, j) = parse_hex_string(bytes, i+1)?; Ok((PdfValue::HexString(v), j)) }
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

pub(crate) fn parse_dict(bytes: &[u8], mut i: usize) -> Result<(BTreeMap<String, PdfValue>, usize)> {
    let mut map = BTreeMap::new();
    loop {
        i = skip_ws(bytes, i);
        if i+1 < bytes.len() && bytes[i] == b'>' && bytes[i+1] == b'>' { return Ok((map, i+2)); }
        if bytes.get(i) != Some(&b'/') { return Err(anyhow!("dict key expected")); }
        let (key, j1) = parse_name(bytes, i+1)?;
        let (val, j2) = parse_value(bytes, j1)?;
        map.insert(key, val);
        i = j2;
    }
}

pub(crate) fn parse_array(bytes: &[u8], mut i: usize) -> Result<(Vec<PdfValue>, usize)> {
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

pub(crate) fn parse_name(bytes: &[u8], mut i: usize) -> Result<(String, usize)> {
    let start = i;
    while i < bytes.len() {
        let b = bytes[i];
        match b { b' ' | b'\t' | b'\r' | b'\n' | b'/' | b'>' | b'<' | b'[' | b']' | b'(' | b')' => break, _ => i+=1 }
    }
    let slice = &bytes[start..i];
    let s = String::from_utf8_lossy(slice).to_string();
    Ok((s, i))
}

pub(crate) fn parse_string(bytes: &[u8], mut i: usize) -> Result<(Vec<u8>, usize)> {
    // PDF string escape handling
    let mut out = Vec::new();
    let mut depth = 1i32;
    while i < bytes.len() {
        let b = bytes[i]; i += 1;
        match b {
            b'(' => { depth += 1; out.push(b); }
            b')' => { depth -= 1; if depth == 0 { break; } else { out.push(b); } }
            b'\\' => {
                if i >= bytes.len() { break; }
                let c = bytes[i];
                if c == b'\r' { i += 1; if i < bytes.len() && bytes[i] == b'\n' { i += 1; } continue; }
                else if c == b'\n' { i += 1; continue; }
                match c {
                    b'n' => { out.push(b'\n'); i += 1; }
                    b'r' => { out.push(b'\r'); i += 1; }
                    b't' => { out.push(b'\t'); i += 1; }
                    b'b' => { out.push(0x08); i += 1; }
                    b'f' => { out.push(0x0C); i += 1; }
                    b'(' | b')' | b'\\' => { out.push(c); i += 1; }
                    b'0'..=b'7' => {
                        let mut val: u32 = (c - b'0') as u32; i += 1; let mut cnt = 1;
                        while cnt < 3 && i < bytes.len() {
                            let d = bytes[i];
                            if (b'0'..=b'7').contains(&d) { val = (val << 3) + (d - b'0') as u32; i += 1; cnt += 1; } else { break; }
                        }
                        out.push((val & 0xFF) as u8);
                    }
                    _ => { out.push(c); i += 1; }
                }
            }
            _ => out.push(b),
        }
    }
    Ok((out, i))
}

pub(crate) fn parse_hex_string(bytes: &[u8], mut i: usize) -> Result<(Vec<u8>, usize)> {
    let start = i; while i < bytes.len() && bytes[i] != b'>' { i+=1; }
    let s = &bytes[start..i];
    let mut nibbles = Vec::new();
    for &b in s { match b { b'0'..=b'9' => nibbles.push(b - b'0'), b'a'..=b'f' => nibbles.push(10 + (b - b'a')), b'A'..=b'F' => nibbles.push(10 + (b - b'A')), _ => {} } }
    let mut out = Vec::with_capacity(nibbles.len()/2);
    let mut k = 0; while k + 1 < nibbles.len() { out.push((nibbles[k]<<4) | nibbles[k+1]); k+=2; }
    if k < nibbles.len() { out.push(nibbles[k] << 4); }
    Ok((out, if i < bytes.len() { i+1 } else { i }))
}

pub(crate) fn parse_number_or_ref(bytes: &[u8], i: usize) -> Result<(PdfValue, usize)> {
    let (n1, j1, is_real1) = parse_number(bytes, i)?;
    let j1s = skip_ws(bytes, j1);
    if let Ok((n2, j2, _)) = parse_number(bytes, j1s) { let j2s = skip_ws(bytes, j2); if starts_with(bytes, j2s, b"R") { return Ok((PdfValue::Ref(n1 as u32, n2 as u16), j2s+1)); } }
    if is_real1 { Ok((PdfValue::Real(n1 as f64), j1)) } else { Ok((PdfValue::Int(n1), j1)) }
}

pub(crate) fn parse_number(bytes: &[u8], mut i: usize) -> Result<(i64, usize, bool)> {
    let start = i; if i < bytes.len() && (bytes[i]==b'+' || bytes[i]==b'-') { i+=1; }
    let mut is_real = false; while i < bytes.len() { match bytes[i] { b'0'..=b'9' => i+=1, b'.' => { is_real=true; i+=1; }, _ => break } }
    let s = std::str::from_utf8(&bytes[start..i]).map_err(|_| anyhow!("num utf8"))?;
    if is_real { let f: f64 = s.parse().map_err(|_| anyhow!("real parse"))?; Ok((f as i64, i, true)) }
    else { let n: i64 = s.parse().map_err(|_| anyhow!("int parse"))?; Ok((n, i, false)) }
}

pub(crate) fn skip_ws(bytes: &[u8], mut i: usize) -> usize { while i < bytes.len() && is_ws(bytes[i]) { i+=1; } i }
pub(crate) fn is_ws(b: u8) -> bool { matches!(b, b' ' | b'\t' | b'\r' | b'\n' | 0x0c | 0x00) }
pub(crate) fn is_digit(b: u8) -> bool { (b'0'..=b'9').contains(&b) }
pub(crate) fn starts_with(bytes: &[u8], i: usize, token: &[u8]) -> bool { bytes.get(i..i+token.len()) == Some(token) }
pub(crate) fn find_token(bytes: &[u8], mut i: usize, token: &[u8]) -> Option<usize> { while i + token.len() <= bytes.len() { if &bytes[i..i+token.len()] == token { return Some(i); } i+=1; } None }
pub(crate) fn parse_uint(bytes: &[u8], mut i: usize) -> (Option<i64>, usize) { let start=i; while i < bytes.len() && is_digit(bytes[i]) { i+=1; } if i==start { return (None,i);} let s=std::str::from_utf8(&bytes[start..i]).ok(); if let Some(ss)=s { if let Ok(n)=ss.parse::<i64>() { return (Some(n), i);} } (None,i) }
pub(crate) fn is_alpha(b: u8) -> bool { (b'a'..=b'z').contains(&b) || (b'A'..=b'Z').contains(&b) || b'*'==b }

// Utility helpers
pub(crate) fn as_name(v: &PdfValue) -> Option<&str> { if let PdfValue::Name(ref s) = v { Some(s.as_str()) } else { None } }
pub(crate) fn as_dict(v: &PdfValue) -> Option<&BTreeMap<String, PdfValue>> { if let PdfValue::Dict(ref d) = v { Some(d) } else { None } }
pub(crate) fn as_array(v: &PdfValue) -> Option<&Vec<PdfValue>> { if let PdfValue::Array(ref a) = v { Some(a) } else { None } }
pub(crate) fn resolve<'a>(doc: &'a PdfDoc, v: &'a PdfValue, depth: usize) -> Result<PdfValue> { if depth > 8 { return Err(anyhow!("ref recursion")); } if let PdfValue::Ref(o,g) = v { return doc.get_object(*o,*g); } Ok(v.clone()) }
