use anyhow::{anyhow, Result};

// Flate via miniz_oxide
pub fn decode_flate(input: &[u8]) -> Result<Vec<u8>> {
    use miniz_oxide::inflate::decompress_to_vec_zlib;
    // Try zlib first, then raw deflate
    match decompress_to_vec_zlib(input) {
        Ok(v) => Ok(v),
        Err(_) => {
            use miniz_oxide::inflate::decompress_to_vec;
            decompress_to_vec(input).map_err(|e| {
                let out_len = e.output.len();
                let preview: Vec<u8> = e.output.iter().copied().take(16).collect();
                anyhow!(
                    "flate raw error: status={:?}, in_len={}, out_len={}, out_preview={:02X?}",
                    e.status, input.len(), out_len, preview
                )
            })
        }
    }
}

pub fn decode_flate_tolerant(input: &[u8]) -> Result<Vec<u8>> {
    use miniz_oxide::inflate::{decompress_to_vec, decompress_to_vec_zlib};
    // First try original input
    if let Ok(v) = decompress_to_vec_zlib(input) { return Ok(v); }
    if let Ok(v) = decompress_to_vec(input) { return Ok(v); }
    // If input looks like zlib, try stripping zlib header (and optional dict) then raw inflate
    if input.len() > 6 {
        let cmf = input[0]; let flg = input[1];
        let zlib_header_ok = (cmf & 0x0F) == 8 && ((u16::from(cmf) << 8 | u16::from(flg)) % 31 == 0);
        if zlib_header_ok {
            let fdict = (flg & 0x20) != 0; // preset dictionary flag
            let mut start = 2usize;
            if fdict { start += 4; }
            if start < input.len() {
                let slice = &input[start..];
                if let Ok(v) = decompress_to_vec(slice) { return Ok(v); }
            }
        }
    }
    // Try trimming trailing bytes up to 512 and retry both zlib/raw
    let is_pad = |b: u8| b == 0x00 || b == 0x0A || b == 0x0D || b == b' ';
    let mut end = input.len();
    let mut trimmed = 0usize;
    while trimmed < 512 && end > 0 {
        if is_pad(input[end-1]) { end -= 1; trimmed += 1; } else { break; }
    }
    while trimmed < 512 && end > 8 {
        let slice = &input[..end];
        if let Ok(v) = decompress_to_vec_zlib(slice) { return Ok(v); }
        if let Ok(v) = decompress_to_vec(slice) { return Ok(v); }
        end -= 1; trimmed += 1;
    }
    anyhow::bail!("flate tolerant failed (in_len={}, trimmed_up_to={})", input.len(), trimmed)
}

// ASCIIHexDecode
pub fn decode_asciihex(input: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(input.len()/2);
    let mut nibbles: Vec<u8> = Vec::new();
    for &b in input {
        match b {
            b'>' => break, // EOD
            b'0'..=b'9' => nibbles.push(b - b'0'),
            b'a'..=b'f' => nibbles.push(10 + (b - b'a')),
            b'A'..=b'F' => nibbles.push(10 + (b - b'A')),
            _ => {}, // ignore whitespace and other
        }
    }
    let mut i = 0;
    while i + 1 < nibbles.len() {
        out.push((nibbles[i] << 4) | nibbles[i+1]);
        i += 2;
    }
    if i < nibbles.len() { // odd nibble padded with 0
        out.push(nibbles[i] << 4);
    }
    Ok(out)
}

