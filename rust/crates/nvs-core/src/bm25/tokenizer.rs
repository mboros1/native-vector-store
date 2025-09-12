// Simple UTF-8 aware tokenizer roughly matching C++ SimpleTokenizer semantics used for BM25.
// - Keeps non-ASCII codepoints inside tokens
// - Keeps ASCII alphanumerics and '_' inside tokens
// - Allows in-word punctuation: '\'', '-', '/', '&'
// - Splits other punctuation into separate tokens by surrounding with spaces
// - Ellipsis '...' split into individual '.' tokens
// - Single period at end-of-line/text is split as '.'; internal periods remain in-word

#[derive(Clone, Copy, Debug)]
pub struct TokenizerOptions {
    pub lowercase: bool,
    pub split_contractions: bool,
    pub remove_stopwords: bool,
    pub remove_punctuation: bool,
}

impl Default for TokenizerOptions {
    fn default() -> Self {
        Self {
            lowercase: false,
            split_contractions: false,
            remove_stopwords: false,
            remove_punctuation: false,
        }
    }
}

pub struct SimpleTokenizer {
    opts: TokenizerOptions,
}

impl SimpleTokenizer {
    pub fn new() -> Self {
        Self {
            opts: TokenizerOptions::default(),
        }
    }

    pub fn with_options(opts: TokenizerOptions) -> Self {
        Self { opts }
    }

    pub fn split(&self, input: &str) -> Vec<String> {
        if input.is_empty() {
            return Vec::new();
        }
        // optional lowercase
        let mut text = if self.opts.lowercase {
            input.to_lowercase()
        } else {
            input.to_string()
        };
        // optional contractions
        if self.opts.split_contractions {
            text = self.process_contractions(&text);
        }
        let pre = self.process_delimiters(&text);
        // split on ASCII whitespace
        let mut tokens: Vec<String> = pre.split_whitespace().map(|s| s.to_string()).collect();
        // Post-processing: if a token ends with '.', split it unless abbreviation
        let mut out: Vec<String> = Vec::with_capacity(tokens.len() + 4);
        for t in tokens.drain(..) {
            if let Some(last) = t.as_bytes().last() {
                if *last == b'.' {
                    let stem = &t[..t.len() - 1];
                    if !stem.is_empty() && !is_abbreviation(stem) {
                        out.push(stem.to_string());
                        out.push(".".to_string());
                        continue;
                    }
                }
            }
            // optional stopwords removal
            if self.opts.remove_stopwords && is_stopword(&t) {
                continue;
            }
            // optional punctuation removal
            if self.opts.remove_punctuation && is_punctuation(&t) {
                continue;
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
                if out.as_bytes().last().copied() != Some(b' ') {
                    out.push(' ');
                }
            } else if cp == b'.' as u32 {
                // ellipsis
                let mut j = i;
                let mut run = 0;
                while j < b.len() && b[j] == b'.' {
                    j += 1;
                    run += 1;
                }
                if run >= 3 {
                    for _ in 0..run {
                        if out.as_bytes().last().copied() != Some(b' ') {
                            out.push(' ');
                        }
                        out.push('.');
                        out.push(' ');
                    }
                    i += run;
                    continue;
                } else {
                    // EOL period?
                    let mut k = i + 1;
                    while k < b.len() && (b[k] == b' ' || b[k] == b'\t' || b[k] == b'\r') {
                        k += 1;
                    }
                    if k >= b.len() || (k < b.len() && b[k] == b'\n') {
                        if out.as_bytes().last().copied() != Some(b' ') {
                            out.push(' ');
                        }
                        out.push('.');
                        out.push(' ');
                    } else {
                        out.push('.');
                    }
                }
            } else if is_word(cp) {
                // append raw bytes
                out.push_str(unsafe { std::str::from_utf8_unchecked(&b[i..i + len]) });
            } else {
                if out.as_bytes().last().copied() != Some(b' ') {
                    out.push(' ');
                }
                out.push_str(unsafe { std::str::from_utf8_unchecked(&b[i..i + len]) });
                out.push(' ');
            }
            i += len;
        }
        out
    }

