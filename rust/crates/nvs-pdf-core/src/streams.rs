use crate::objects::PdfValue;
use anyhow::{anyhow, Result};
use std::collections::BTreeMap;
use std::time::Instant;
#[allow(dead_code)]
#[derive(Debug, Copy, Clone)]
pub enum FilterKind {
    Flate,
    ASCII85,
    ASCIIHex,
    RunLength,
    LZW,
    Image,
    Other,
}
#[allow(dead_code)]
#[derive(Debug, Copy, Clone)]
pub enum PredictorKind {
    None,
    TIFF2,
    PNG,
}

pub fn get_stream_data_with_filters(
    dict: &BTreeMap<String, PdfValue>,
    data: Vec<u8>,
) -> Result<Vec<u8>> {
    let mut out = data;
    if let Some(filter) = dict.get("Filter") {
        // DecodeParms may be a dict or array aligned with Filter
        let decode_parms = dict.get("DecodeParms");
        match filter {
            PdfValue::Name(n) => {
                out = apply_filter_with_params(n, out, decode_parms, 0)?;
            }
            PdfValue::Array(arr) => {
                let mut cur = out;
                for (idx, f) in arr.iter().enumerate() {
                    if let Some(n) = as_name(f) {
                        cur = apply_filter_with_params(n, cur, decode_parms, idx)?;
                    } else {
                        return Err(anyhow!("filter name expected"));
                    }
                }
                out = cur;
            }
            _ => {}
        }
    }
    Ok(out)
}

fn apply_filter_with_params(
    name: &str,
    data: Vec<u8>,
    decode_parms: Option<&PdfValue>,
    index: usize,
) -> Result<Vec<u8>> {
    let tdec = Instant::now();
    let mut decoded = match name {
        "FlateDecode" => match crate::filters::decode_flate(&data) {
            Ok(v) => v,
            Err(e) => crate::filters::decode_flate_tolerant(&data).map_err(|e2| {
                anyhow!(
                    "filter=FlateDecode in_len={} err={} tolerant={} ",
                    data.len(),
                    e,
                    e2
                )
            })?,
        },
        "ASCII85Decode" => return crate::filters::decode_ascii85(&data),
        "ASCIIHexDecode" => return crate::filters::decode_asciihex(&data),
        "RunLengthDecode" => return crate::filters::decode_runlength(&data),
        "LZWDecode" => {
            let mut early_change = true; // default per PDF spec is 1
            if let Some(params) = decode_parms {
                if let Some(dp) = get_decode_params_for_index(params, index) {
                    if let Some(ec) = dp.get("EarlyChange").and_then(as_int) {
                        early_change = ec != 0;
                    }
                }
            }
            match crate::filters::decode_lzw_with_params(&data, early_change) {
                Ok(v) => v,
                Err(e1) => match crate::filters::decode_lzw_with_params(&data, !early_change) {
                    Ok(v2) => v2,
                    Err(e2) => crate::filters::decode_lzw_tolerant(&data).map_err(|e3| {
                        anyhow!(
                            "filter=LZWDecode in_len={} EC1={} EC0={} tolerant={} ",
                            data.len(),
                            e1,
                            e2,
                            e3
                        )
                    })?,
                },
            }
        }
        // Image-only filters: pass through for text streams
        "DCTDecode" | "JPXDecode" | "JBIG2Decode" | "CCITTFaxDecode" => data,
        _other => data, // be permissive: unknown filter → pass-through
    };

    // Apply Predictor if specified (PNG/TIFF predictors) — only meaningful for Flate/LZW
    if matches!(name, "FlateDecode" | "LZWDecode") {
        if let Some(params) = decode_parms {
            if let Some(dp) = get_decode_params_for_index(params, index) {
                let predictor = dp.get("Predictor").and_then(as_int).unwrap_or(1) as u32;
                if predictor > 1 {
                    let tp = Instant::now();
                    let colors = dp.get("Colors").and_then(as_int).unwrap_or(1) as u32;
                    let bpc = dp.get("BitsPerComponent").and_then(as_int).unwrap_or(8) as u32;
                    let columns = dp.get("Columns").and_then(as_int).unwrap_or(1) as u32;
                    decoded = apply_predictor(&decoded, predictor, colors, bpc, columns)
                        .map_err(|e| anyhow!("predictor_apply failed pred={} cols={} colors={} bpc={} in_len={} err={}", predictor, columns, colors, bpc, decoded.len(), e))?;
                    crate::stats::add_decode_duration(tp.elapsed().as_nanos() as u128);
                }
            }
        }
    }
    crate::stats::add_decode_duration(tdec.elapsed().as_nanos() as u128);
    Ok(decoded)
}

fn as_name(v: &PdfValue) -> Option<&str> {
    if let PdfValue::Name(ref s) = v {
        Some(s.as_str())
    } else {
        None
    }
}

fn as_int(v: &PdfValue) -> Option<i64> {
    match v {
        PdfValue::Int(i) => Some(*i),
        PdfValue::Real(f) => Some(*f as i64),
        _ => None,
    }
}

fn get_decode_params_for_index(
    decode_parms: &PdfValue,
    index: usize,
) -> Option<&BTreeMap<String, PdfValue>> {
    match decode_parms {
        PdfValue::Dict(ref d) => Some(d),
        PdfValue::Array(ref arr) => {
            if index < arr.len() {
                if let PdfValue::Dict(ref d) = arr[index] {
                    return Some(d);
                }
            }
            None
        }
        _ => None,
    }
}

