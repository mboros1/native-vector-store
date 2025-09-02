// Simple UTF-8 aware tokenizer roughly matching C++ SimpleTokenizer semantics used for BM25.
// - Keeps non-ASCII codepoints inside tokens
// - Keeps ASCII alphanumerics and '_' inside tokens
// - Allows in-word punctuation: '\'', '-', '/', '&'
// - Splits other punctuation into separate tokens by surrounding with spaces
// - Ellipsis '...' split into individual '.' tokens
// - Single period at end-of-line/text is split as '.'; internal periods remain in-word

pub struct SimpleTokenizer;

impl SimpleTokenizer {
    pub fn new() -> Self { Self }

    pub fn split(&self, input: &str) -> Vec<String> {
        if input.is_empty() { return Vec::new(); }
        let pre = self.process_delimiters(input);
        // split on ASCII whitespace
        let mut tokens: Vec<String> = pre.split_whitespace().map(|s| s.to_string()).collect();
        // Post-processing: if a token ends with '.', split it unless abbreviation
        let mut out: Vec<String> = Vec::with_capacity(tokens.len() + 4);
        for t in tokens.drain(..) {
            if let Some(last) = t.as_bytes().last() {
                if *last == b'.' {
                    let stem = &t[..t.len()-1];
                    if !stem.is_empty() && !is_abbreviation(stem) {
                        out.push(stem.to_string());
                        out.push(".".to_string());
                        continue;
                    }
                }
            }
            out.push(t);
        }
        out
    }

    fn process_delimiters(&self, text: &str) -> String {
        let mut out = String::with_capacity(text.len() * 2);

        let mut i = 0;
        let b = text.as_bytes();
        while i < b.len() {
            let (cp, len) = decode_utf8(&b[i..]);
            if is_whitespace(cp) {
                if out.as_bytes().last().copied() != Some(b' ') { out.push(' '); }
            } else if cp == b'.' as u32 {
                // ellipsis
                let mut j = i; let mut run = 0;
                while j < b.len() && b[j] == b'.' { j += 1; run += 1; }
                if run >= 3 {
                    for _ in 0..run { if out.as_bytes().last().copied() != Some(b' ') { out.push(' ');} out.push('.'); out.push(' '); }
                    i += run; continue;
                } else {
                    // EOL period?
                    let mut k = i + 1;
                    while k < b.len() && (b[k] == b' ' || b[k] == b'\t' || b[k] == b'\r') { k += 1; }
                    if k >= b.len() || (k < b.len() && b[k] == b'\n') {
                        if out.as_bytes().last().copied() != Some(b' ') { out.push(' ');} out.push('.'); out.push(' ');
                    } else {
                        out.push('.');
                    }
                }
            } else if is_word(cp) {
                // append raw bytes
                out.push_str(unsafe { std::str::from_utf8_unchecked(&b[i..i+len]) });
            } else {
                if out.as_bytes().last().copied() != Some(b' ') { out.push(' ');} 
                out.push_str(unsafe { std::str::from_utf8_unchecked(&b[i..i+len]) });
                out.push(' ');
            }
            i += len;
        }
        out
    }
}

fn decode_utf8(s: &[u8]) -> (u32, usize) {
    let c = s[0];
    if c < 0x80 { return (c as u32, 1); }
    if c & 0xE0 == 0xC0 && s.len() >= 2 { return ((((c & 0x1F) as u32) << 6) | ((s[1] & 0x3F) as u32), 2); }
    if c & 0xF0 == 0xE0 && s.len() >= 3 { return ((((c & 0x0F) as u32) << 12) | (((s[1] & 0x3F) as u32) << 6) | ((s[2] & 0x3F) as u32), 3); }
    if c & 0xF8 == 0xF0 && s.len() >= 4 { return ((((c & 0x07) as u32) << 18) | (((s[1] & 0x3F) as u32) << 12) | (((s[2] & 0x3F) as u32) << 6) | ((s[3] & 0x3F) as u32), 4); }
    (c as u32, 1)
}

fn is_whitespace(cp: u32) -> bool {
    cp == b' ' as u32 || cp == b'\t' as u32 || cp == b'\n' as u32 || cp == b'\r' as u32
}

fn is_ascii_alnum_underscore(cp: u32) -> bool {
    (cp >= b'A' as u32 && cp <= b'Z' as u32) ||
    (cp >= b'a' as u32 && cp <= b'z' as u32) ||
    (cp >= b'0' as u32 && cp <= b'9' as u32) ||
    cp == b'_' as u32
}

fn is_allowed_punct(cp: u32) -> bool {
    cp == b'.' as u32 || cp == b'\'' as u32 || cp == b'-' as u32 || cp == b'/' as u32 || cp == b'&' as u32
}