    fn process_contractions(&self, text: &str) -> String {
        // Very simple, ASCII-focused; mirrors C++ intent
        // Expand special cases
        let mut s = text
            .replace("won't", "will not")
            .replace("Won't", "Will not")
            .replace("shan't", "shall not")
            .replace("Shan't", "Shall not")
            .replace("can't", "can not")
            .replace("Can't", "Can not")
            .replace("ain't", "is not")
            .replace("Ain't", "Is not")
            .replace("cannot", "can not")
            .replace("Cannot", "Can not");
        // Split n't -> not
        s = s.replace("n't", " not");
        // Split 'll, 're, 've, 's, 'm, 'd to separate tokens
        for suf in ["'ll", "'re", "'ve", "'s", "'m", "'d"] {
            s = s.replace(suf, &format!(" {}", suf));
        }
        s
    }
}

// Normalize text for BM25:
// - Convert control chars (\r, \f, \t, etc.) to spaces/newlines
// - Remove soft hyphen (U+00AD) and zero-width/BOM characters
// - Dehyphenate line breaks: "word-\nnext" -> "word next"
// - Collapse multiple whitespace into single spaces
pub fn preprocess_bm25(input: &str) -> String {
    if input.is_empty() { return String::new(); }
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\u{00AD}' | '\u{200B}' | '\u{FEFF}' => { /* skip soft hyphen/zero-width/BOM */ }
            '\r' | '\t' => { out.push(' '); }
            '\x0C' => { out.push(' '); } // form feed
            '-' => {
                // If hyphen is followed by a line break or whitespace+linebreak, treat as hyphenation -> space
                let it = chars.clone();
                let mut is_break = false;
                let mut consumed = 0;
                for nc in it {
                    if nc == '\n' {
                        is_break = true;
                        consumed += 1;
                        break;
                    } else if nc == '\r' || nc == '\t' || nc == ' ' {
                        consumed += 1;
                        continue;
                    } else { break; }
                }
                if is_break {
                    // consume the peeked whitespace/break
                    for _ in 0..consumed { let _ = chars.next(); }
                    out.push(' ');
                } else {
                    out.push('-');
                }
            }
            '\n' => { out.push(' '); }
            c if c.is_control() => { out.push(' '); }
            c => out.push(c),
        }
    }
    // collapse whitespace
    let mut collapsed = String::with_capacity(out.len());
    let mut last_space = false;
    for c in out.chars() {
        if c.is_whitespace() {
            if !last_space {
                collapsed.push(' ');
                last_space = true;
            }
        } else {
            collapsed.push(c);
            last_space = false;
        }
    }
    collapsed
}

// BM25 term filter: drop numeric-only and punctuation/noise tokens.
// Heuristics:
// - Keep if token contains at least 1 alphabetic character and length >= 2 after trimming punctuation.
// - Drop if token contains no alphabetic characters (numeric-like), allowing only digits and [+-.,/].
// - Drop if very short (< 2 chars) after trim.
fn strip_possessive(s: &str) -> &str {
    // Remove trailing 's or ’s using char boundaries
    let mut prev: Option<(usize, char)> = None;
    let mut last: Option<(usize, char)> = None;
    for (i, c) in s.char_indices() {
        prev = last;
        last = Some((i, c));
    }
    if let (Some((pi, pc)), Some((_li, lc))) = (prev, last) {
        if (lc == 's' || lc == 'S') && (pc == '\'' || pc == '\u{2019}') {
            return &s[..pi];
        }
    }
    s
}

pub fn bm25_keep_token(mut tok: &str) -> bool {
    if tok.is_empty() { return false; }
    // Trim common leading/trailing punctuation
    fn is_trim_punct(c: char) -> bool { matches!(c, '.'|','|';'|':'|'"'|'\''|'(' |')'|'['|']'|'{'|'}'|'!'|'?'|'%'|'+'|'-'|'/'|'\\'|'*'|'&'|'#'|'@'|'~'|'`'|'|') }
    tok = tok.trim_matches(is_trim_punct);
    if tok.len() < 2 { return false; }
    // Strip possessive endings: 's or ’s (safe on char boundaries)
    tok = strip_possessive(tok);
    if tok.len() < 2 { return false; }
    // Drop URL tracking params
    if tok.len() >= 4 && tok.as_bytes()[0..4].eq_ignore_ascii_case(b"utm_") { return false; }
    // Drop tokens with triple hyphen runs (formatting/artifacts)
    if tok.contains("---") { return false; }
    let mut has_ascii_letter = false;
    let mut upper_seq_only = true;
    for ch in tok.chars() {
        if ch.is_ascii_alphabetic() { has_ascii_letter = true; }
        if !matches!(ch, 'A'|'C'|'D'|'E'|'F'|'G'|'H'|'I'|'K'|'L'|'M'|'N'|'P'|'Q'|'R'|'S'|'T'|'V'|'W'|'Y'|'-') { upper_seq_only = false; }
    }
    if has_ascii_letter {
        // Drop long amino-acid sequence-like tokens
        if upper_seq_only && tok.len() >= 10 { return false; }
        return true; // keep alpha-containing tokens
    }
    // No letters: numeric-like? Allow only digits and simple numeric punctuation
    for ch in tok.chars() {
        if !(ch.is_ascii_digit() || matches!(ch, '+'|'-'|'.'|','|'/'|'\\')) {
            // Contains other symbols; drop
            return false;
        }
    }
    // All digits and numeric punctuation -> drop
    false
}

