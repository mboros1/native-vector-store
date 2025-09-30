use anyhow::Result;
use clap::Parser;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Parser, Debug)]
#[command(name = "chunk-html-cli")]
#[command(about = "Chunk an HTML file or directory into Docling-style JSON chunks (Rust)", long_about = None
)]
struct Cli {
    /// Input HTML (file) or directory (recurses by default)
    #[arg(short = 'i', long = "input")]
    input: PathBuf,
    /// Output JSON (for file input) or directory (for dir input)
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
    /// Optional section limit
    #[arg(long = "section-limit")]
    section_limit: Option<usize>,
    /// Recurse when input is a directory
    #[arg(long = "recursive", default_value_t = true)]
    recursive: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    if cli.input.is_file() {
        let out = if let Some(o) = cli.output.clone() {
            o
        } else {
            let stem = cli
                .input
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("output");
            cli.input
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join(format!("{}_chunks.json", stem))
        };
        process_one(&cli.input, &out, &cli)?;
        println!("{} -> {}", cli.input.display(), out.display());
        return Ok(());
    }
    if cli.input.is_dir() {
        let out_dir = if let Some(o) = cli.output.clone() {
            o
        } else {
            cli.input.clone()
        };
        fs::create_dir_all(&out_dir)?;
        let mut count = 0usize;
        if cli.recursive {
            for entry in walkdir::WalkDir::new(&cli.input)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                if entry.file_type().is_file() && is_html(entry.path()) {
                    let stem = entry
                        .path()
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("output");
                    let out = out_dir.join(format!("{}_chunks.json", stem));
                    let _ = process_one(&entry.path().to_path_buf(), &out, &cli);
                    count += 1;
                }
            }
        } else {
            for entry in fs::read_dir(&cli.input)? {
                if let Ok(e) = entry {
                    let p = e.path();
                    if p.is_file() && is_html(&p) {
                        let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("output");
                        let out = out_dir.join(format!("{}_chunks.json", stem));
                        let _ = process_one(&p, &out, &cli);
                        count += 1;
                    }
                }
            }
        }
        println!("Processed {} files into {}", count, out_dir.display());
        return Ok(());
    }
    anyhow::bail!("input path not found: {}", cli.input.display())
}

fn is_html(p: &Path) -> bool {
    match p.extension().and_then(|e| e.to_str()) {
        Some(ext) => matches!(ext.to_ascii_lowercase().as_str(), "html" | "htm"),
        None => false,
    }
}

fn process_one(input: &PathBuf, out_path: &PathBuf, cli: &Cli) -> Result<()> {
    let opts = nvs_html::HtmlChunkOptions {
        max_tokens: cli.max_chunk_size,
        min_tokens: cli.min_chunk_size,
        overlap_tokens: cli.overlap,
        section_limit: cli.section_limit,
    };
    let (chunks, _stats) = nvs_html::parse_to_chunks_with_stats(input, &opts)?;
    nvs_html::write_chunks_json(input, &chunks, out_path)?;
    Ok(())
}
