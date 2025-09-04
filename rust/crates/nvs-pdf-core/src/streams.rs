use crate::objects::PdfValue;
use anyhow::{anyhow, Result};
use std::collections::BTreeMap;

pub fn get_stream_data_with_filters(dict: &BTreeMap<String,PdfValue>, data: Vec<u8>) -> Result<Vec<u8>> {
    let mut out = data;
    if let Some(filter) = dict.get("Filter") {
        match filter {
            PdfValue::Name(n) => { out = apply_filter(n, out)?; }
            PdfValue::Array(arr) => {
                let mut cur = out;
                for f in arr { if let Some(n) = as_name(f) { cur = apply_filter(n, cur)?; } else { return Err(anyhow!("filter name expected")); } }
                out = cur;
            }
            _ => {}
        }
    }
    Ok(out)
}

fn apply_filter(name: &str, data: Vec<u8>) -> Result<Vec<u8>> {
    match name {
        "FlateDecode" => crate::filters::decode_flate(&data),
        "ASCII85Decode" => crate::filters::decode_ascii85(&data),
        "ASCIIHexDecode" => crate::filters::decode_asciihex(&data),
        "RunLengthDecode" => crate::filters::decode_runlength(&data),
        other => Err(anyhow!("unsupported filter {}", other)),
    }
}

fn as_name(v: &PdfValue) -> Option<&str> { if let PdfValue::Name(ref s) = v { Some(s.as_str()) } else { None } }

