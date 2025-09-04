use anyhow::{bail, Context, Result};
use std::fs::File;
use std::io::Read;
use std::path::Path;

#[derive(Clone, Debug)]
pub struct PageText {
    pub page_number: i32, // 0-based
    pub text: String,
}

#[cfg(any(feature = "pdfium-system", feature = "pdfium-bundled"))]
pub fn extract_text_pages(pdf_path: &Path, page_limit: Option<usize>, _threads: usize) -> Result<Vec<(String, i32)>> {
    use pdfium_render::prelude::*;

    if !pdf_path.exists() {
        bail!("PDF not found: {}", pdf_path.display());
    }

    // Try env-provided path first (supports bundled installs). Prefer compile-time provided
    // envs from build.rs, then runtime.
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
        // Fallback to system loader (works for system installs and sometimes for bundled if on loader path)
        Pdfium::bind_to_system_library().map_err(|e| anyhow::anyhow!("bind pdfium (system): {}", e))?
    };
    let pdfium = Pdfium::new(bindings);
    let doc = pdfium.load_pdf_from_file(pdf_path, None)
        .with_context(|| format!("open pdf: {}", pdf_path.display()))?;

    let page_count = doc.pages().len() as usize;
    let limit = page_limit.unwrap_or(page_count).min(page_count);

    // Extract sequentially; pdfium-render handles internal threading. We can parallelize later.
    let mut out = Vec::with_capacity(limit);
    for i in 0..limit {
        let page = doc.pages().get(i as u16)?;
        // all() returns all text; we rely on embedded newlines to split into lines later.
        let text = page.text()?.all();
        out.push((text, i as i32));
    }
    Ok(out)
}

#[cfg(not(any(feature = "pdfium-system", feature = "pdfium-bundled")))]
pub fn extract_text_pages(_pdf_path: &Path, _page_limit: Option<usize>, _threads: usize) -> Result<Vec<(String, i32)>> {
    bail!("pdfium not enabled; build with --features nvs-pdf/pdfium-bundled (downloads at build) or nvs-pdf/pdfium-system");
}

// Utility: read whole file for hashing if needed elsewhere
pub fn read_file_bytes(path: &Path) -> Result<Vec<u8>> {
    let mut f = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf)?;
    Ok(buf)
}