fn apply_predictor(
    data: &[u8],
    predictor: u32,
    colors: u32,
    bpc: u32,
    columns: u32,
) -> Result<Vec<u8>> {
    if predictor == 2 {
        // TIFF predictor (horizontal differencing)
        let bpp = ((colors * bpc + 7) / 8) as usize;
        if bpp == 0 {
            return Ok(data.to_vec());
        }
        let mut out = data.to_vec();
        let row_len = (columns as usize) * ((colors * bpc) as usize) / 8;
        let mut i = 0usize;
        while i + row_len <= out.len() {
            let row_start = i;
            for j in bpp..row_len {
                let prev = out[row_start + j - bpp];
                let val = out[row_start + j].wrapping_add(prev);
                out[row_start + j] = val;
            }
            i += row_len;
        }
        return Ok(out);
    }
    // PNG predictor (10-15): each row starts with 1-byte filter type
    if predictor >= 10 {
        let row_bytes = ((columns * colors * bpc + 7) / 8) as usize;
        if row_bytes == 0 {
            return Ok(data.to_vec());
        }
        let mut out = Vec::with_capacity(data.len());
        let mut i = 0usize;
        let bpp = ((colors * bpc + 7) / 8) as usize;
        while i < data.len() {
            if i >= data.len() {
                break;
            }
            let filter = data[i];
            i += 1;
            if i + row_bytes > data.len() {
                break;
            }
            let row = &data[i..i + row_bytes];
            let mut dst = vec![0u8; row_bytes];
            match filter {
                0 => {
                    dst.copy_from_slice(row);
                }
                1 => {
                    // Sub
                    for x in 0..row_bytes {
                        let left = if x >= bpp { dst[x - bpp] } else { 0 };
                        dst[x] = row[x].wrapping_add(left);
                    }
                }
                2 => {
                    // Up
                    // Previous row in output
                    let prev_offset = if out.len() >= row_bytes {
                        out.len() - row_bytes
                    } else {
                        usize::MAX
                    };
                    for x in 0..row_bytes {
                        let up = if prev_offset != usize::MAX {
                            out[prev_offset + x]
                        } else {
                            0
                        };
                        dst[x] = row[x].wrapping_add(up);
                    }
                }
                3 => {
                    // Average
                    let prev_offset = if out.len() >= row_bytes {
                        out.len() - row_bytes
                    } else {
                        usize::MAX
                    };
                    for x in 0..row_bytes {
                        let left = if x >= bpp { dst[x - bpp] } else { 0 };
                        let up = if prev_offset != usize::MAX {
                            out[prev_offset + x]
                        } else {
                            0
                        };
                        let avg = ((left as u16 + up as u16) / 2) as u8;
                        dst[x] = row[x].wrapping_add(avg);
                    }
                }
                4 => {
                    // Paeth
                    let prev_offset = if out.len() >= row_bytes {
                        out.len() - row_bytes
                    } else {
                        usize::MAX
                    };
                    for x in 0..row_bytes {
                        let a = if x >= bpp { dst[x - bpp] } else { 0 };
                        let b = if prev_offset != usize::MAX {
                            out[prev_offset + x]
                        } else {
                            0
                        };
                        let c = if prev_offset != usize::MAX && x >= bpp {
                            out[prev_offset + x - bpp]
                        } else {
                            0
                        };
                        dst[x] = row[x].wrapping_add(paeth(a, b, c));
                    }
                }
                _ => {
                    dst.copy_from_slice(row);
                }
            }
            out.extend_from_slice(&dst);
            i += row_bytes;
        }
        return Ok(out);
    }
    Ok(data.to_vec())
}

fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let a = a as i32;
    let b = b as i32;
    let c = c as i32;
    let p = a + b - c;
    let pa = (p - a).abs();
    let pb = (p - b).abs();
    let pc = (p - c).abs();
    if pa <= pb && pa <= pc {
        a as u8
    } else if pb <= pc {
        b as u8
    } else {
        c as u8
    }
}

#[cfg(test)]
mod tests {
    use super::apply_predictor;

    #[test]
    fn tiff_predictor_horizontal_differencing() {
        // Original row: [1,2,3,4]; encoded with horizontal differencing predictor 2 becomes [1,1,1,1]
        let encoded = vec![1u8, 1, 1, 1];
        let decoded = apply_predictor(&encoded, 2, 1, 8, 4).expect("tiff predictor");
        assert_eq!(decoded, vec![1u8, 2, 3, 4]);
    }

    #[test]
    fn png_predictor_sub() {
        // PNG Sub filter (1): each byte is difference to left. For original [1,2,3,4], encoded row is [filter=1, 1,1,1,1]
        let encoded = vec![1u8, 1, 1, 1, 1];
        let decoded = apply_predictor(&encoded, 15, 1, 8, 4).expect("png sub predictor");
        assert_eq!(decoded, vec![1u8, 2, 3, 4]);
    }

    #[test]
    fn png_predictor_none() {
        let encoded = vec![0u8, 9, 8, 7, 6];
        let decoded = apply_predictor(&encoded, 15, 1, 8, 4).expect("png none predictor");
        assert_eq!(decoded, vec![9u8, 8, 7, 6]);
    }
}
