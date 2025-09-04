// Text normalization helpers for page text post-processing.

pub fn normalize_page_text(s: &str) -> String {
    let mut t = s.replace("\r\n", "\n").replace('\r', "\n");
    { // de-hyphenate
        let mut out = String::with_capacity(t.len());
        let bytes = t.as_bytes();
        let mut i = 0usize;
        while i < bytes.len() {
            if i+2 < bytes.len() && bytes[i] == b'-' && bytes[i+1] == b'\n' {
                let prev = if i>0 { bytes[i-1] } else { b' ' };
                let next = bytes[i+2];
                if prev.is_ascii_alphabetic() && next.is_ascii_lowercase() { i += 2; continue; }
            }
            out.push(bytes[i] as char); i += 1;
        }
        t = out;
    }
    { // join D\niabetes
        let mut out = String::with_capacity(t.len());
        let b = t.as_bytes(); let mut i = 0usize;
        while i < b.len() {
            if i+2 < b.len() && b[i].is_ascii_alphabetic() && b[i+1] == b'\n' && b[i+2].is_ascii_lowercase() {
                out.push(b[i] as char); out.push(b[i+2] as char); i += 3; continue;
            }
            out.push(b[i] as char); i += 1;
        }
        t = out;
    }
    t
}

