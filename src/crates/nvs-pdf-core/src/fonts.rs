use encoding_rs::{MACINTOSH, WINDOWS_1252};
use std::collections::HashMap;

#[derive(Debug, Default, Clone)]
pub struct ToUnicodeMap {
    // Discrete mappings for variable-length codes (greedy by length on lookup)
    map: HashMap<Vec<u8>, String>,
    // Compact contiguous ranges (do not expand to discrete entries)
    ranges1: Vec<Range>,
    ranges2: Vec<Range>,
    ranges3: Vec<Range>,
    ranges4: Vec<Range>,
    max_key_len: usize,
}

#[derive(Debug, Clone, Copy)]
struct Range {
    start: u32, // big-endian code interpreted as integer
    end: u32,
    start_cp: u32, // Unicode codepoint for start
}

impl ToUnicodeMap {
    pub fn insert(&mut self, key: Vec<u8>, val: String) {
        if !key.is_empty() {
            self.max_key_len = self.max_key_len.max(key.len());
            self.map.insert(key, val);
        }
    }

    pub fn insert_range(&mut self, key_len: usize, start: u32, end: u32, start_cp: u32) {
        if key_len == 0 || key_len > 4 || start > end { return; }
        self.max_key_len = self.max_key_len.max(key_len);
        let r = Range { start, end, start_cp };
        match key_len {
            1 => self.ranges1.push(r),
            2 => self.ranges2.push(r),
            3 => self.ranges3.push(r),
            4 => self.ranges4.push(r),
            _ => {}
        }
    }

    pub fn finalize(&mut self) {
        // Sort ranges by start for binary search
        let by_start = |a: &Range, b: &Range| a.start.cmp(&b.start);
        self.ranges1.sort_by(by_start);
        self.ranges2.sort_by(by_start);
        self.ranges3.sort_by(by_start);
        self.ranges4.sort_by(by_start);
    }

    fn find_in_ranges(ranges: &Vec<Range>, code: u32) -> Option<u32> {
        if ranges.is_empty() { return None; }
        let mut lo = 0usize;
        let mut hi = ranges.len();
        while lo < hi {
            let mid = (lo + hi) / 2;
            let r = ranges[mid];
            if code < r.start { hi = mid; }
            else if code > r.end { lo = mid + 1; }
            else {
                // inside range
                return Some(r.start_cp + (code - r.start));
            }
        }
        None
    }

    pub fn map_bytes(&self, bytes: &[u8]) -> String {
        if self.map.is_empty() && self.ranges1.is_empty() && self.ranges2.is_empty() && self.ranges3.is_empty() && self.ranges4.is_empty() {
            return bytes.iter().map(|&b| b as char).collect();
        }
        let mut out = String::new();
        let mut i = 0usize;
        while i < bytes.len() {
            let mut matched = false;
            let maxl = self.max_key_len.min(bytes.len() - i);
            let mut l = maxl;
            while l > 0 {
                let slice = &bytes[i..i + l];
                if let Some(s) = self.map.get(slice) {
                    out.push_str(s);
                    i += l;
                    matched = true;
                    break;
                }
                // Try range bucket for this length
                let code = be_to_u32(slice);
                let cp_opt = match l {
                    1 => Self::find_in_ranges(&self.ranges1, code),
                    2 => Self::find_in_ranges(&self.ranges2, code),
                    3 => Self::find_in_ranges(&self.ranges3, code),
                    4 => Self::find_in_ranges(&self.ranges4, code),
                    _ => None,
                };
                if let Some(cp) = cp_opt {
                    if let Some(ch) = std::char::from_u32(cp) { out.push(ch); } else { out.push('\u{FFFD}'); }
                    i += l;
                    matched = true;
                    break;
                }
                l -= 1;
            }
            if !matched {
                out.push(bytes[i] as char);
                i += 1;
            }
        }
        out
    }
    pub fn is_empty(&self) -> bool {
        self.map.is_empty() && self.ranges1.is_empty() && self.ranges2.is_empty() && self.ranges3.is_empty() && self.ranges4.is_empty()
    }
}

