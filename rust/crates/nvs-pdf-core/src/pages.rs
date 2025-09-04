use anyhow::{anyhow, Result};
use crate::objects::{PdfDoc, PdfValue, as_dict, as_array, as_name, parse_indirect_object};

pub fn collect_page_object_ids(doc: &PdfDoc) -> Vec<(u32,u16)> {
    let mut pages = Vec::new();
    for ((obj, gen), val) in iter_all_objects_values(doc) {
        if let Some(d) = as_dict(&val) {
            if d.get("Type").and_then(|v| as_name(v)) == Some("Page") { pages.push((obj, gen)); }
        }
    }
    pages
}

pub fn collect_pages_via_tree(doc: &PdfDoc) -> Result<Vec<(u32,u16)>> {
    // Find Catalog among all objects (including ObjStm-expanded)
    let mut catalog_ref: Option<(u32,u16)> = None;
    for ((obj,gen), val) in iter_all_objects_values(doc) {
        if let Some(d) = as_dict(&val) {
            if d.get("Type").and_then(|v| as_name(v)) == Some("Catalog") { catalog_ref = Some((obj,gen)); break; }
        }
    }
    let (cat_obj, cat_gen) = catalog_ref.ok_or_else(|| anyhow!("Catalog not found"))?;
    let catalog = doc.get_object(cat_obj, cat_gen)?;
    let cat_dict = as_dict(&catalog).ok_or_else(|| anyhow!("Catalog not dict"))?;
    let pages_val = cat_dict.get("Pages").ok_or_else(|| anyhow!("Catalog missing Pages"))?;
    let mut out = Vec::new();
    traverse_pages_node(doc, pages_val, &mut out, 0)?;
    Ok(out)
}

// Helper: iterate all objects as parsed values, including inline_objects (ObjStm-expanded)
fn iter_all_objects_values(doc: &PdfDoc) -> Vec<((u32,u16), PdfValue)> {
    let mut out = Vec::with_capacity(doc.objects.len() + doc.inline_objects.len());
    for (&(obj,gen), range) in doc.objects.iter() {
        if let Ok(val) = parse_indirect_object(&doc.data[range.clone()]) { out.push(((obj,gen), val)); }
    }
    for (&(obj,gen), buf) in doc.inline_objects.iter() {
        if let Ok(val) = parse_indirect_object(buf) { out.push(((obj,gen), val)); }
    }
    out
}

fn traverse_pages_node(doc: &PdfDoc, node: &PdfValue, out: &mut Vec<(u32,u16)>, depth: usize) -> Result<()> {
    if depth > 64 { return Err(anyhow!("pages tree too deep")); }
    let val = match node { PdfValue::Ref(o,g) => doc.get_object(*o,*g)?, _ => node.clone() };
    let d = as_dict(&val).ok_or_else(|| anyhow!("pages node not dict"))?;
    match d.get("Type").and_then(|v| as_name(v)) {
        Some("Pages") => {
            if let Some(kids) = d.get("Kids").and_then(|v| as_array(v)) {
                for kid in kids { traverse_pages_node(doc, kid, out, depth+1)?; }
            }
            Ok(())
        }
        Some("Page") => { if let PdfValue::Ref(o,g) = node { out.push((*o,*g)); } Ok(()) }
        _ => Ok(()),
    }
}
