use crate::normalize::normalize_page_text;
use crate::objects::{as_dict, as_name, resolve, PdfDoc, PdfValue};
use crate::streams::get_stream_data_with_filters;
use anyhow::Result;
use std::collections::BTreeMap;

#[derive(Clone, Default, serde::Serialize)]
pub struct PageTextDebug {
    pub has_contents: bool,
    pub filters: Vec<String>,
    pub predictors: Vec<i64>,
    pub fonts_total: usize,
    pub fonts_with_tounicode: usize,
    pub decode_ok: bool,
    pub interpret_ok: bool,
    pub notes: Vec<String>,
}

pub fn extract_page_text_with_debug(
    doc: &PdfDoc,
    page: (u32, u16),
) -> (Option<String>, PageTextDebug) {
    let mut dbg = PageTextDebug::default();
    let val = match doc.get_object(page.0, page.1) {
        Ok(v) => v,
        Err(e) => {
            dbg.notes.push(format!("get_object: {}", e));
            return (None, dbg);
        }
    };
    let dict = if let Some(d) = as_dict(&val) {
        d
    } else {
        dbg.notes.push("page not dict".into());
        return (None, dbg);
    };
    // Fonts stats
    if let Some(res) = dict.get("Resources").and_then(|v| as_dict(v)) {
        if let Some(fdict) = res.get("Font").and_then(|v| as_dict(v)) {
            let mut total = 0usize;
            let mut with_tu = 0usize;
            for (_name, fv) in fdict {
                total += 1;
                let rf = resolve(doc, fv, 0).unwrap_or_else(|_| fv.clone());
                if let Some(fd) = as_dict(&rf) {
                    if fd.get("ToUnicode").is_some() {
                        with_tu += 1;
                    }
                }
            }
            dbg.fonts_total = total;
            dbg.fonts_with_tounicode = with_tu;
        }
    }
    let contents = match dict.get("Contents") {
        Some(v) => v,
        None => {
            dbg.has_contents = false;
            return (Some(String::new()), dbg);
        }
    };
    dbg.has_contents = true;
    let mut buffers: Vec<u8> = Vec::new();
    let mut filters = Vec::new();
    let mut preds = Vec::new();
    let mut decode_ok = true;
    let decode_one = |vv: PdfValue,
                      filters: &mut Vec<String>,
                      preds: &mut Vec<i64>,
                      buffers: &mut Vec<u8>|
     -> Result<()> {
        if let PdfValue::Stream { dict: sdict, data } = vv {
            if let Some(fv) = sdict.get("Filter") {
                match fv {
                    PdfValue::Name(n) => {
                        filters.push(n.to_string());
                    }
                    PdfValue::Array(arr) => {
                        for f in arr {
                            if let Some(n) = as_name(f) {
                                filters.push(n.to_string());
                            }
                        }
                    }
                    _ => {}
                }
            }
            if let Some(dp) = sdict.get("DecodeParms") {
                if let Some(p) = get_predictor(dp) {
                    preds.push(p);
                }
            }
            let dec = get_stream_data_with_filters(&sdict, data)?;
            buffers.extend_from_slice(&dec);
        }
        Ok(())
    };
    match contents {
        PdfValue::Stream { dict, data } => {
            let vv = PdfValue::Stream {
                dict: dict.clone(),
                data: data.clone(),
            };
            if let Err(e) = decode_one(vv, &mut filters, &mut preds, &mut buffers) {
                dbg.notes.push(format!("decode: {}", e));
                decode_ok = false;
            }
        }
        PdfValue::Array(arr) => {
            for v in arr {
                let vv = match resolve(doc, v, 0) {
                    Ok(x) => x,
                    Err(e) => {
                        dbg.notes.push(format!("resolve stream: {}", e));
                        continue;
                    }
                };
                if let Err(e) = decode_one(vv, &mut filters, &mut preds, &mut buffers) {
                    dbg.notes.push(format!("decode arr: {}", e));
                    decode_ok = false;
                }
            }
        }
        PdfValue::Ref(obj, gen) => {
            let vv = match doc.get_object(*obj, *gen) {
                Ok(x) => x,
                Err(e) => {
                    dbg.notes.push(format!("get stream: {}", e));
                    return (None, dbg);
                }
            };
            if let Err(e) = decode_one(vv, &mut filters, &mut preds, &mut buffers) {
                dbg.notes.push(format!("decode ref: {}", e));
                decode_ok = false;
            }
        }
        _ => {}
    }
    dbg.filters = filters;
    dbg.predictors = preds;
    dbg.decode_ok = decode_ok;
    if !decode_ok {
        return (None, dbg);
    }
    match crate::interpret::interpret_text_with_resources(
        doc,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &buffers,
    ) {
        Ok(txt) => {
            dbg.interpret_ok = true;
            (Some(normalize_page_text(&txt)), dbg)
        }
        Err(e) => {
            dbg.notes.push(format!("interpret: {}", e));
            (None, dbg)
        }
    }
}

fn get_predictor(dp: &PdfValue) -> Option<i64> {
    match dp {
        PdfValue::Dict(d) => d.get("Predictor").and_then(|v| match v {
            PdfValue::Int(i) => Some(*i),
            PdfValue::Real(f) => Some(*f as i64),
            _ => None,
        }),
        PdfValue::Array(arr) => {
            for v in arr {
                if let Some(i) = get_predictor(v) {
                    return Some(i);
                }
            }
            None
        }
        _ => None,
    }
}
