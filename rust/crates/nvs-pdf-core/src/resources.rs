use crate::fonts::{parse_tounicode_cmap, ToUnicodeMap};
use crate::metrics::MetricKey;
use crate::objects::{as_dict, as_name, resolve, PdfDoc, PdfValue};
use crate::streams::get_stream_data_with_filters;
use anyhow::Result;
use std::collections::BTreeMap;

#[derive(Clone, Default)]
pub struct FontInfo {
    pub to_unicode: Option<ToUnicodeMap>,
    pub base_encoding: Option<String>,
}

#[derive(Clone, Default)]
pub struct PageResources {
    pub xobjects: BTreeMap<String, PdfValue>,
    pub fonts: BTreeMap<String, FontInfo>,
}

pub fn collect_resources(
    doc: &PdfDoc,
    page_dict: &BTreeMap<String, PdfValue>,
) -> Result<PageResources> {
    let mut res_out = PageResources::default();
    if let Some(res) = page_dict.get("Resources").and_then(|v| as_dict(v)) {
        if let Some(xobj) = res.get("XObject").and_then(|v| as_dict(v)) {
            for (k, v) in xobj {
                res_out.xobjects.insert(k.clone(), v.clone());
            }
        }
        if let Some(fdict) = res.get("Font").and_then(|v| as_dict(v)) {
            for (name, fv) in fdict {
                let rf = resolve(doc, fv, 0).unwrap_or_else(|_| fv.clone());
                if let Some(fd) = as_dict(&rf) {
                    let mut fi = FontInfo::default();
                    if let Some(enc_name) = fd
                        .get("Encoding")
                        .and_then(|v| as_name(v))
                        .map(|s| s.to_string())
                    {
                        fi.base_encoding = Some(enc_name);
                    }
                    if let Some(tu) = fd.get("ToUnicode") {
                        let rf2 = resolve(doc, tu, 0).unwrap_or_else(|_| tu.clone());
                        if let PdfValue::Stream { dict: sdict, data } = rf2 {
                            if let Ok(dec) = get_stream_data_with_filters(&sdict, data) {
                                fi.to_unicode = Some(crate::measure!(
                                    MetricKey::Fonts,
                                    parse_tounicode_cmap(&dec)
                                ));
                            }
                        }
                    }
                    res_out.fonts.insert(name.clone(), fi);
                }
            }
        }
    }
    Ok(res_out)
}
