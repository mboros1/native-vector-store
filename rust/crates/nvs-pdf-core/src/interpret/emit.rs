use crate::fonts::map_bytes_with_tounicode_or_base;
use crate::resources::FontInfo;
use std::collections::BTreeMap;

pub fn append_bytes_as_text(out: &mut String, s: &[u8]) {
    match String::from_utf8(s.to_vec()) {
        Ok(t) => out.push_str(&t),
        Err(_) => {
            for &b in s { out.push(b as char); }
        }
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

