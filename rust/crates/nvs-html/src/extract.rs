use anyhow::Result;
use std::path::Path;

#[derive(Clone, Debug, Default)]
pub struct ExtractStats {
    pub io_ms: u128,
    pub decode_ms: u128,
    pub parse_ms: u128,
    pub sections_ms: u128,
    pub total_ms: u128,
}

pub fn extract_sections_with_stats(
    html_path: &Path,
    section_limit: Option<usize>,
) -> Result<(Vec<(String, i32)>, ExtractStats)> {
    let (sections, br) = nvs_html_core::fast_extract_sections_with_stats(html_path, section_limit)?;
    Ok((sections, ExtractStats {
        io_ms: br.io_ms,
        decode_ms: br.decode_ms,
        parse_ms: br.parse_ms,
        sections_ms: br.sections_ms,
        total_ms: br.total_ms,
    }))
}

