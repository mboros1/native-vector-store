use anyhow::Result;
use memmap2::MmapOptions;
use serde::Serialize;
use std::path::Path;

pub mod parse;

#[derive(Debug, Default, Serialize, Clone)]
pub struct HtmlExtractBreakdown {
    pub io_ms: u128,
    pub decode_ms: u128,
    pub parse_ms: u128,
    pub sections_ms: u128,
    pub total_ms: u128,
}

/// High-level entry: mmap a file and extract sections as (text, section_index).
///
/// Example (no_run)
/// ```no_run
/// let path = std::path::Path::new("/path/to/file.html");
/// let (sections, stats) = nvs_html_core::fast_extract_sections_with_stats(path, Some(10))?;
/// assert!(sections.len() <= 10);
/// # anyhow::Ok(())
/// ```
// High-level entry: mmap a file and extract sections
pub fn fast_extract_sections_with_stats(
    path: &Path,
    section_limit: Option<usize>,
) -> Result<(Vec<(String, i32)>, HtmlExtractBreakdown)> {
    use std::time::Instant;
    let t0 = Instant::now();
    let ti = Instant::now();
    let f = std::fs::File::open(path)?;
    let mmap = unsafe { MmapOptions::new().map(&f)? };
    let io_ms = ti.elapsed().as_millis();
    let (sections, mut br) = fast_extract_sections_from_bytes_with_stats(&mmap, section_limit)?;
    br.io_ms = io_ms;
    br.total_ms = t0.elapsed().as_millis();
    Ok((sections, br))
}

/// Variant from provided bytes (already in memory).
///
/// Example
/// ```
/// let html = br#"<html><body><h1>Title</h1><p>Hello</p></body></html>"#;
/// let (sections, stats) = nvs_html_core::fast_extract_sections_from_bytes_with_stats(html, None)?;
/// assert!(!sections.is_empty());
/// # anyhow::Ok(())
/// ```
// Variant from provided bytes (already in memory)
pub fn fast_extract_sections_from_bytes_with_stats(
    data: &[u8],
    section_limit: Option<usize>,
) -> Result<(Vec<(String, i32)>, HtmlExtractBreakdown)> {
    use std::time::Instant;
    let t0 = Instant::now();
    // Decode to UTF-8 using encoding_rs
    let td = Instant::now();
    let (decoded, _, had_errors) = encoding_rs::UTF_8.decode(data);
    let decoded_owned: String;
    let html: &str = if had_errors {
        decoded_owned = decoded.into_owned();
        &decoded_owned
    } else {
        // If decode returns Cow::Borrowed, it still points to an internal buffer; own it to be safe
        decoded_owned = decoded.into_owned();
        &decoded_owned
    };
    let decode_ms = td.elapsed().as_millis();

    let tp = Instant::now();
    let dom = parse::parse_html_to_dom(html);
    let parse_ms = tp.elapsed().as_millis();

    let ts = Instant::now();
    let mut sections = parse::extract_sections(&dom);
    if let Some(limit) = section_limit { sections.truncate(limit); }
    let sections_ms = ts.elapsed().as_millis();

    let out = sections
        .into_iter()
        .enumerate()
        .map(|(i, s)| (s, i as i32))
        .collect::<Vec<_>>();

    Ok((
        out,
        HtmlExtractBreakdown { io_ms: 0, decode_ms, parse_ms, sections_ms, total_ms: t0.elapsed().as_millis() },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn extract_basic_sections() {
        let html = br#"<html><body><h1>Title</h1><p>A</p><h2>Sub</h2><p>B</p></body></html>"#;
        let (sections, _stats) = fast_extract_sections_from_bytes_with_stats(html, None).unwrap();
        assert!(sections.len() >= 2);
    }
}
