use crate::objects::{as_dict, as_name, resolve, PdfDoc, PdfValue};
use crate::resources::FontInfo;
use crate::streams::get_stream_data_with_filters;
use crate::fonts::parse_tounicode_cmap;
use std::collections::BTreeMap;
use std::time::Instant;

// Resolve a Form XObject, returning its decoded stream and merged XObject/Font resources.
pub fn resolve_form_and_merge_resources(
    doc: &PdfDoc,
    dict: &BTreeMap<String, PdfValue>,
    data: Vec<u8>,
    parent_xobjs: &BTreeMap<String, PdfValue>,
    parent_fonts: &BTreeMap<String, FontInfo>,
) -> Option<(Vec<u8>, BTreeMap<String, PdfValue>, BTreeMap<String, FontInfo>)> {
    // Verify subtype
    if dict.get("Subtype").and_then(|v| as_name(v)) != Some("Form") { return None; }
    let dec = get_stream_data_with_filters(dict, data).ok()?;
    let mut sub_xobjs = parent_xobjs.clone();
    let mut sub_fonts = parent_fonts.clone();
    if let Some(res) = dict.get("Resources").and_then(|v| as_dict(v)) {
        if let Some(xd) = res.get("XObject").and_then(|v| as_dict(v)) {
            for (k, v) in xd {
                let rv = resolve(doc, v, 0).unwrap_or_else(|_| v.clone());
                sub_xobjs.insert(k.clone(), rv);
            }
        }
        if let Some(fdict) = res.get("Font").and_then(|v| as_dict(v)) {
            for (name, fv) in fdict {
                let rf = resolve(doc, fv, 0).unwrap_or_else(|_| fv.clone());
                if let Some(fd) = as_dict(&rf) {
                    let mut fi = FontInfo::default();
                    if let Some(enc_name) = fd.get("Encoding").and_then(|v| as_name(v)).map(|s| s.to_string()) {
                        fi.base_encoding = Some(enc_name);
                    }
                    if let Some(tu) = fd.get("ToUnicode") {
                        let rf2 = resolve(doc, tu, 0).unwrap_or_else(|_| tu.clone());
                        if let PdfValue::Stream { dict: sdict, data } = rf2 {
                            if let Ok(dec) = get_stream_data_with_filters(&sdict, data) {
                                let tf = Instant::now();
                                fi.to_unicode = Some(parse_tounicode_cmap(&dec));
                                crate::stats::add_fonts_duration(tf.elapsed().as_nanos() as u128);
                            }
                        }
                    }
                    sub_fonts.insert(name.clone(), fi);
                }
            }
        }
    }
    Some((dec, sub_xobjs, sub_fonts))
}

