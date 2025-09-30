use crate::fonts::map_bytes_with_tounicode_or_base;
use crate::resources::FontInfo;
use encoding_rs::WINDOWS_1252;
use std::collections::BTreeMap;

fn suspicious_score(s: &str) -> usize {
    // Count classic mojibake markers likely from double-decoding UTF-8
    s.matches('Ã').count() + s.matches('Â').count() + s.matches('â').count()
}

pub fn append_bytes_as_text(out: &mut String, s: &[u8]) {
    // 1) If valid UTF-8, use it directly (many PDFs with ToUnicode ultimately yield UTF-8-like slices)
    if let Ok(utf8) = std::str::from_utf8(s) {
        out.push_str(utf8);
        return;
    }
    // 2) Try Windows-1252 and naive Latin-1 mapping, pick the one with fewer suspicious markers
    let win = WINDOWS_1252
        .decode_without_bom_handling_and_without_replacement(s)
        .map(|cow| cow.to_string());
    let latin1: String = s.iter().map(|&b| b as char).collect();
    match win {
        Some(w) => {
            let sw = suspicious_score(&w);
            let sl = suspicious_score(&latin1);
            if sw <= sl {
                out.push_str(&w);
            } else {
                out.push_str(&latin1);
            }
        }
        None => out.push_str(&latin1),
    }
}

pub fn emit_mapped_text(
    out: &mut String,
    s: &[u8],
    current_font: &Option<String>,
    fonts: &BTreeMap<String, FontInfo>,
) {
    if let Some(ref fname) = current_font {
        if let Some(fi) = fonts.get(fname) {
            let mapped = map_bytes_with_tounicode_or_base(
                fi.to_unicode.as_ref(),
                fi.base_encoding.as_deref(),
                s,
            );
            out.push_str(&mapped);
            return;
        }
    }
    append_bytes_as_text(out, s);
}
