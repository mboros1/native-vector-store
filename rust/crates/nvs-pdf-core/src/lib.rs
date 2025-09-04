use anyhow::Result;
use serde::Serialize;
use std::path::Path;

pub mod filters;
pub mod parser; // thin re-export layer
pub mod objects;
pub mod pages;
pub mod streams;
pub mod content;
pub mod fonts;

#[derive(Debug, Default, Serialize, Clone)]
pub struct ProbeResult {
    pub path: String,
    pub size_bytes: u64,
    pub page_candidates: usize,
    pub has_encrypt: bool,
    pub has_tounicode: bool,
    pub filter_flate: bool,
    pub filter_lzw: bool,
    pub filter_ascii85: bool,
    pub filter_asciihex: bool,
    pub filter_runlength: bool,
}

pub fn probe_pdf_bytes(path: &str, data: &[u8]) -> ProbeResult {
    // Heuristic, fast regex scans; not a full parser.
    let s = match std::str::from_utf8(data) {
        Ok(v) => v,
        Err(_) => "", // Binary or mixed: skip string-based probes below
    };
    let mut r = ProbeResult { path: path.to_string(), size_bytes: data.len() as u64, ..Default::default() };
    // Page candidates: count "/Type /Page" occurrences
    if !s.is_empty() {
        r.page_candidates = s.matches("/Type /Page").count();
        r.has_encrypt = s.contains("/Encrypt");
        r.has_tounicode = s.contains("/ToUnicode");
        r.filter_flate = s.contains("/FlateDecode");
        r.filter_lzw = s.contains("/LZWDecode");
        r.filter_ascii85 = s.contains("/ASCII85Decode");
        r.filter_asciihex = s.contains("/ASCIIHexDecode");
        r.filter_runlength = s.contains("/RunLengthDecode");
    } else {
        // Fallback: scan raw bytes for common filter names
        let hay = data;
        r.filter_flate = memmem(hay, b"FlateDecode");
        r.filter_lzw = memmem(hay, b"LZWDecode");
        r.filter_ascii85 = memmem(hay, b"ASCII85Decode");
        r.filter_asciihex = memmem(hay, b"ASCIIHexDecode");
        r.filter_runlength = memmem(hay, b"RunLengthDecode");
        r.has_encrypt = memmem(hay, b"/Encrypt");
        r.has_tounicode = memmem(hay, b"/ToUnicode");
        // rough page count
        r.page_candidates = byte_count(hay, b"/Type /Page");
    }
    r
}

fn memmem(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}
fn byte_count(hay: &[u8], needle: &[u8]) -> usize {
    hay.windows(needle.len()).filter(|w| *w == needle).count()
}

pub fn probe_path(path: &std::path::Path) -> Result<ProbeResult> {
    use memmap2::MmapOptions;
    let f = std::fs::File::open(path)?;
    let meta = f.metadata()?;
    let mmap = unsafe { MmapOptions::new().map(&f)? };
    Ok(probe_pdf_bytes(&path.display().to_string(), &mmap))
}

#[derive(Debug, Default, Serialize, Clone)]
pub struct ProbeSummary {
    pub files: usize,
    pub total_bytes: u64,
    pub total_pages_est: usize,
    pub encrypted: usize,
    pub with_tounicode: usize,
    pub flate: usize,
    pub lzw: usize,
    pub ascii85: usize,
    pub asciihex: usize,
    pub runlength: usize,
}

pub fn summarize(results: &[ProbeResult]) -> ProbeSummary {
    let mut s = ProbeSummary::default();
    s.files = results.len();
    for r in results {
        s.total_bytes += r.size_bytes;
        s.total_pages_est += r.page_candidates;
        s.encrypted += (r.has_encrypt as usize);
        s.with_tounicode += (r.has_tounicode as usize);
        s.flate += (r.filter_flate as usize);
        s.lzw += (r.filter_lzw as usize);
        s.ascii85 += (r.filter_ascii85 as usize);
        s.asciihex += (r.filter_asciihex as usize);
        s.runlength += (r.filter_runlength as usize);
    }
    s
}

// Fast path stub: attempt to extract pages using Rust fast-path. Returns
// Ok(Some(pages)) when supported, Ok(None) to signal fallback to PDFium.
pub fn fast_extract_pages(path: &Path, page_limit: Option<usize>) -> Result<Option<Vec<(String, i32)>>> {
    let f = std::fs::File::open(path)?;
    let mmap = unsafe { memmap2::MmapOptions::new().map(&f)? };
    let pr = probe_pdf_bytes(&path.display().to_string(), &mmap);
    if pr.has_encrypt || pr.filter_lzw { return Ok(None); }
    // Build doc by scanning objects (fast-path)
    let doc = parser::PdfDoc::from_bytes(&mmap)?;
    let mut pages_out = Vec::new();
    // Prefer page tree traversal; fallback to direct scan
    let mut page_ids = match parser::collect_pages_via_tree(&doc) {
        Ok(v) if !v.is_empty() => v,
        _ => parser::collect_page_object_ids(&doc),
    };
    if let Some(limit) = page_limit { page_ids.truncate(limit); }
    for (idx, id) in page_ids.into_iter().enumerate() {
        match parser::extract_page_text(&doc, id) {
            Ok(txt) => pages_out.push((txt, idx as i32)),
            Err(_) => return Ok(None), // fallback if any page fails
        }
    }
    Ok(Some(pages_out))
}