// Return a normalized token for BM25 (trim punctuation, strip possessive), or None to drop.
pub fn bm25_normalize_token(tok: &str) -> Option<String> {
    if tok.is_empty() { return None; }
    // Drop tokens with triple hyphens anywhere (formatting/artifacts)
    if tok.contains("---") { return None; }
    fn is_trim_punct(c: char) -> bool { matches!(c, '.'|','|';'|':'|'"'|'\''|'(' |')'|'['|']'|'{'|'}'|'!'|'?'|'%'|'+'|'-'|'/'|'\\'|'*'|'&'|'#'|'@'|'~'|'`'|'|') }
    let mut s = tok.trim_matches(is_trim_punct);
    if s.is_empty() { return None; }
    s = strip_possessive(s);
    if s.len() < 2 { return None; }
    // Normalize case by caller; still apply filters
    if s.len() >= 4 && s.as_bytes()[0..4].eq_ignore_ascii_case(b"utm_") { return None; }
    if s.contains("---") { return None; }
    // Check letters and AA-sequence drop
    let mut has_ascii_letter = false;
    let mut upper_seq_only = true;
    for ch in s.chars() {
        if ch.is_ascii_alphabetic() { has_ascii_letter = true; }
        if !matches!(ch, 'A'|'C'|'D'|'E'|'F'|'G'|'H'|'I'|'K'|'L'|'M'|'N'|'P'|'Q'|'R'|'S'|'T'|'V'|'W'|'Y'|'-') { upper_seq_only = false; }
    }
    if has_ascii_letter {
        if upper_seq_only && s.len() >= 10 { return None; }
        return Some(s.to_string());
    }
    // No letters: numeric-like allowed chars
    for ch in s.chars() {
        if !(ch.is_ascii_digit() || matches!(ch, '+'|'-'|'.'|','|'/'|'\\')) {
            return None;
        }
    }
    None
}

fn decode_utf8(s: &[u8]) -> (u32, usize) {
    let c = s[0];
    if c < 0x80 {
        return (c as u32, 1);
    }
    if c & 0xE0 == 0xC0 && s.len() >= 2 {
        return ((((c & 0x1F) as u32) << 6) | ((s[1] & 0x3F) as u32), 2);
    }
    if c & 0xF0 == 0xE0 && s.len() >= 3 {
        return (
            (((c & 0x0F) as u32) << 12) | (((s[1] & 0x3F) as u32) << 6) | ((s[2] & 0x3F) as u32),
            3,
        );
    }
    if c & 0xF8 == 0xF0 && s.len() >= 4 {
        return (
            (((c & 0x07) as u32) << 18)
                | (((s[1] & 0x3F) as u32) << 12)
                | (((s[2] & 0x3F) as u32) << 6)
                | ((s[3] & 0x3F) as u32),
            4,
        );
    }
    (c as u32, 1)
}

fn is_whitespace(cp: u32) -> bool {
    cp == b' ' as u32 || cp == b'\t' as u32 || cp == b'\n' as u32 || cp == b'\r' as u32
}

fn is_ascii_alnum_underscore(cp: u32) -> bool {
    (cp >= b'A' as u32 && cp <= b'Z' as u32)
        || (cp >= b'a' as u32 && cp <= b'z' as u32)
        || (cp >= b'0' as u32 && cp <= b'9' as u32)
        || cp == b'_' as u32
}

fn is_allowed_punct(cp: u32) -> bool {
    cp == b'.' as u32
        || cp == b'\'' as u32
        || cp == b'-' as u32
        || cp == b'/' as u32
        || cp == b'&' as u32
}

fn is_word(cp: u32) -> bool {
    if cp >= 0x80 {
        return true;
    }
    if is_ascii_alnum_underscore(cp) {
        return true;
    }
    if is_allowed_punct(cp) {
        return true;
    }
    false
}

