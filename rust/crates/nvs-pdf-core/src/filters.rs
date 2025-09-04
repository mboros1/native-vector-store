use anyhow::{anyhow, Result};

// Flate via miniz_oxide
pub fn decode_flate(input: &[u8]) -> Result<Vec<u8>> {
    use miniz_oxide::inflate::decompress_to_vec_zlib;
    // Try zlib first, then raw deflate
    match decompress_to_vec_zlib(input) {
        Ok(v) => Ok(v),
        Err(_) => {
            use miniz_oxide::inflate::decompress_to_vec;
            decompress_to_vec(input).map_err(|e| anyhow!("flate raw error: {:?}", e))
        }
    }
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

