use anyhow::Result;
use serde::Serialize;
use std::path::Path;

pub mod content;
pub mod debug;
pub mod filters;
pub mod fonts;
pub mod interpret;
pub mod metrics;
pub mod normalize;
pub mod objects;
pub mod pages;
pub mod parser; // thin re-export layer
pub mod resources;
pub mod stats;
pub mod streams;

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

/// Probe PDF bytes for quick stats (filters, encryption, rough page count).
///
/// Example
/// ```
/// let bytes = b"%PDF-1.7\n1 0 obj\n<</Type /Page>>\nendobj\n";
/// let pr = nvs_pdf_core::probe_pdf_bytes("mem.pdf", bytes);
/// assert!(pr.page_candidates >= 0);
/// ```
pub fn probe_pdf_bytes(path: &str, data: &[u8]) -> ProbeResult {
    // Heuristic, fast regex scans; not a full parser.
    let s = std::str::from_utf8(data).unwrap_or_else(|_| "");
    let mut r = ProbeResult {
        path: path.to_string(),
        size_bytes: data.len() as u64,
        ..Default::default()
    };
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

/// Probe a PDF on disk via memory-mapping.
///
/// Example (no_run)
/// ```no_run
/// let pr = nvs_pdf_core::probe_path(std::path::Path::new("/path/to/file.pdf"))?;
/// println!("pages_est={}", pr.page_candidates);
/// # anyhow::Ok(())
/// ```
pub fn probe_path(path: &Path) -> Result<ProbeResult> {
    use memmap2::MmapOptions;
    let f = std::fs::File::open(path)?;
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
        s.encrypted += r.has_encrypt as usize;
        s.with_tounicode += r.has_tounicode as usize;
        s.flate += r.filter_flate as usize;
        s.lzw += r.filter_lzw as usize;
        s.ascii85 += r.filter_ascii85 as usize;
        s.asciihex += r.filter_asciihex as usize;
        s.runlength += r.filter_runlength as usize;
    }
    s
}

// Fast path stub: attempt to extract pages using Rust fast-path. Returns
// Ok(Some(pages)) when supported, Ok(None) to signal fallback to PDFium.
/// Try the fast Rust extractor; returns `Ok(None)` if unsupported (e.g., encrypted).
///
/// Example (no_run)
/// ```no_run
/// let out = nvs_pdf_core::fast_extract_pages(std::path::Path::new("/path/to/file.pdf"), Some(2))?;
/// if let Some(pages) = out { println!("{}", pages.len()); }
/// # anyhow::Ok(())
/// ```
pub fn fast_extract_pages(
    path: &Path,
    page_limit: Option<usize>,
) -> Result<Option<Vec<(String, i32)>>> {
    Ok(fast_extract_pages_with_stats(path, page_limit)?.0)
}

#[derive(Debug, Default, Serialize, Clone)]
pub struct FastExtractBreakdown {
    pub io_ms: u128,
    pub build_ms: u128,
    pub tree_ms: u128,
    pub pages_ms: u128,
    pub interpret_ms: u128,
    pub decode_ms: u128,
    pub fonts_ms: u128,
    pub resources_ms: u128,
    pub streams_ms: u128,
    pub normalize_ms: u128,
    pub total_ms: u128,
}

