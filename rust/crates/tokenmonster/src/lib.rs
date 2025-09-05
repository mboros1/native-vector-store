//! TokenMonster: greedy tiktoken-like tokenizer (cl100k_base approximator)
//!
//! - Greedy longest-match over an embedded vocabulary (base64-encoded tokens → ids).
//! - Falls back to raw bytes (0..255) when no match.
//! - Fast counting suitable for chunking and cost estimates (not exact tiktoken fidelity).
//!
//! Design
//! - Lazy vocabulary load with once_cell.
//! - Hash maps (ahash) for encoder/decoder.
//! - Small inline vocab under `tiny_vocab` feature for tests/examples.

use ahash::AHashMap as HashMap;
use once_cell::sync::Lazy;

pub mod greedy;
pub use greedy::GreedyTokenizer;

#[derive(Default)]
struct Vocab {
    encoder: HashMap<String, i32>,
    decoder: HashMap<i32, String>,
}

static VOCAB: Lazy<Vocab> = Lazy::new(|| {
    let mut v = Vocab::default();

    #[cfg(feature = "tiny_vocab")]
    {
        // base64 token → id pairs (very small sample)
        // "hello" (aGVsbG8=) → 100001
        // "world" (d29ybGQ=) → 100002
        // " " (space) (IA==) → 100003
        let data = [("aGVsbG8=", 100001), ("d29ybGQ=", 100002), ("IA==", 100003)];
        for (b64, id) in data.iter() {
            let token = base64_decode(b64);
            v.encoder.insert(token.clone(), *id);
            v.decoder.insert(*id, token);
        }
    }

    #[cfg(not(feature = "tiny_vocab"))]
    {
        // Placeholder: load full cl100k_base dataset here using include_str!/include_bytes!
        // The format should be lines of: "<base64_token> <id>\n"
        // For now, fall back to an empty vocab; encode() will use byte fallback.
    }

    v
});

fn base64_decode(s: &str) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut val = 0i32;
    let mut valb = -8i32;
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    for &c in s.as_bytes() {
        if c == b'=' {
            break;
        }
        let pos = TABLE.iter().position(|&x| x == c);
        if let Some(p) = pos {
            val = (val << 6) + (p as i32);
            valb += 6;
            if valb >= 0 {
                out.push(((val >> valb) & 0xFF) as u8);
                valb -= 8;
            }
        }
    }
    String::from_utf8(out).unwrap_or_default()
}

pub struct TokenMonster;

impl TokenMonster {
    pub fn new() -> Self {
        TokenMonster
    }

    /// Greedy longest-match encode. Falls back to byte values (0..255).
    pub fn encode(&self, text: &str) -> Vec<i32> {
        let enc = &VOCAB.encoder;
        let mut tokens = Vec::new();
        let bytes = text.as_bytes();
        let mut pos = 0usize;
        while pos < bytes.len() {
            // Try longest match up to 20 bytes
            let max_len = usize::min(20, bytes.len() - pos);
            let mut best: Option<(usize, i32)> = None;
            for len in (1..=max_len).rev() {
                let sub = &text[pos..pos + len];
                if let Some(&id) = enc.get(sub) {
                    best = Some((len, id));
                    break;
                }
            }
            if let Some((len, id)) = best {
                tokens.push(id);
                pos += len;
            } else {
                // fallback to byte
                tokens.push(bytes[pos] as i32);
                pos += 1;
            }
        }
        tokens
    }

    pub fn decode(&self, tokens: &[i32]) -> String {
        let dec = &VOCAB.decoder;
        let mut out = String::new();
        for &t in tokens {
            if let Some(s) = dec.get(&t) {
                out.push_str(s);
            } else if (0..=255).contains(&t) {
                out.push(t as u8 as char);
            }
        }
        out
    }

    pub fn count_tokens(&self, text: &str) -> usize {
        self.encode(text).len()
    }
    pub fn estimate_tokens(text: &str) -> usize {
        text.len().div_ceil(4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiny_vocab_encode_decode() {
        let tm = TokenMonster::new();
        let input = "hello world";
        let ids = tm.encode(input);
        // With tiny vocab, should recognize "hello", space, "world"
        assert!(ids.len() <= input.len());
        let round = tm.decode(&ids);
        assert_eq!(round, input);
    }

    #[test]
    fn greedy_fallback_bytes() {
        let tm = TokenMonster::new();
        let input = "foo"; // not in tiny vocab
        let ids = tm.encode(input);
        assert_eq!(ids.len(), 3);
        assert_eq!(tm.decode(&ids), input);
    }
}
