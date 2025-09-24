use crate::objects::{is_alpha, parse_name, parse_number, parse_string, skip_ws};
use anyhow::Result;

#[derive(Debug)]
pub enum Tok {
    Op(String),
    Name(String),
    Str(Vec<u8>),
    Num(f64),
    ArrStart,
    ArrEnd,
}

pub struct Tokenizer<'a> {
    b: &'a [u8],
    i: usize,
    peeked_op: Option<String>,
}

impl<'a> Tokenizer<'a> {
    pub fn new(b: &'a [u8]) -> Self {
        Self {
            b,
            i: 0,
            peeked_op: None,
        }
    }
    fn skip_ws(&mut self) {
        self.i = skip_ws(self.b, self.i);
    }
    pub fn next(&mut self) -> Result<Option<Tok>> {
        self.skip_ws();
        if self.i >= self.b.len() {
            return Ok(None);
        }
        let c = self.b[self.i];
        Ok(Some(match c {
            b'(' => {
                let (s, j) = parse_string(self.b, self.i + 1)?;
                self.i = j;
                Tok::Str(s)
            }
            b'[' => {
                self.i += 1;
                Tok::ArrStart
            }
            b']' => {
                self.i += 1;
                Tok::ArrEnd
            }
            b'/' => {
                let (n, j) = parse_name(self.b, self.i + 1)?;
                self.i = j;
                Tok::Name(n)
            }
            b'\'' => {
                self.i += 1;
                Tok::Op("'".to_string())
            }
            b'"' => {
                self.i += 1;
                Tok::Op("\"".to_string())
            }
            b'+' | b'-' | b'.' | b'0'..=b'9' => match parse_number(self.b, self.i) {
                Ok((n, j, _)) => {
                    self.i = j;
                    Tok::Num(n as f64)
                }
                Err(_) => {
                    self.i += 1;
                    return self.next();
                }
            },
            _ => {
                let start = self.i;
                let mut j = start;
                while j < self.b.len() && is_alpha(self.b[j]) {
                    j += 1;
                }
                if j > start {
                    let op = String::from_utf8_lossy(&self.b[start..j]).to_string();
                    self.i = j;
                    Tok::Op(op)
                } else {
                    self.i += 1;
                    return self.next();
                }
            }
        }))
    }
    pub fn peek_op(&mut self) -> Result<Option<String>> {
        let save = self.i;
        let tok = self.next()?;
        self.i = save;
        if let Some(Tok::Op(op)) = tok {
            self.peeked_op = Some(op.clone());
            Ok(Some(op))
        } else {
            Ok(None)
        }
    }
    pub fn consume_op(&mut self) {
        if let Some(_op) = self.peeked_op.take() {
            self.skip_ws();
            let _ = self.next();
        }
    }
    pub fn skip_inline_image_after_id(&mut self) {
        if self.i < self.b.len() && crate::objects::is_ws(self.b[self.i]) {
            self.i += 1;
        }
        while self.i + 1 < self.b.len() {
            let prev = if self.i == 0 {
                b' '
            } else {
                self.b[self.i - 1]
            };
            if self.b[self.i] == b'E' && self.b[self.i + 1] == b'I' {
                let next = if self.i + 2 < self.b.len() {
                    self.b[self.i + 2]
                } else {
                    b' '
                };
                if crate::objects::is_ws(prev) && (crate::objects::is_ws(next) || !is_alpha(next)) {
                    self.i += 2;
                    break;
                }
            }
            self.i += 1;
        }
    }
}