// ASCII85Decode
pub fn decode_ascii85(input: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity((input.len() * 4) / 5);
    let mut tuple = [0u32; 5];
    let mut tlen = 0usize;
    let mut i = 0;
    while i < input.len() {
        let b = input[i]; i += 1;
        match b {
            b'~' => break, // EOD marker (expect '>')
            b'z' => {
                if tlen != 0 { return Err(anyhow!("ascii85: 'z' inside tuple")); }
                out.extend_from_slice(&[0,0,0,0]);
            }
            b'!'..=b'u' => {
                tuple[tlen] = (b - b'!') as u32; tlen += 1;
                if tlen == 5 {
                    let mut acc = 0u32;
                    for &v in &tuple { acc = acc * 85 + v; }
                    out.extend_from_slice(&acc.to_be_bytes());
                    tlen = 0;
                }
            }
            _ => {}, // ignore whitespace
        }
    }
    if tlen > 0 {
        for k in tlen..5 { tuple[k] = 84; } // pad with 'u'
        let mut acc = 0u32; for &v in &tuple { acc = acc * 85 + v; }
        let bytes = acc.to_be_bytes();
        // emit tlen-1 bytes
        let emit = tlen.saturating_sub(1);
        out.extend_from_slice(&bytes[..emit]);
    }
    Ok(out)
}

// RunLengthDecode
pub fn decode_runlength(input: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(input.len());
    let mut i = 0usize;
    while i < input.len() {
        let b = input[i]; i += 1;
        if b == 128 { break; } // EOD
        if b < 128 {
            let n = (b as usize) + 1;
            if i + n > input.len() { return Err(anyhow!("runlength: literal overrun")); }
            out.extend_from_slice(&input[i..i+n]);
            i += n;
        } else { // b in 129..=255
            let n = (257 - b as usize);
            if i >= input.len() { return Err(anyhow!("runlength: repeat missing byte")); }
            let byte = input[i]; i += 1;
            for _ in 0..n { out.push(byte); }
        }
    }
    Ok(out)
}

// LZWDecode (PDF variant). Minimal implementation sufficient for text streams.
// Assumptions:
// - Initial code size 9 bits, Clear=256, EOD=257, first free=258
// - EarlyChange = 1 (PDF default). We ignore DecodeParms for now.
pub fn decode_lzw(input: &[u8]) -> Result<Vec<u8>> { decode_lzw_with_params(input, true) }

pub fn decode_lzw_with_params(input: &[u8], early_change: bool) -> Result<Vec<u8>> {
    struct BitReader<'a> { b: &'a [u8], i: usize, bit: u8 }
    impl<'a> BitReader<'a> { fn new(b: &'a [u8]) -> Self { Self { b, i: 0, bit: 0 } }
        fn read_bits(&mut self, n: u8) -> Option<u32> {
            let mut need = n; let mut out: u32 = 0; let mut shift = 0;
            while need > 0 {
                if self.i >= self.b.len() { return None; }
                let mut byte = self.b[self.i];
                let avail = 8 - self.bit;
                let take = avail.min(need);
                let mask = ((1u16 << take) - 1) as u8;
                let chunk = (byte >> self.bit) & mask;
                out |= (chunk as u32) << shift;
                shift += take as u32;
                self.bit += take;
                if self.bit == 8 { self.i += 1; self.bit = 0; }
                need -= take;
            }
            Some(out)
        }
    }
    let mut br = BitReader::new(input);
    let clear: u16 = 256; let eod: u16 = 257;
    let mut code_size: u8 = 9;
    let mut next_code: u16 = 258;
    let mut dict: Vec<Vec<u8>> = Vec::with_capacity(4096);
    dict.resize(258, Vec::new());
    for i in 0..256 { dict[i as usize] = vec![i as u8]; }
    dict[256] = Vec::new(); dict[257] = Vec::new();
    let mut prev: Option<Vec<u8>> = None;
    let mut out: Vec<u8> = Vec::new();
    while let Some(c) = br.read_bits(code_size) {
        let code = c as u16;
        if code == clear {
            // reset
            code_size = 9; next_code = 258;
            dict.truncate(258);
            prev = None;
            continue;
        }
        if code == eod { break; }
        let mut entry: Vec<u8> = if (code as usize) < dict.len() && !dict[code as usize].is_empty() {
            dict[code as usize].clone()
        } else if code == next_code && prev.is_some() {
            let mut t = prev.as_ref().unwrap().clone();
            let k = t[0]; t.push(k); t
        } else {
            return Err(anyhow!("lzw: invalid code {}", code));
        };
        out.extend_from_slice(&entry);
        if let Some(ref p) = prev {
            let mut new_entry = p.clone();
            new_entry.push(entry[0]);
            if (dict.len() as u16) == next_code { dict.push(new_entry); } else { dict.resize(next_code as usize + 1, Vec::new()); dict[next_code as usize] = new_entry; }
            next_code = next_code.saturating_add(1);
            // Increase code size depending on EarlyChange (PDF DecodeParms EarlyChange)
            let threshold = if early_change { (1u16 << code_size) - 1 } else { (1u16 << code_size) };
            if next_code == threshold { code_size = (code_size + 1).min(12); }
        }
        prev = Some(entry);
        if code_size > 12 { break; }
    }
    Ok(out)
}

