use std::collections::HashMap;
use encoding_rs::{WINDOWS_1252, MACINTOSH};

#[derive(Debug, Default, Clone)]
pub struct ToUnicodeMap {
    // map of code (u16) -> unicode string
    single: HashMap<u16, String>,
}

impl ToUnicodeMap {
    pub fn map_bytes(&self, bytes: &[u8]) -> String {
        // MVP: try single-byte codes, then two-byte codes; fallback raw bytes
        if self.single.is_empty() {
            return bytes.iter().map(|&b| b as char).collect();
        }
        let mut out = String::new();
        let mut i = 0usize;
        while i < bytes.len() {
            // try 2-byte code if possible
            if i+1 < bytes.len() {
                let code = ((bytes[i] as u16) << 8) | (bytes[i+1] as u16);
                if let Some(s) = self.single.get(&code) { out.push_str(s); i+=2; continue; }
            }
            let code = bytes[i] as u16;
            if let Some(s) = self.single.get(&code) { out.push_str(s); }
            else { out.push(bytes[i] as char); }
            i+=1;
        }
        out
    }
}

pub fn parse_tounicode_cmap(data: &[u8]) -> ToUnicodeMap {
    let s = match std::str::from_utf8(data) { Ok(v) => v, Err(_) => return ToUnicodeMap::default() };
    let mut map = ToUnicodeMap::default();
    let mut i = 0usize;
    while i < s.len() {
        if s[i..].starts_with("beginbfchar") {
            // lines like: <src> <dst>
            // scan until endbfchar
            if let Some(end) = s[i..].find("endbfchar") {
                let block = &s[i..i+end];
                for line in block.lines() {
                    let (src_opt, dst_opt) = (extract_hex(line, 0), extract_hex(line, 1));
                    if let (Some(src), Some(dst)) = (src_opt, dst_opt) {
                        if let Some(u) = hex_to_string(&dst) { map.single.insert(hex_to_u16(&src), u); }
                    }
                }
                i += end + "endbfchar".len();
                continue;
            } else { break; }
        }
        if s[i..].starts_with("beginbfrange") {
            if let Some(end) = s[i..].find("endbfrange") {
                let block = &s[i..i+end];
                for line in block.lines() {
                    // Cases: <start> <end> <dst>  OR  <start> <end> [<d1><d2>...]
                    if let (Some(start_hex), Some(end_hex)) = (extract_hex(line, 0), extract_hex(line, 1)) {
                        let start = hex_to_u16(&start_hex);
                        let endc = hex_to_u16(&end_hex);
                        if let Some(vec_hex) = extract_hex_array(line) {
                            let mut code = start;
                            for dh in vec_hex {
                                if let Some(u) = hex_to_string(&dh) { map.single.insert(code, u); }
                                code += 1;
                                if code > endc { break; }
                            }
                        } else if let Some(dst_hex) = extract_hex(line, 2) {
                            // contiguous mapping; MVP: map start to dst, ignore others for now
                            if let Some(u) = hex_to_string(&dst_hex) { map.single.insert(start, u); }
                        }
                    }
                }
                i += end + "endbfrange".len();
                continue;
            } else { break; }
        }
        i += 1;
    }
    map
}

fn extract_hex(line: &str, idx: usize) -> Option<String> {
    // find the idx-th <...> token
    let mut n = 0;
    let bytes = line.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'<' {
            let j = line[i..].find('>')? + i;
            if n == idx { return Some(line[i+1..j].to_string()); }
            n += 1; i = j+1; continue;
        }
        i += 1;
    }
    None
}

fn extract_hex_array(line: &str) -> Option<Vec<String>> {
    // find [...] block and extract <...> tokens inside
    let start = line.find('[')?;
    let end = line[start..].find(']')? + start;
    let mut v = Vec::new();
    let mut i = start;
    let bytes = line.as_bytes();
    while i < end {
        if bytes[i] == b'<' {
            let j = line[i..].find('>')? + i;
            v.push(line[i+1..j].to_string());
            i = j+1; continue;
        }
        i += 1;
    }
    Some(v)
}

fn hex_to_u16(h: &str) -> u16 {
    u16::from_str_radix(h, 16).unwrap_or(0)
}

fn hex_to_string(h: &str) -> Option<String> {
    // interpret as big-endian UTF-16 if even length >= 4; else bytes
    if h.len() % 4 == 0 && h.len() >= 4 {
        let mut u16s = Vec::new();
        let mut i = 0usize;
        while i+4 <= h.len() {
            let part = &h[i..i+4];
            let val = u16::from_str_radix(part, 16).ok()?;
            u16s.push(val);
            i += 4;
        }
        String::from_utf16(&u16s).ok()
    } else if h.len() % 2 == 0 {
        let mut bytes = Vec::new();
        let mut i = 0usize;
        while i+2 <= h.len() {
            let part = &h[i..i+2];
            let val = u8::from_str_radix(part, 16).ok()?;
            bytes.push(val);
            i += 2;
        }
        String::from_utf8(bytes).ok()
    } else {
        None
    }
}

// Base encodings: WinAnsi (Windows-1252), MacRoman (macintosh), PDFDocEncoding (approx via Windows-1252 for now).
pub fn map_bytes_with_tounicode_or_base(tu: Option<&ToUnicodeMap>, base_enc: Option<&str>, bytes: &[u8]) -> String {
    if let Some(m) = tu { if !m.single.is_empty() { return m.map_bytes(bytes); } }
    if let Some(enc) = base_enc {
        // Decode using encoding_rs for WinAnsi/MacRoman; approximate PDFDocEncoding as Windows-1252.
        let cow = match enc {
            "WinAnsiEncoding" => WINDOWS_1252.decode_without_bom_handling_and_without_replacement(bytes),
            "MacRomanEncoding" => MACINTOSH.decode_without_bom_handling_and_without_replacement(bytes),
            // Approximation: PDFDocEncoding overlaps Windows-1252 in most ASCII range; refine if needed.
            "PDFDocEncoding" => WINDOWS_1252.decode_without_bom_handling_and_without_replacement(bytes),
            _ => None,
        };
        if let Some(s) = cow { return s.into_owned(); }
    }
    // Fallback: naive byte→char mapping
    bytes.iter().map(|&b| b as char).collect()
}
