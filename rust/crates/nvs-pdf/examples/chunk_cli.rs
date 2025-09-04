use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "nvs-pdf chunk")] 
#[command(about = "Chunk a PDF into Docling-style JSON chunks", long_about = None)]
struct Cli {
    /// Input PDF
    #[arg(short = 'i', long = "input")]
    input: PathBuf,
    /// Output chunks JSON
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
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let opts = nvs_pdf::ChunkOptions {
        max_tokens: cli.max_chunk_size,
        min_tokens: cli.min_chunk_size,
        overlap_tokens: cli.overlap,
        thread_count: 0,
        page_limit: cli.page_limit,
    };

    let chunks = nvs_pdf::parse_to_chunks(&cli.input, &opts)?;

    let out = if let Some(o) = cli.output {
        o
    } else {
        let stem = cli.input.file_stem().and_then(|s| s.to_str()).unwrap_or("output");
        let parent = cli.input.parent().unwrap_or_else(|| std::path::Path::new("."));
        parent.join(format!("{}_chunks.json", stem))
    };

    nvs_pdf::write_chunks_json(&cli.input, &chunks, &out)?;
    eprintln!("Wrote {} chunks to {}", chunks.len(), out.display());
    Ok(())
}

