use anyhow::Result;
use crate::objects::{PdfDoc, PdfValue, resolve};
use crate::streams::get_stream_data_with_filters;

pub fn resolve_contents(doc: &PdfDoc, contents: &PdfValue) -> Result<Vec<u8>> {
    let mut buffers: Vec<u8> = Vec::new();
    match contents {
        PdfValue::Stream { dict, data } => {
            let dec = get_stream_data_with_filters(dict, data.clone())?;
            buffers.extend_from_slice(&dec);
        }
        PdfValue::Array(arr) => {
            for v in arr {
                let vv = resolve(doc, v, 0)?;
                if let PdfValue::Stream{ dict, data } = vv {
                    let dec = get_stream_data_with_filters(&dict, data)?;
                    buffers.extend_from_slice(&dec);
                }
            }
        }
        PdfValue::Ref(obj, gen) => {
            let vv = doc.get_object(*obj, *gen)?;
            if let PdfValue::Stream{ dict, data } = vv {
                let dec = get_stream_data_with_filters(&dict, data)?;
                buffers.extend_from_slice(&dec);
            }
        }
        _ => {}
    }
    Ok(buffers)
}