fn is_abbreviation(tok: &str) -> bool {
    // Case-insensitive check using imported list (lowercase)
    crate::bm25::english_abbreviations::contains(tok)
}

pub fn is_stopword(tok: &str) -> bool {
    // Use the comprehensive embedded list
    crate::bm25::english_stop_words::contains(tok)
}

#[cfg(test)]
mod bm25_norm_tests {
    use super::*;

    #[test]
    fn preprocess_dehyphenates_line_breaks_and_controls() {
        let s = "High-\nquality and\tbar\x0C";
        let out = preprocess_bm25(s);
        assert!(out.contains("High"));
        assert!(out.contains("quality"));
        assert!(out.contains("and"));
        assert!(out.contains("bar"));
        assert!(!out.contains("\x0C"));
        assert!(!out.contains("-\n"));
    }

    #[test]
    fn normalize_strips_possessive_ascii_and_unicode() {
        assert_eq!(bm25_normalize_token("doctor's").as_deref(), Some("doctor"));
        assert_eq!(bm25_normalize_token("women’s").as_deref(), Some("women"));
    }

    #[test]
    fn normalize_drops_numeric_and_url_tracking() {
        assert_eq!(bm25_normalize_token("-0.03"), None);
        assert_eq!(bm25_normalize_token("utm_campaign"), None);
    }

    #[test]
    fn normalize_drops_triple_hyphen_and_sequences() {
        assert_eq!(bm25_normalize_token("---ABC"), None);
        let aa = "ACDEFGHIKLMNPQRSTVWY-".repeat(1); // length >= 21
        assert_eq!(bm25_normalize_token(&aa), None);
    }

    #[test]
    fn normalize_keeps_biomedical_patterns() {
        assert_eq!(bm25_normalize_token("il-6").as_deref(), Some("il-6"));
        assert_eq!(bm25_normalize_token("p53").as_deref(), Some("p53"));
        assert_eq!(bm25_normalize_token("covid-19").as_deref(), Some("covid-19"));
    }

    #[test]
    fn normalize_trims_leading_punct() {
        assert_eq!(bm25_normalize_token("&chibnall").as_deref(), Some("chibnall"));
        assert_eq!(bm25_normalize_token("'administrators'").as_deref(), Some("administrators"));
    }
}

fn is_punctuation(tok: &str) -> bool {
    // Use the imported punctuation list
    crate::bm25::english_punctuations::contains(tok)
}

#[cfg(test)]
mod simple_tokenizer_tests {
    use super::*;

    #[test]
    fn basic_tokens() {
        let t = SimpleTokenizer::new();
        assert_eq!(
            t.split("Hello, world!").as_slice(),
            ["Hello", ",", "world", "!"]
        );
        assert_eq!(
            t.split("self-driving and/or R&D").as_slice(),
            ["self-driving", "and/or", "R&D"]
        );
        assert_eq!(
            t.split("End of sentence.").as_slice(),
            ["End", "of", "sentence", "."]
        );
    }

    #[test]
    fn unicode() {
        let t = SimpleTokenizer::new();
        assert_eq!(t.split("café naïve").as_slice(), ["café", "naïve"]);
        assert_eq!(t.split("привет мир").as_slice(), ["привет", "мир"]);
    }

    #[test]
    fn contractions_and_stopwords() {
        let t = SimpleTokenizer::with_options(TokenizerOptions {
            lowercase: true,
            split_contractions: true,
            remove_stopwords: true,
            remove_punctuation: false,
        });
        // With the comprehensive English stopword list, all resulting tokens are stopwords
        let toks = t.split("I can't and won't do it");
        assert_eq!(toks.as_slice(), [] as [&str; 0]);
    }

    #[test]
    fn urls_emails_commas() {
        let t = SimpleTokenizer::new();
        assert_eq!(
            t.split("one,two,three").as_slice(),
            ["one", ",", "two", ",", "three"]
        );
        assert_eq!(
            t.split("contact user@example.com today").as_slice(),
            ["contact", "user", "@", "example.com", "today"]
        );
        assert_eq!(
            t.split("Visit https://example.com/page").as_slice(),
            ["Visit", "https", ":", "//example.com/page"]
        );
    }

    #[test]
    fn quotes_paren_currency() {
        let t = SimpleTokenizer::new();
        assert_eq!(t.split("\"quoted\"").as_slice(), ["\"", "quoted", "\""]);
        assert_eq!(t.split("(example)").as_slice(), ["(", "example", ")"]);
        assert_eq!(
            t.split("$100 €50 £25").as_slice(),
            ["$", "100", "€50", "£25"]
        );
    }

