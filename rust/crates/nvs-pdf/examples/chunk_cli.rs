use anyhow::Result;
use clap::Parser;
use std::fs;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "nvs-pdf chunk")] 
#[command(about = "Chunk a PDF into Docling-style JSON chunks", long_about = None)]
struct Cli {
    /// Input PDF or directory of PDFs
    #[arg(short = 'i', long = "input")]
    input: PathBuf,
    /// Output chunks JSON (for single file) or output directory (for directory mode)
    #[arg(short = 'o', long = "output")]
    output: Option<PathBuf>,
    /// Maximum tokens per chunk
    #[arg(long = "max-chunk-size", default_value_t = 512)]
    max_chunk_size: usize,
    /// Minimum tokens per chunk
    #[arg(long = "min-chunk-size", default_value_t = 150)]
    min_chunk_size: usize,
    /// Overlap tokens between chunks
    #[arg(long = "overlap", default_value_t = 50)]
    overlap: usize,
    /// Optional page limit
    #[arg(long = "page-limit")]
    page_limit: Option<usize>,
    /// Threads for PDF extraction (0 = auto)
    #[arg(long = "threads", default_value_t = 0)]
    threads: usize,
    /// Recurse into subdirectories when input is a directory
    #[arg(long = "recursive", default_value_t = true)]
    recursive: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let opts = nvs_pdf::ChunkOptions {
        max_tokens: cli.max_chunk_size,
        min_tokens: cli.min_chunk_size,
        overlap_tokens: cli.overlap,
        thread_count: cli.threads,
        page_limit: cli.page_limit,
    };

    if cli.input.is_file() {
        if std::env::var("NVS_PDF_LOG").is_ok() { eprintln!("[nvs-pdf] processing single file: {}", cli.input.display()); }
        let (chunks, stats) = nvs_pdf::parse_to_chunks_with_stats(&cli.input, &opts)?;
        let out = if let Some(o) = cli.output {
            o
        } else {
            let stem = cli.input.file_stem().and_then(|s| s.to_str()).unwrap_or("output");
            let parent = cli.input.parent().unwrap_or_else(|| std::path::Path::new("."));
            parent.join(format!("{}_chunks.json", stem))
        };
        nvs_pdf::write_chunks_json(&cli.input, &chunks, &out)?;
        eprintln!(
            "Wrote {} chunks to {} (pages={}, extract={}ms, chunk={}ms, total={}ms)",
            chunks.len(), out.display(), stats.pages, stats.extract_ms, stats.chunk_ms, stats.total_ms
        );
        return Ok(());
    }

    if cli.input.is_dir() {
        if std::env::var("NVS_PDF_LOG").is_ok() { eprintln!("[nvs-pdf] scanning directory: {}", cli.input.display()); }
        // Determine output directory
        let out_dir = if let Some(o) = cli.output.clone() {
            o
        } else {
            cli.input.clone()
        };
        fs::create_dir_all(&out_dir)?;

        // Collect pdfs
        let mut pdfs: Vec<PathBuf> = Vec::new();
        if cli.recursive {
            for entry in walkdir::WalkDir::new(&cli.input).into_iter().filter_map(|e| e.ok()) {
                if entry.file_type().is_file() {
                    if let Some(ext) = entry.path().extension().and_then(|e| e.to_str()) {
                        if ext.eq_ignore_ascii_case("pdf") { pdfs.push(entry.path().to_path_buf()); }
                    }
                }
            }
        } else {
            for entry in fs::read_dir(&cli.input)? {
                if let Ok(e) = entry { let p = e.path(); if p.is_file() {
                    if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                        if ext.eq_ignore_ascii_case("pdf") { pdfs.push(p); }
                    }
                }}
            }
        }
        pdfs.sort();
        if pdfs.is_empty() {
            eprintln!("No PDFs found in {}", cli.input.display());
            return Ok(());
        }

        eprintln!("Found {} PDFs. Output dir: {}", pdfs.len(), out_dir.display());
        let mut total_pages = 0usize; let mut total_chunks = 0usize; let mut total_extract_ms = 0u128; let mut total_chunk_ms = 0u128; let mut total_ms = 0u128;
        eprintln!("Using threads={} for page extraction", opts.thread_count);
        for (idx, pdf) in pdfs.iter().enumerate() {
            eprintln!("Processing [{}/{}]: {}", idx+1, pdfs.len(), pdf.display());
            if std::env::var("NVS_PDF_LOG").is_ok() { eprintln!("[nvs-pdf] start {}", pdf.display()); }
            let (chunks, stats) = nvs_pdf::parse_to_chunks_with_stats(pdf, &opts)?;
            let stem = pdf.file_stem().and_then(|s| s.to_str()).unwrap_or("output");
            let out_file = out_dir.join(format!("{}_chunks.json", stem));
            nvs_pdf::write_chunks_json(pdf, &chunks, &out_file)?;
            eprintln!("[{}/{}] {} -> {} (pages={}, chunks={}, extract={}ms, chunk={}ms, total={}ms)", idx+1, pdfs.len(), pdf.display(), out_file.display(), stats.pages, chunks.len(), stats.extract_ms, stats.chunk_ms, stats.total_ms);
            if std::env::var("NVS_PDF_LOG").is_ok() { eprintln!("[nvs-pdf] done {}", pdf.display()); }
            total_pages += stats.pages; total_chunks += chunks.len(); total_extract_ms += stats.extract_ms; total_chunk_ms += stats.chunk_ms; total_ms += stats.total_ms;
        }
        eprintln!("Done. PDFs={}, pages={}, chunks={}, extract={}ms, chunk={}ms, total={}ms", pdfs.len(), total_pages, total_chunks, total_extract_ms, total_chunk_ms, total_ms);
        return Ok(());
    }

    anyhow::bail!("Input path is neither file nor directory: {}", cli.input.display());
}
