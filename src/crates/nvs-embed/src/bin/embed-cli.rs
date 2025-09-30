use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Parser, Debug)]
#[command(name = "embed-cli")]
#[command(about = "Embed chunked JSON (Docling-style) into NVS packer-ready docs", long_about = None
)]
struct Cli {
    /// Input chunk JSON (array of {text, meta}) or directory to process recursively
    #[arg(short = 'i', long = "input")]
    input: PathBuf,
    /// Output file or directory; if input is dir, outputs will mirror filenames with .docs.json
    #[arg(short = 'o', long = "output")]
    output: PathBuf,
    /// Backend: local|openai (default local)
    #[arg(long = "backend", default_value = "local")]
    backend: String,
    /// OpenAI embedding model (when --backend=openai)
    #[arg(long = "model", default_value = "text-embedding-3-small")]
    model: String,
    /// Local model id (when --backend=local)
    #[arg(long = "local-model-id", default_value = "thenlper/gte-small")]
    local_model_id: String,
    /// Local model directory with tokenizer.json, model.safetensors, config.json
    #[arg(long = "local-model-dir")]
    local_model_dir: Option<PathBuf>,
    /// Maximum sequence length for local backend
    #[arg(long = "max-len", default_value_t = 512)]
    max_len: usize,
    /// Concurrent requests to the embedding API
    #[arg(long = "concurrency", default_value_t = 8)]
    concurrency: usize,
    /// Inputs per request
    #[arg(long = "batch-size", default_value_t = 64)]
    batch_size: usize,
    /// Concurrent files to embed when input is a directory
    #[arg(long = "file-concurrency", default_value_t = 8)]
    file_concurrency: usize,
    /// Total concurrent API requests across all files
    #[arg(long = "total-concurrency", default_value_t = 16)]
    total_concurrency: usize,
    /// Timeout per API request in seconds
    #[arg(long = "timeout-secs", default_value_t = 120)]
    timeout_secs: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    if let Some(dir) = &cli.local_model_dir {
        std::env::set_var("NVS_LOCAL_EMBED_MODEL_DIR", dir);
    }
    let backend_is_local = cli.backend.eq_ignore_ascii_case("local");
    let backend: Arc<dyn nvs_embed::EmbeddingBackend> = match cli.backend.as_str() {
        #[cfg(feature = "local-embed")]
        s if s.eq_ignore_ascii_case("local") => {
            let local = nvs_embed::LocalGTEBackendBuilder::new()
                .model_id(cli.local_model_id.clone())
                .max_len(cli.max_len)
                .build()?;
            Arc::new(local)
        }
        "local" => {
            eprintln!(
                "warning: local backend requested but binary not built with 'local-embed' feature; falling back to OpenAI"
            );
            Arc::new(
                nvs_embed::OpenAIBackend::builder(cli.model.clone())
                    .timeout(cli.timeout_secs)
                    .build()?,
            )
        }
        _ => Arc::new(
            nvs_embed::OpenAIBackend::builder(cli.model.clone())
                .timeout(cli.timeout_secs)
                .build()?,
        ),
    };
    // Derive concurrency defaults tuned for local CPU backend
    let mut opts = nvs_embed::EmbedOptions {
        concurrency: cli.concurrency,
        batch_size: cli.batch_size,
        file_concurrency: cli.file_concurrency,
        total_concurrency: cli.total_concurrency,
    };
    if backend_is_local {
        // If user didn't override from defaults, tune for CPU: more files in flight, fewer per-file batches
        let cores = num_cpus::get().max(1);
        if cli.concurrency == 8 {
            // default value
            opts.concurrency = 1;
        }
        if cli.file_concurrency == 8 {
            // default value
            opts.file_concurrency = std::cmp::max(1, cores / 2);
        }
        if cli.total_concurrency == 16 {
            // default value
            opts.total_concurrency = cores;
        }
        if cli.batch_size == 64 { // default value
             // Keep as-is; users can raise to 96/128 on beefier CPUs
        }
    }
    if cli.input.is_dir() {
        let ok =
            nvs_embed::embed_chunks_dir(backend.clone(), &cli.input, &cli.output, &opts).await?;
        println!("Processed {} files into {}", ok, cli.output.display());
    } else if cli.input.is_file() {
        let out = if cli.output.is_dir() {
            cli.output.join("out.docs.json")
        } else {
            cli.output.clone()
        };
        nvs_embed::embed_chunks_file(backend.clone(), &cli.input, &out, &opts).await?;
        println!("{} -> {}", cli.input.display(), out.display());
    } else {
        anyhow::bail!("input path not found: {}", cli.input.display());
    }
    Ok(())
}