    #[test]
    fn periods_and_abbrev() {
        let t = SimpleTokenizer::new();
        assert_eq!(t.split("...").as_slice(), [".", ".", "."]); // ellipsis split
        // Abbreviations keep period when in-word
        assert_eq!(t.split("Dr. Smith").as_slice(), ["Dr.", "Smith"]);
        // Multi-part: "U.S." -> split trailing period per C++ behavior, known limitation
        assert_eq!(
            t.split("U.S. government").as_slice(),
            ["U.S", ".", "government"]
        );
        // Abbreviation alone at EOL splits period
        assert_eq!(t.split("Dr.").as_slice(), ["Dr", "."]);
    }

    #[test]
    fn whitespace_cases() {
        let t = SimpleTokenizer::new();
        assert_eq!(t.split("").as_slice(), [] as [&str; 0]);
        assert_eq!(t.split("   \t  \n  ").as_slice(), [] as [&str; 0]);
        assert_eq!(
            t.split("multiple   spaces    here").as_slice(),
            ["multiple", "spaces", "here"]
        );
        assert_eq!(
            t.split("line1\nline2\ttab").as_slice(),
            ["line1", "line2", "tab"]
        );
        assert_eq!(
            t.split("  \t  word1 word2  \n  ").as_slice(),
            ["word1", "word2"]
        );
    }

    #[test]
    fn numbers_and_mixed() {
        let t = SimpleTokenizer::new();
        assert_eq!(t.split("123 456.78").as_slice(), ["123", "456.78"]);
        assert_eq!(
            t.split("test123 456test").as_slice(),
            ["test123", "456test"]
        );
    }

    #[test]
    fn operators_percent_dates_time() {
        let t = SimpleTokenizer::new();
        assert_eq!(t.split("2+2=4").as_slice(), ["2", "+", "2", "=", "4"]);
        assert_eq!(
            t.split("100% complete").as_slice(),
            ["100", "%", "complete"]
        );
        assert_eq!(t.split("12/25/2024").as_slice(), ["12/25/2024"]);
        assert_eq!(t.split("2024-12-25").as_slice(), ["2024-12-25"]);
        assert_eq!(t.split("3:30pm").as_slice(), ["3", ":", "30pm"]);
    }

    #[test]
    fn multiple_delimiters_and_apostrophes() {
        let t = SimpleTokenizer::new();
        assert_eq!(
            t.split("word!!!???...").as_slice(),
            ["word", "!", "!", "!", "?", "?", "?", ".", ".", "."]
        );
        // straight and curly apostrophes should both behave as in-word punctuation
        assert_eq!(t.split("it's it's").as_slice(), ["it's", "it's"]);
    }

    #[test]
    fn punctuation_removal() {
        let t = SimpleTokenizer::with_options(TokenizerOptions {
            lowercase: false,
            split_contractions: false,
            remove_stopwords: false,
            remove_punctuation: true,
        });
        // Test that punctuation marks are removed
        assert_eq!(t.split("Hello, world!").as_slice(), ["Hello", "world"]);
        assert_eq!(
            t.split("What? Really! Yes...").as_slice(),
            ["What", "Really", "Yes"]
        );
        // Punctuation within words should still be preserved
        assert_eq!(
            t.split("self-driving and/or R&D").as_slice(),
            ["self-driving", "and/or", "R&D"]
        );
        // Test with mixed punctuation
        assert_eq!(
            t.split("(example) [test] {code}").as_slice(),
            ["example", "test", "code"]
        );
    }

    #[test]
    fn combined_options() {
        let t = SimpleTokenizer::with_options(TokenizerOptions {
            lowercase: true,
            split_contractions: true,
            remove_stopwords: true,
            remove_punctuation: true,
        });
        // With all options enabled
        let toks = t.split("I can't believe it's working!");
        // "I", "can", "not", "believe", "it", "'s", "working" become
        // After stopword removal: "believe", "working" (others are stopwords)
        // After punctuation removal: "believe", "working" (! is removed)
        assert_eq!(toks.as_slice(), ["believe", "working"]);

        // Another test
        let toks2 = t.split("The quick brown fox jumps over the lazy dog.");
        // After stopword removal: "quick", "brown", "fox", "jumps", "lazy", "dog"
        // After punctuation removal: same (. is removed)
        assert_eq!(
            toks2.as_slice(),
            ["quick", "brown", "fox", "jumps", "lazy", "dog"]
        );
    }
}