/// Fast extractor with timing breakdown. Returns `Ok((None, ...))` when unsupported.
///
/// Example (no_run)
/// ```no_run
/// let (pages_opt, stats) = nvs_pdf_core::fast_extract_pages_with_stats(std::path::Path::new("/path.pdf"), None)?;
/// assert!(stats.total_ms >= 0);
/// # anyhow::Ok(())
/// ```
pub fn fast_extract_pages_with_stats(
    path: &Path,
    page_limit: Option<usize>,
) -> Result<(Option<Vec<(String, i32)>>, FastExtractBreakdown)> {
    use memmap2::MmapOptions;
    use std::time::Instant;
    stats::reset();
    let t0 = Instant::now();
    let ti = Instant::now();
    let f = std::fs::File::open(path)?;
    let mmap = unsafe { MmapOptions::new().map(&f)? };
    let pr = probe_pdf_bytes(&path.display().to_string(), &mmap);
    let io_ms = ti.elapsed().as_millis();
    if pr.has_encrypt {
        return Ok((
            None,
            FastExtractBreakdown {
                total_ms: t0.elapsed().as_millis(),
                ..Default::default()
            },
        ));
    }
    let tb = Instant::now();
    let doc = parser::PdfDoc::from_bytes(&mmap)?;
    let build_ms = tb.elapsed().as_millis();

    let tt = Instant::now();
    let ids_tree = pages::collect_pages_via_tree(&doc).ok();
    let mut page_ids = if let Some(v) = ids_tree {
        v
    } else {
        pages::collect_page_object_ids(&doc)
    };
    let tree_ms = tt.elapsed().as_millis();
    if let Some(limit) = page_limit {
        page_ids.truncate(limit);
    }
    let mut pages_out = Vec::new();
    let mut any_ok = false;
    for (idx, id) in page_ids.into_iter().enumerate() {
        let tp = Instant::now();
        match parser::extract_page_text(&doc, id) {
            Ok(txt) => {
                pages_out.push((txt, idx as i32));
                any_ok = true;
            }
            Err(_) => { /* skip page on fast path */ }
        }
        let dt = tp.elapsed().as_millis();
        // interpreter time is added inside content module; ensure at least per-page overhead is included
        stats::add_interpret_duration(0);
        let _ = dt; // touch dt to avoid warnings if unused in future extensions
    }
    let s = stats::snapshot();
    // Convert nanos to millis (ceil) to avoid pervasive zeros from fast ops
    let to_ms = |ns: u128| -> u128 {
        if ns == 0 {
            0
        } else {
            (ns + 999_999) / 1_000_000
        }
    };
    let out = if any_ok { Some(pages_out) } else { None };
    let br = FastExtractBreakdown {
        io_ms,
        build_ms,
        tree_ms,
        pages_ms: to_ms(s.page_total_ns),
        interpret_ms: to_ms(s.interpret_ns),
        decode_ms: to_ms(s.decode_ns),
        fonts_ms: to_ms(s.fonts_ns),
        resources_ms: to_ms(s.resources_ns),
        streams_ms: to_ms(s.streams_ns),
        normalize_ms: to_ms(s.normalize_ns),
        total_ms: t0.elapsed().as_millis(),
    };
    Ok((out, br))
}

// Variant that operates directly on provided bytes (skips file IO/mmapping).
/// Fast extractor variant from in-memory bytes.
///
/// Example
/// ```
/// let dummy_pdf = b"%PDF-1.7\n1 0 obj<<>>endobj\nxref\n0 1\n0000000000 65535 f \ntrailer<<>>startxref\n0\n%%EOF";
/// let (_pages, stats) = nvs_pdf_core::fast_extract_pages_from_bytes_with_stats(dummy_pdf, None)?;
/// assert!(stats.total_ms >= 0);
/// # anyhow::Ok(())
/// ```
pub fn fast_extract_pages_from_bytes_with_stats(
    data: &[u8],
    page_limit: Option<usize>,
) -> Result<(Option<Vec<(String, i32)>>, FastExtractBreakdown)> {
    use std::time::Instant;
    stats::reset();
    let t0 = Instant::now();
    // No IO here; io_ms set to 0 in breakdown
    let tb = Instant::now();
    let doc = parser::PdfDoc::from_bytes(data)?;
    let build_ms = tb.elapsed().as_millis();

    let tt = Instant::now();
    let ids_tree = pages::collect_pages_via_tree(&doc).ok();
    let mut page_ids = if let Some(v) = ids_tree {
        v
    } else {
        pages::collect_page_object_ids(&doc)
    };
    let tree_ms = tt.elapsed().as_millis();
    if let Some(limit) = page_limit {
        page_ids.truncate(limit);
    }
    let mut pages_out = Vec::new();
    let mut any_ok = false;
    for (idx, id) in page_ids.into_iter().enumerate() {
        match parser::extract_page_text(&doc, id) {
            Ok(txt) => {
                pages_out.push((txt, idx as i32));
                any_ok = true;
            }
            Err(_) => { /* skip page on fast path */ }
        }
    }
    let s = stats::snapshot();
    let out = if any_ok { Some(pages_out) } else { None };
    let to_ms = |ns: u128| -> u128 {
        if ns == 0 {
            0
        } else {
            (ns + 999_999) / 1_000_000
        }
    };
    let br = FastExtractBreakdown {
        io_ms: 0,
        build_ms,
        tree_ms,
        pages_ms: to_ms(s.page_total_ns),
        interpret_ms: to_ms(s.interpret_ns),
        decode_ms: to_ms(s.decode_ns),
        fonts_ms: to_ms(s.fonts_ns),
        resources_ms: to_ms(s.resources_ns),
        streams_ms: to_ms(s.streams_ns),
        normalize_ms: to_ms(s.normalize_ns),
        total_ms: t0.elapsed().as_millis(),
    };
    Ok((out, br))
}

