use anyhow::{bail, Context, Result};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::time::Instant;

#[derive(Clone, Debug)]
pub struct PageText {
    pub page_number: i32, // 0-based
    pub text: String,
}

#[derive(Clone, Copy, Default, Debug)]
pub struct ExtractStats {
    pub bind_open_ms: u128,
    pub pages_ms: u128,
    pub total_ms: u128,
}

pub fn extract_text_pages_with_stats(
    pdf_path: &Path,
    page_limit: Option<usize>,
    _threads: usize,
) -> Result<(Vec<(String, i32)>, ExtractStats)> {
    if !pdf_path.exists() {
        bail!("PDF not found: {}", pdf_path.display());
    }

    // First try the custom Rust backend fast path; if it yields pages, use them.
    if let Ok((maybe_pages, br)) = nvs_pdf_core::fast_extract_pages_with_stats(pdf_path, page_limit) {
        if let Some(pages) = maybe_pages {
            let stats = ExtractStats {
                bind_open_ms: br.io_ms + br.build_ms + br.tree_ms,
                pages_ms: br.pages_ms,
                total_ms: br.total_ms,
            };
            return Ok((pages, stats));
        }
        // else: fall through to PDFium if available
    }

    // Fallback to PDFium if the feature is enabled.
    #[cfg(feature = "pdfium")]
    {
        use pdfium_render::prelude::*;
        use once_cell::sync::Lazy;
        use std::sync::Mutex;

        let t0 = Instant::now();
        let t_bind = Instant::now();
        static BIND_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));
        let _g = BIND_MUTEX.lock().unwrap();
        let pdfium = {
            let bundle_dir_build = option_env!("PDFIUM_BUNDLE_DIR").map(|s| s.to_string());
            let lib_path_build = option_env!("PDFIUM_LIBRARY_PATH").map(|s| s.to_string());
            let lib_dir_build = option_env!("PDFIUM_LIB_DIR").map(|s| s.to_string());
            let bindings = if let Some(lib_path) = lib_path_build.or_else(|| std::env::var("PDFIUM_LIBRARY_PATH").ok()) {
                let p = std::path::Path::new(&lib_path);
                Pdfium::bind_to_library(p).map_err(|e| anyhow::anyhow!("bind pdfium at {}: {}", lib_path, e))?
            } else if let Some(lib_dir) = lib_dir_build.or_else(|| std::env::var("PDFIUM_LIB_DIR").ok()).or(bundle_dir_build) {
                let dir = std::path::Path::new(&lib_dir);
                let name = Pdfium::pdfium_platform_library_name_at_path(dir);
                Pdfium::bind_to_library(name).map_err(|e| anyhow::anyhow!("bind pdfium in {}: {}", lib_dir, e))?
            } else {
                Pdfium::bind_to_system_library().map_err(|e| anyhow::anyhow!("bind pdfium (system): {}", e))?
            };
            Pdfium::new(bindings)
        };
        let doc = pdfium
            .load_pdf_from_file(pdf_path, None)
            .with_context(|| format!("open pdf: {}", pdf_path.display()))?;
        let bind_open_ms = t_bind.elapsed().as_millis();
        let page_count = doc.pages().len() as usize;
        let limit = page_limit.unwrap_or(page_count).min(page_count);

        let t_pages = Instant::now();
        let mut out = Vec::with_capacity(limit);
        for i in 0..limit {
            let page = doc.pages().get(i as u16)?;
            let text = page.text()?.all();
            out.push((text, i as i32));
        }
        let pages_ms = t_pages.elapsed().as_millis();
        let total_ms = t0.elapsed().as_millis();
        return Ok((out, ExtractStats { bind_open_ms, pages_ms, total_ms }));
    }

    // If we reach here without the pdfium feature, the fast path couldn’t extract text.
    Err(anyhow::anyhow!(
        "PDF requires pdfium fallback, but feature 'pdfium' is not enabled"
    ))
}

#[cfg(feature = "pdfium")]
pub fn extract_text_pages_with_pdfium(
    pdfium: &pdfium_render::prelude::Pdfium,
    pdf_path: &Path,
    page_limit: Option<usize>,
) -> Result<(Vec<(String, i32)>, ExtractStats)> {
    use pdfium_render::prelude::*;
    let t0 = Instant::now();
    let t_open = Instant::now();
    let doc = pdfium
        .load_pdf_from_file(pdf_path, None)
        .with_context(|| format!("open pdf: {}", pdf_path.display()))?;
    let open_ms = t_open.elapsed().as_millis();
    let page_count = doc.pages().len() as usize;
    let limit = page_limit.unwrap_or(page_count).min(page_count);
    let t_pages = Instant::now();
    let mut out = Vec::with_capacity(limit);
    for i in 0..limit {
        let page = doc.pages().get(i as u16)?;
        let text = page.text()?.all();
        out.push((text, i as i32));
    }
    let pages_ms = t_pages.elapsed().as_millis();
    let total_ms = t0.elapsed().as_millis();
    Ok((out, ExtractStats { bind_open_ms: open_ms, pages_ms, total_ms }))
}

pub fn extract_text_pages(
    pdf_path: &Path,
    page_limit: Option<usize>,
    threads: usize,
) -> Result<Vec<(String, i32)>> {
    let (pages, _stats) = extract_text_pages_with_stats(pdf_path, page_limit, threads)?;
    Ok(pages)
}

// Utility: read whole file for hashing if needed elsewhere
pub fn read_file_bytes(path: &Path) -> Result<Vec<u8>> {
    let mut f = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf)?;
    Ok(buf)
}