pub fn parse_tounicode_cmap(data: &[u8]) -> ToUnicodeMap {
    let s = match std::str::from_utf8(data) {
        Ok(v) => v,
        Err(_) => return ToUnicodeMap::default(),
    };
    let mut map = ToUnicodeMap::default();
    let mut i = 0usize;
    while i < s.len() {
        if s[i..].starts_with("beginbfchar") {
            if let Some(end) = s[i..].find("endbfchar") {
                let block = &s[i..i + end];
                for line in block.lines() {
                    let (src_opt, dst_opt) = (extract_hex(line, 0), extract_hex(line, 1));
                    if let (Some(src), Some(dst)) = (src_opt, dst_opt) {
                        if let Some(src_bytes) = hex_to_bytes(&src) {
                            if let Some(u) = hex_to_string(&dst) {
                                map.insert(src_bytes, u);
                            }
                        }
                    }
                }
                i += end + "endbfchar".len();
                continue;
            } else {
                break;
            }
        }
        if s[i..].starts_with("beginbfrange") {
            if let Some(end) = s[i..].find("endbfrange") {
                let block = &s[i..i + end];
                for line in block.lines() {
                    if let (Some(start_hex), Some(end_hex)) = (extract_hex(line, 0), extract_hex(line, 1)) {
                        let start_bytes = match hex_to_bytes(&start_hex) { Some(v) => v, None => continue };
                        let end_bytes = match hex_to_bytes(&end_hex) { Some(v) => v, None => continue };
                        if start_bytes.len() != end_bytes.len() { continue; }
                        if let Some(vec_hex) = extract_hex_array(line) {
                            // Array of explicit destinations
                            let mut cur = start_bytes.clone();
                            for dh in vec_hex {
                                if cur > end_bytes { break; }
                                if let Some(u) = hex_to_string(&dh) {
                                    map.insert(cur.clone(), u);
                                }
                                incr_be_bytes(&mut cur);
                            }
                        } else if let Some(dst_hex) = extract_hex(line, 2) {
                            // Single destination: treat as starting Unicode scalar, map the full range
                            if let Some(u) = hex_to_string(&dst_hex) {
                                // Only support simple single-codepoint increments via range
                                let mut chars = u.chars();
                                if let (Some(first), None) = (chars.next(), chars.next()) {
                                    let start_code = be_to_u32(&start_bytes);
                                    let end_code = be_to_u32(&end_bytes);
                                    map.insert_range(start_bytes.len(), start_code, end_code, first as u32);
                                } else {
                                    // Complex multi-codepoint dst for start only
                                    map.insert(start_bytes.clone(), u);
                                }
                            }
                        }
                    }
                }
                i += end + "endbfrange".len();
                continue;
            } else {
                break;
            }
        }
        i += 1;
    }
    map.finalize();
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tounicode_beginbfchar_simple() {
        let cmap = b"beginbfchar\n<01> <0041>\nendbfchar";
        let tu = parse_tounicode_cmap(cmap);
        let mapped = tu.map_bytes(&[0x01]);
        assert_eq!(mapped, "A");
    }

    #[test]
    fn tounicode_beginbfrange_contiguous() {
        let cmap = b"beginbfrange\n<0010> <0012> <0041>\nendbfrange"; // 0x0010..0x0012 -> 'A'..'C'
        let tu = parse_tounicode_cmap(cmap);
        assert_eq!(tu.map_bytes(&[0x00, 0x10]), "A");
        assert_eq!(tu.map_bytes(&[0x00, 0x11]), "B");
        assert_eq!(tu.map_bytes(&[0x00, 0x12]), "C");
    }

    #[test]
    fn tounicode_beginbfrange_array() {
        let cmap = b"beginbfrange\n<20> <22> [<0044><0045><0046>]\nendbfrange"; // 0x20..0x22 -> 'D','E','F'
        let tu = parse_tounicode_cmap(cmap);
        assert_eq!(tu.map_bytes(&[0x20]), "D");
        assert_eq!(tu.map_bytes(&[0x21]), "E");
        assert_eq!(tu.map_bytes(&[0x22]), "F");
    }
}

fn extract_hex(line: &str, idx: usize) -> Option<String> {
    // find the idx-th <...> token
    let mut n = 0;
    let bytes = line.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'<' {
            let j = line[i..].find('>')? + i;
            if n == idx {
                return Some(line[i + 1..j].to_string());
            }
            n += 1;
            i = j + 1;
            continue;
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
            v.push(line[i + 1..j].to_string());
            i = j + 1;
            continue;
        }
        i += 1;
    }
    Some(v)
}

fn hex_to_bytes(h: &str) -> Option<Vec<u8>> {
    if h.len() % 2 != 0 { return None; }
    let mut out = Vec::with_capacity(h.len() / 2);
    let bytes = h.as_bytes();
    let mut i = 0usize;
    while i + 1 < bytes.len() {
        let part = &h[i..i+2];
        let val = u8::from_str_radix(part, 16).ok()?;
        out.push(val);
        i += 2;
    }
    Some(out)
}

fn hex_to_string(h: &str) -> Option<String> {
    // interpret as big-endian UTF-16 if even length >= 4; else bytes
    if h.len() % 4 == 0 && h.len() >= 4 {
        let mut u16s = Vec::new();
        let mut i = 0usize;
        while i + 4 <= h.len() {
            let part = &h[i..i + 4];
            let val = u16::from_str_radix(part, 16).ok()?;
            u16s.push(val);
            i += 4;
        }
        String::from_utf16(&u16s).ok()
    } else if h.len() % 2 == 0 {
        let mut bytes = Vec::new();
        let mut i = 0usize;
        while i + 2 <= h.len() {
            let part = &h[i..i + 2];
            let val = u8::from_str_radix(part, 16).ok()?;
            bytes.push(val);
            i += 2;
        }
        String::from_utf8(bytes).ok()
    } else {
        None
    }
}