#[derive(serde::Serialize, Default, Clone)]
pub struct FastDebugReport {
    pub path: String,
    pub page_candidates: usize,
    pub pages_found: usize,
    pub used_tree: bool,
    pub pages: Vec<debug::PageTextDebug>,
    pub errors: Vec<String>,
    pub xref_streams: Vec<XrefStreamDebug>,
    pub objstm_streams: Vec<ObjStmDebug>,
}

pub fn fast_extract_pages_with_debug(
    path: &Path,
    page_limit: Option<usize>,
) -> Result<(Option<Vec<(String, i32)>>, FastDebugReport)> {
    use memmap2::MmapOptions;
    let mut report = FastDebugReport::default();
    report.path = path.display().to_string();
    let f = std::fs::File::open(path)?;
    let mmap = unsafe { MmapOptions::new().map(&f)? };
    let pr = probe_pdf_bytes(&path.display().to_string(), &mmap);
    report.page_candidates = pr.page_candidates;
    if pr.has_encrypt {
        report.errors.push("encrypted".into());
        return Ok((None, report));
    }
    let doc = match parser::PdfDoc::from_bytes(&mmap) {
        Ok(d) => d,
        Err(e) => {
            report.errors.push(format!("from_bytes: {}", e));
            // Probe XRef streams for more detail
            report.xref_streams = probe_xref_streams(&mmap);
            report.objstm_streams = probe_objstm_streams(&mmap);
            return Ok((None, report));
        }
    };
    let mut pages_out = Vec::new();
    let ids_tree = pages::collect_pages_via_tree(&doc).ok();
    let mut page_ids = if let Some(v) = ids_tree {
        report.used_tree = true;
        v
    } else {
        report.used_tree = false;
        pages::collect_page_object_ids(&doc)
    };
    report.pages_found = page_ids.len();
    if let Some(limit) = page_limit {
        page_ids.truncate(limit);
    }
    for (idx, id) in page_ids.into_iter().enumerate() {
        let (txt_opt, dbg) = debug::extract_page_text_with_debug(&doc, id);
        if let Some(txt) = txt_opt {
            pages_out.push((txt, idx as i32));
        }
        report.pages.push(dbg);
    }
    if pages_out.is_empty() {
        Ok((None, report))
    } else {
        Ok((Some(pages_out), report))
    }
}

#[derive(serde::Serialize, Default, Clone)]
pub struct XrefStreamDebug {
    pub obj: u32,
    pub gen: u16,
    pub filter: Vec<String>,
    pub decodeparms_predictor: Option<i64>,
    pub w: Vec<i64>,
    pub index_len: usize,
    pub size: Option<i64>,
    pub length: Option<i64>,
    pub decode_error: Option<String>,
    pub data_len: Option<usize>,
    pub data_preview_hex: Option<String>,
}