fn is_word(cp: u32) -> bool {
    if cp >= 0x80 { return true; }
    if is_ascii_alnum_underscore(cp) { return true; }
    if is_allowed_punct(cp) { return true; }
    false
}

fn is_abbreviation(tok: &str) -> bool {
    // Match the C++ minimal set for parity
    const ABBRS: &[&str] = &[
        "Dr","Mr","Mrs","Ms","Prof","Sr","Jr",
        "Ph","M","B","D",
        "Inc","Corp","Co","Ltd",
        "Jan","Feb","Mar","Apr","Jun","Jul","Aug","Sep","Sept","Oct","Nov","Dec",
        "Mon","Tue","Wed","Thu","Fri","Sat","Sun",
        "St","Ave","Rd","Blvd",
        "U","S","N","E","W",
        "vs","etc","al","eg","ie","cf",
    ];
    ABBRS.contains(&tok)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_tokens() {
        let t = SimpleTokenizer::new();
        assert_eq!(t.split("Hello, world!").as_slice(), ["Hello", ",", "world", "!"]);
        assert_eq!(t.split("self-driving and/or R&D").as_slice(), ["self-driving","and/or","R&D"]);
        assert_eq!(t.split("End of sentence.").as_slice(), ["End","of","sentence","."]);
    }

    #[test]
    fn unicode() {
        let t = SimpleTokenizer::new();
        assert_eq!(t.split("café naïve").as_slice(), ["café","naïve"]);
        assert_eq!(t.split("привет мир").as_slice(), ["привет","мир"]);
    }

    #[test]
    fn urls_emails_commas() {
        let t = SimpleTokenizer::new();
        assert_eq!(t.split("one,two,three").as_slice(), ["one", ",", "two", ",", "three"]);
        assert_eq!(t.split("contact user@example.com today").as_slice(), ["contact","user","@","example.com","today"]);
        assert_eq!(t.split("Visit https://example.com/page").as_slice(), ["Visit","https",":","//example.com/page"]);
    }

    #[test]
    fn quotes_paren_currency() {
        let t = SimpleTokenizer::new();
        assert_eq!(t.split("\"quoted\"").as_slice(), ["\"","quoted","\""]);
        assert_eq!(t.split("(example)").as_slice(), ["(","example",")"]);
        assert_eq!(t.split("$100 €50 £25").as_slice(), ["$","100","€50","£25"]);
    }

    #[test]
    fn periods_and_abbrev() {
        let t = SimpleTokenizer::new();
        assert_eq!(t.split("...").as_slice(), [".",".","."]); // ellipsis split
        // Abbreviations keep period when in-word
        assert_eq!(t.split("Dr. Smith").as_slice(), ["Dr.","Smith"]);
        // Multi-part: "U.S." -> split trailing period per C++ behavior, known limitation
        assert_eq!(t.split("U.S. government").as_slice(), ["U.S",".","government"]);
        // Abbreviation alone at EOL splits period
        assert_eq!(t.split("Dr.").as_slice(), ["Dr","."]);
    }

    #[test]
    fn whitespace_cases() {
        let t = SimpleTokenizer::new();
        assert_eq!(t.split("").as_slice(), [] as [&str;0]);
        assert_eq!(t.split("   \t  \n  ").as_slice(), [] as [&str;0]);
        assert_eq!(t.split("multiple   spaces    here").as_slice(), ["multiple","spaces","here"]);
        assert_eq!(t.split("line1\nline2\ttab").as_slice(), ["line1","line2","tab"]);
        assert_eq!(t.split("  \t  word1 word2  \n  ").as_slice(), ["word1","word2"]);
    }

    #[test]
    fn numbers_and_mixed() {
        let t = SimpleTokenizer::new();
        assert_eq!(t.split("123 456.78").as_slice(), ["123","456.78"]);
        assert_eq!(t.split("test123 456test").as_slice(), ["test123","456test"]);
    }

    #[test]
    fn operators_percent_dates_time() {
        let t = SimpleTokenizer::new();
        assert_eq!(t.split("2+2=4").as_slice(), ["2","+","2","=","4"]);
        assert_eq!(t.split("100% complete").as_slice(), ["100","%","complete"]);
        assert_eq!(t.split("12/25/2024").as_slice(), ["12/25/2024"]);
        assert_eq!(t.split("2024-12-25").as_slice(), ["2024-12-25"]);
        assert_eq!(t.split("3:30pm").as_slice(), ["3",":","30pm"]);
    }

    #[test]
    fn multiple_delimiters_and_apostrophes() {
        let t = SimpleTokenizer::new();
        assert_eq!(t.split("word!!!???...").as_slice(), ["word","!","!","!","?","?","?",".",".","."]);
        // straight and curly apostrophes should both behave as in-word punctuation
        assert_eq!(t.split("it's it’s").as_slice(), ["it's","it’s"]);
    }
}
