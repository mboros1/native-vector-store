// Text normalization helpers for page text post-processing.

pub fn normalize_page_text(s: &str) -> String {
    let mut t = s.replace("\r\n", "\n").replace('\r', "\n");
    // Strip soft hyphen and zero-width characters; drop C1 control chars (U+0080..U+009F)
    {
        let mut out = String::with_capacity(t.len());
        for ch in t.chars() {
            match ch {
                '\u{00AD}' | '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{FEFF}' => {
                    // skip
                }
                c if (0x80..=0x9F).contains(&(c as u32)) => {
                    // skip C1 controls that often appear from bad decodes
                }
                // Common ligatures → plain letters
                '\u{FB00}' => out.push_str("ff"),
                '\u{FB01}' => out.push_str("fi"),
                '\u{FB02}' => out.push_str("fl"),
                '\u{FB03}' => out.push_str("ffi"),
                '\u{FB04}' => out.push_str("ffl"),
                _ => out.push(ch),
            }
        }
        t = out;
    }
    {
        // de-hyphenate
        let mut out = String::with_capacity(t.len());
        let bytes = t.as_bytes();
        let mut i = 0usize;
        while i < bytes.len() {
            if i + 2 < bytes.len() && bytes[i] == b'-' && bytes[i + 1] == b'\n' {
                let prev = if i > 0 { bytes[i - 1] } else { b' ' };
                let next = bytes[i + 2];
                if prev.is_ascii_alphabetic() && next.is_ascii_lowercase() {
                    i += 2;
                    continue;
                }
            }
            out.push(bytes[i] as char);
            i += 1;
        }
        t = out;
    }
    {
        // join D\niabetes
        let mut out = String::with_capacity(t.len());
        let b = t.as_bytes();
        let mut i = 0usize;
        while i < b.len() {
            if i + 2 < b.len()
                && b[i].is_ascii_alphabetic()
                && b[i + 1] == b'\n'
                && b[i + 2].is_ascii_lowercase()
            {
                out.push(b[i] as char);
                out.push(b[i + 2] as char);
                i += 3;
                continue;
            }
            out.push(b[i] as char);
            i += 1;
        }
        t = out;
    }
    // Conservative: do not attempt blanket mojibake "repair" here to avoid unintended regressions.
    t
}