fn probe_xref_streams(bytes: &[u8]) -> Vec<XrefStreamDebug> {
    use crate::objects::parse_indirect_object;
    let mut out = Vec::new();
    let mut i = 0usize;
    while i + 6 < bytes.len() {
        if bytes[i].is_ascii_digit() {
            // parse obj number
            let (objnum_opt, j1) = parse_uint_local(bytes, i);
            if objnum_opt.is_none() {
                i += 1;
                continue;
            }
            let objnum = objnum_opt.unwrap() as u32;
            let j1 = skip_ws_local(bytes, j1);
            let (gen_opt, j2) = parse_uint_local(bytes, j1);
            if gen_opt.is_none() {
                i += 1;
                continue;
            }
            let gen = gen_opt.unwrap() as u16;
            let j2 = skip_ws_local(bytes, j2);
            if bytes.get(j2..j2 + 3) == Some(b"obj") {
                if let Some(end) = find_token_local(bytes, j2 + 3, b"endobj") {
                    let slice = &bytes[i..end + 6];
                    if let Ok(val) = parse_indirect_object(slice) {
                        if let objects::PdfValue::Stream { dict, data } = val {
                            if dict.get("Type").and_then(|v| as_name_local(v)) == Some("XRef") {
                                let mut d = XrefStreamDebug {
                                    obj: objnum,
                                    gen,
                                    ..Default::default()
                                };
                                if let Some(fv) = dict.get("Filter") {
                                    match fv {
                                        objects::PdfValue::Name(n) => {
                                            d.filter.push(n.clone())
                                        }
                                        objects::PdfValue::Array(arr) => {
                                            for f in arr {
                                                if let Some(n) = as_name_local(f) {
                                                    d.filter.push(n.to_string());
                                                }
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                                if let Some(dp) = dict.get("DecodeParms") {
                                    d.decodeparms_predictor = get_predictor_local(dp);
                                }
                                if let Some(warr) = dict.get("W").and_then(|v| as_array_local(v)) {
                                    d.w = warr
                                        .iter()
                                        .filter_map(|v| match v {
                                            objects::PdfValue::Int(i) => Some(*i),
                                            objects::PdfValue::Real(f) => Some(*f as i64),
                                            _ => None,
                                        })
                                        .collect();
                                }
                                if let Some(idx) = dict.get("Index").and_then(|v| as_array_local(v))
                                {
                                    d.index_len = idx.len();
                                }
                                if let Some(objects::PdfValue::Int(sz)) = dict.get("Size") {
                                    d.size = Some(*sz);
                                }
                                if let Some(lenv) = dict.get("Length") {
                                    d.length = match lenv {
                                        objects::PdfValue::Int(i) => Some(*i),
                                        objects::PdfValue::Real(f) => Some(*f as i64),
                                        _ => None,
                                    };
                                }
                                d.data_len = Some(data.len());
                                d.data_preview_hex = Some(hex_preview(&data, 16));
                                match streams::get_stream_data_with_filters(
                                    &dict,
                                    data.clone(),
                                ) {
                                    Ok(_) => { /* ok */ }
                                    Err(e) => {
                                        d.decode_error = Some(format!("{}", e));
                                    }
                                }
                                out.push(d);
                            }
                        }
                    }
                    i = end + 6;
                    continue;
                }
            }
        }
        i += 1;
    }
    out
}

#[derive(serde::Serialize, Default, Clone)]
pub struct ObjStmDebug {
    pub obj: u32,
    pub gen: u16,
    pub filter: Vec<String>,
    pub decodeparms_predictor: Option<i64>,
    pub length: Option<i64>,
    pub n: Option<i64>,
    pub first: Option<i64>,
    pub decode_error: Option<String>,
    pub data_len: Option<usize>,
    pub data_preview_hex: Option<String>,
}

fn probe_objstm_streams(bytes: &[u8]) -> Vec<ObjStmDebug> {
    use crate::objects::parse_indirect_object;
    let mut out = Vec::new();
    let mut i = 0usize;
    while i + 6 < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let (objnum_opt, j1) = parse_uint_local(bytes, i);
            if objnum_opt.is_none() {
                i += 1;
                continue;
            }
            let objnum = objnum_opt.unwrap() as u32;
            let j1 = skip_ws_local(bytes, j1);
            let (gen_opt, j2) = parse_uint_local(bytes, j1);
            if gen_opt.is_none() {
                i += 1;
                continue;
            }
            let gen = gen_opt.unwrap() as u16;
            let j2 = skip_ws_local(bytes, j2);
            if bytes.get(j2..j2 + 3) == Some(b"obj") {
                if let Some(end) = find_token_local(bytes, j2 + 3, b"endobj") {
                    let slice = &bytes[i..end + 6];
                    if let Ok(val) = parse_indirect_object(slice) {
                        if let objects::PdfValue::Stream { dict, data } = val {
                            if dict.get("Type").and_then(|v| as_name_local(v)) == Some("ObjStm") {
                                let mut d = ObjStmDebug {
                                    obj: objnum,
                                    gen,
                                    ..Default::default()
                                };
                                if let Some(fv) = dict.get("Filter") {
                                    match fv {
                                        objects::PdfValue::Name(n) => {
                                            d.filter.push(n.clone())
                                        }
                                        objects::PdfValue::Array(arr) => {
                                            for f in arr {
                                                if let Some(n) = as_name_local(f) {
                                                    d.filter.push(n.to_string());
                                                }
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                                if let Some(dp) = dict.get("DecodeParms") {
                                    d.decodeparms_predictor = get_predictor_local(dp);
                                }
                                if let Some(lenv) = dict.get("Length") {
                                    d.length = match lenv {
                                        objects::PdfValue::Int(i) => Some(*i),
                                        objects::PdfValue::Real(f) => Some(*f as i64),
                                        _ => None,
                                    };
                                }
                                if let Some(objects::PdfValue::Int(nv)) = dict.get("N") {
                                    d.n = Some(*nv);
                                }
                                if let Some(objects::PdfValue::Int(fv)) = dict.get("First") {
                                    d.first = Some(*fv);
                                }
                                d.data_len = Some(data.len());
                                d.data_preview_hex = Some(hex_preview(&data, 16));
                                match streams::get_stream_data_with_filters(
                                    &dict,
                                    data.clone(),
                                ) {
                                    Ok(_) => {}
                                    Err(e) => {
                                        d.decode_error = Some(format!("{}", e));
                                    }
                                }
                                out.push(d);
                            }
                        }
                    }
                    i = end + 6;
                    continue;
                }
            }
        }
        i += 1;
    }
    out
}

fn parse_uint_local(bytes: &[u8], mut i: usize) -> (Option<i64>, usize) {
    let start = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == start {
        return (None, i);
    }
    if let Ok(s) = std::str::from_utf8(&bytes[start..i]) {
        if let Ok(n) = s.parse::<i64>() {
            return (Some(n), i);
        }
    }
    (None, i)
}
fn skip_ws_local(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && objects::is_ws(bytes[i]) {
        i += 1;
    }
    i
}
fn find_token_local(bytes: &[u8], mut i: usize, token: &[u8]) -> Option<usize> {
    while i + token.len() <= bytes.len() {
        if &bytes[i..i + token.len()] == token {
            return Some(i);
        }
        i += 1;
    }
    None
}
fn as_name_local(v: &objects::PdfValue) -> Option<&str> {
    if let objects::PdfValue::Name(ref s) = v {
        Some(s.as_str())
    } else {
        None
    }
}
fn as_array_local(v: &objects::PdfValue) -> Option<&Vec<objects::PdfValue>> {
    if let objects::PdfValue::Array(ref a) = v {
        Some(a)
    } else {
        None
    }
}
fn get_predictor_local(dp: &objects::PdfValue) -> Option<i64> {
    match dp {
        objects::PdfValue::Dict(d) => d.get("Predictor").and_then(|v| match v {
            objects::PdfValue::Int(i) => Some(*i),
            objects::PdfValue::Real(f) => Some(*f as i64),
            _ => None,
        }),
        objects::PdfValue::Array(arr) => {
            for v in arr {
                if let Some(i) = get_predictor_local(v) {
                    return Some(i);
                }
            }
            None
        }
        _ => None,
    }
}

fn hex_preview(data: &[u8], n: usize) -> String {
    let mut s = String::new();
    let take = std::cmp::min(n, data.len());
    for (i, b) in data[..take].iter().enumerate() {
        if i > 0 {
            s.push(' ');
        }
        s.push_str(&format!("{:02X}", b));
    }
    s
}
