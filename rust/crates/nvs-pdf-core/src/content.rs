use crate::interpret::interpret_contents;
use crate::metrics::MetricKey;
use crate::normalize::normalize_page_text;
use crate::objects::{as_dict, resolve, PdfDoc, PdfValue};
use crate::resources::collect_resources;
use crate::streams::get_stream_data_with_filters;
use anyhow::Result;
use std::time::Instant;

// Resolve and decode the page's Contents into a single concatenated buffer
fn resolve_contents(doc: &PdfDoc, contents: &PdfValue) -> Result<Vec<u8>> {
    let mut buffers: Vec<u8> = Vec::new();
    match contents {
        PdfValue::Stream { dict, data } => {
            let dec = get_stream_data_with_filters(dict, data.clone())?;
            buffers.extend_from_slice(&dec);
        }
        PdfValue::Array(arr) => {
            for v in arr {
                let vv = resolve(doc, v, 0)?;
                if let PdfValue::Stream { dict, data } = vv {
                    let dec = get_stream_data_with_filters(&dict, data)?;
                    buffers.extend_from_slice(&dec);
                }
            }
        }
        PdfValue::Ref(obj, gen) => {
            let vv = doc.get_object(*obj, *gen)?;
            if let PdfValue::Stream { dict, data } = vv {
                let dec = get_stream_data_with_filters(&dict, data)?;
                buffers.extend_from_slice(&dec);
            }
        }
        _ => {}
    }
    Ok(buffers)
}

// Extract, interpret, normalize a page's text
pub fn extract_page_text(doc: &PdfDoc, page: (u32, u16)) -> Result<String> {
    let t_page = Instant::now();
    let val = doc.get_object(page.0, page.1)?;
    let dict = as_dict(&val).ok_or_else(|| anyhow::anyhow!("page not dict"))?;

    // Collect resources (fonts, xobjects)
    let res = crate::measure!(MetricKey::Resources, { collect_resources(doc, dict) })?;

    // Resolve + decode Contents (handles Array/Ref/Stream)
    let contents = match dict.get("Contents") {
        Some(v) => v,
        None => return Ok(String::new()), // pages without content → empty text
    };
    let buffers = crate::measure!(MetricKey::Streams, { resolve_contents(doc, contents) })?;

    // Interpret to text using the shared interpreter
    let text = crate::measure!(MetricKey::Interpret, {
        interpret_contents(doc, &res, &buffers)
    })?;

    // Normalize whitespace/newlines
    let norm = crate::measure!(MetricKey::Normalize, { normalize_page_text(&text) });
    crate::stats::add_page_total_duration(t_page.elapsed().as_nanos() as u128);
    Ok(norm)
}

