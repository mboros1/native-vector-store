use anyhow::{bail, Context, Result};
// Sequential extractor for robustness under xargs; no per-file threading here.
use std::fs::File;
use std::io::Read;
use std::path::Path;

#[derive(Clone, Debug)]
pub struct PageText {
    pub page_number: i32, // 0-based
    pub text: String,
}

pub fn extract_text_pages(
    pdf_path: &Path,
    page_limit: Option<usize>,
    _threads: usize,
) -> Result<Vec<(String, i32)>> {
    use pdfium_render::prelude::*;

    if !pdf_path.exists() {
        bail!("PDF not found: {}", pdf_path.display());
    }

    // Helper to bind pdfium
    fn bind_pdfium() -> Result<Pdfium> {
        let bundle_dir_build = option_env!("PDFIUM_BUNDLE_DIR").map(|s| s.to_string());
        let lib_path_build = option_env!("PDFIUM_LIBRARY_PATH").map(|s| s.to_string());
        let lib_dir_build = option_env!("PDFIUM_LIB_DIR").map(|s| s.to_string());
        let bindings = if let Some(lib_path) =
            lib_path_build.or_else(|| std::env::var("PDFIUM_LIBRARY_PATH").ok())
        {
            let p = std::path::Path::new(&lib_path);
            Pdfium::bind_to_library(p)
                .map_err(|e| anyhow::anyhow!("bind pdfium at {}: {}", lib_path, e))?
        } else if let Some(lib_dir) = lib_dir_build
            .or_else(|| std::env::var("PDFIUM_LIB_DIR").ok())
            .or(bundle_dir_build)
        {
            let dir = std::path::Path::new(&lib_dir);
            let name = Pdfium::pdfium_platform_library_name_at_path(dir);
            Pdfium::bind_to_library(name)
                .map_err(|e| anyhow::anyhow!("bind pdfium in {}: {}", lib_dir, e))?
        } else {
            Pdfium::bind_to_system_library()
                .map_err(|e| anyhow::anyhow!("bind pdfium (system): {}", e))?
        };
        Ok(Pdfium::new(bindings))
    }

    // Sequential extraction
    let pdfium = bind_pdfium()?;
    let doc = pdfium
        .load_pdf_from_file(pdf_path, None)
        .with_context(|| format!("open pdf: {}", pdf_path.display()))?;
    let page_count = doc.pages().len() as usize;
    let limit = page_limit.unwrap_or(page_count).min(page_count);

    let mut out = Vec::with_capacity(limit);
    for i in 0..limit {
        let page = doc.pages().get(i as u16)?;
        let text = page.text()?.all();
        out.push((text, i as i32));
    }
    Ok(out)
}

// Utility: read whole file for hashing if needed elsewhere
pub fn read_file_bytes(path: &Path) -> Result<Vec<u8>> {
    let mut f = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf)?;
    Ok(buf)
}