fn incr_be_bytes(b: &mut [u8]) {
    // Increment big-endian byte vector by 1
    for i in (0..b.len()).rev() {
        if b[i] == 0xFF { b[i] = 0x00; } else { b[i] += 1; break; }
    }
}

fn be_to_u32(bytes: &[u8]) -> u32 {
    let mut v: u32 = 0;
    for &b in bytes { v = (v << 8) | (b as u32); }
    v
}

// Base encodings: WinAnsi (Windows-1252), MacRoman (macintosh), PDFDocEncoding (approx via Windows-1252 for now).
pub fn map_bytes_with_tounicode_or_base(
    tu: Option<&ToUnicodeMap>,
    base_enc: Option<&str>,
    bytes: &[u8],
) -> String {
    fn suspicious_score(s: &str) -> usize {
        s.matches('Ã').count() + s.matches('Â').count() + s.matches('â').count()
    }
    fn cp1252_reverse_encode(s: &str) -> Option<Vec<u8>> {
        // Map Unicode chars back to Windows-1252 codepoints; return None if any char is not representable.
        fn cpbyte(ch: char) -> Option<u8> {
            let u = ch as u32;
            match u {
                0x0000..=0x00FF => Some(u as u8),
                0x20AC => Some(0x80), // €
                0x201A => Some(0x82), // ‚
                0x0192 => Some(0x83), // ƒ
                0x201E => Some(0x84), // „
                0x2026 => Some(0x85), // …
                0x2020 => Some(0x86), // †
                0x2021 => Some(0x87), // ‡
                0x02C6 => Some(0x88), // ˆ
                0x2030 => Some(0x89), // ‰
                0x0160 => Some(0x8A), // Š
                0x2039 => Some(0x8B), // ‹
                0x0152 => Some(0x8C), // Œ
                0x017D => Some(0x8E), // Ž
                0x2018 => Some(0x91), // ‘
                0x2019 => Some(0x92), // ’
                0x201C => Some(0x93), // “
                0x201D => Some(0x94), // ”
                0x2022 => Some(0x95), // •
                0x2013 => Some(0x96), // –
                0x2014 => Some(0x97), // —
                0x02DC => Some(0x98), // ˜
                0x2122 => Some(0x99), // ™
                0x0161 => Some(0x9A), // š
                0x203A => Some(0x9B), // ›
                0x0153 => Some(0x9C), // œ
                0x017E => Some(0x9E), // ž
                0x0178 => Some(0x9F), // Ÿ
                _ => None,
            }
        }
        let mut out = Vec::with_capacity(s.len());
        for ch in s.chars() {
            if let Some(b) = cpbyte(ch) { out.push(b); } else { return None; }
        }
        Some(out)
    }
    fn repair_mojibake(decoded: &str) -> Option<String> {
        let before = suspicious_score(decoded);
        if before == 0 { return None; }
        if let Some(bytes) = cp1252_reverse_encode(decoded) {
            if let Ok(red) = String::from_utf8(bytes) {
                let after = suspicious_score(&red);
                if after < before { return Some(red); }
            }
        }
        None
    }
    if let Some(m) = tu {
        if !m.is_empty() {
            let s = m.map_bytes(bytes);
            if let Some(fixed) = repair_mojibake(&s) { return fixed; }
            return s;
        }
    }
    if let Some(enc) = base_enc {
        // Decode using encoding_rs for WinAnsi/MacRoman; approximate PDFDocEncoding as Windows-1252.
        let cow = match enc {
            "WinAnsiEncoding" => {
                WINDOWS_1252.decode_without_bom_handling_and_without_replacement(bytes)
            }
            "MacRomanEncoding" => {
                MACINTOSH.decode_without_bom_handling_and_without_replacement(bytes)
            }
            // Approximation: PDFDocEncoding overlaps Windows-1252 in most ASCII range; refine if needed.
            "PDFDocEncoding" => {
                WINDOWS_1252.decode_without_bom_handling_and_without_replacement(bytes)
            }
            _ => None,
        };
        if let Some(s) = cow {
            let base_decoded = s.into_owned();
            // If bytes also form valid UTF-8, prefer the version with fewer mojibake markers.
            if let Ok(utf8) = std::str::from_utf8(bytes) {
                let bscore = suspicious_score(&base_decoded);
                let uscore = suspicious_score(utf8);
                if uscore < bscore {
                    let good = utf8.to_string();
                    if let Some(fixed) = repair_mojibake(&good) { return fixed; }
                    return good;
                }
            }
            if let Some(fixed) = repair_mojibake(&base_decoded) { return fixed; }
            return base_decoded;
        }
    }
    // Fallback: naive byte→char mapping
    let s: String = bytes.iter().map(|&b| b as char).collect();
    if let Some(fixed) = repair_mojibake(&s) { return fixed; }
    s
}