// Tolerant LZW: attempts to continue across invalid codes by resetting dictionary,
// and tries both EarlyChange variants internally if needed.
pub fn decode_lzw_tolerant(input: &[u8]) -> Result<Vec<u8>> {
    // try EC=1
    match decode_lzw_with_params(input, true) {
        Ok(v) => return Ok(v),
        Err(_) => {}
    }
    // try EC=0
    match decode_lzw_with_params(input, false) {
        Ok(v) => return Ok(v),
        Err(_) => {}
    }
    // Fallback: best-effort streaming decode with resets
    struct BitReader<'a> { b: &'a [u8], i: usize, bit: u8 }
    impl<'a> BitReader<'a> { fn new(b: &'a [u8]) -> Self { Self { b, i: 0, bit: 0 } }
        fn read_bits(&mut self, n: u8) -> Option<u32> {
            let mut need = n; let mut out: u32 = 0; let mut shift = 0;
            while need > 0 {
                if self.i >= self.b.len() { return None; }
                let byte = self.b[self.i];
                let avail = 8 - self.bit;
                let take = avail.min(need);
                let mask = ((1u16 << take) - 1) as u8;
                let chunk = (byte >> self.bit) & mask;
                out |= (chunk as u32) << shift;
                shift += take as u32;
                self.bit += take;
                if self.bit == 8 { self.i += 1; self.bit = 0; }
                need -= take;
            }
            Some(out)
        }
    }
    let clear: u16 = 256; let eod: u16 = 257;
    let mut decoded = Vec::new();
    for &ec_flag in &[true, false] {
        let mut br = BitReader::new(input);
        let mut code_size: u8 = 9; let mut next_code: u16 = 258;
        let mut dict: Vec<Vec<u8>> = Vec::with_capacity(4096);
        dict.resize(258, Vec::new()); for i in 0..256 { dict[i as usize] = vec![i as u8]; }
        dict[256] = Vec::new(); dict[257] = Vec::new();
        let mut prev: Option<Vec<u8>> = None;
        let mut out = Vec::new();
        while let Some(c) = br.read_bits(code_size) {
            let code = c as u16;
            if code == clear {
                code_size = 9; next_code = 258;
                dict.truncate(258);
                prev = None; continue;
            }
            if code == eod { break; }
            let entry: Vec<u8> = if (code as usize) < dict.len() && !dict[code as usize].is_empty() {
                dict[code as usize].clone()
            } else if code == next_code && prev.is_some() {
                let mut t = prev.as_ref().unwrap().clone(); let k = t[0]; t.push(k); t
            } else {
                // invalid code: reset dictionary and continue
                code_size = 9; next_code = 258;
                dict.truncate(258); prev = None; continue;
            };
            out.extend_from_slice(&entry);
            if let Some(ref p) = prev {
                let mut new_entry = p.clone(); new_entry.push(entry[0]);
                if (dict.len() as u16) == next_code { dict.push(new_entry); } else { dict.resize(next_code as usize + 1, Vec::new()); dict[next_code as usize] = new_entry; }
                next_code = next_code.saturating_add(1);
                let threshold = if ec_flag { (1u16 << code_size) - 1 } else { 1u16 << code_size };
                if next_code == threshold { code_size = (code_size + 1).min(12); }
            }
            prev = Some(entry);
        }
        if !out.is_empty() { decoded = out; break; }
    }
    if decoded.is_empty() { anyhow::bail!("lzw tolerant decode failed") } else { Ok(decoded) }
}
