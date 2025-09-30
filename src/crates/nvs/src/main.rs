use anyhow::{anyhow, Context, Result};
use clap::Parser;
use console::style;
use crossbeam_channel as chan;
use futures::{stream, StreamExt};
use indicatif::{ProgressBar, ProgressStyle};
use memmap2::Mmap;
use serde::{Deserialize, Serialize};
use serde_json::{Map as JsonMap, Value as JsonValue};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex as AsyncMutex;
use walkdir::WalkDir;
use xxhash_rust::xxh64::xxh64;

static DEFAULT_WORKDIR: &str = ".nvs-work";
static DEFAULT_BUNDLE_DIRNAME: &str = ".nvs-bundle";
static DEFAULT_MODEL: &str = "text-embedding-3-small";

#[derive(Parser, Debug)]
#[command(name = "nvs")]
#[command(about = "Unified CLI for Native Vector Store (Autopilot: nvs <INPUT>)", long_about = None
)]
struct Cli {
    /// Input directory or file (Autopilot mode). Use subcommands for fine control.
    input: Option<PathBuf>,

    /// Output bundle directory (defaults to <INPUT>/.nvs-bundle when using Autopilot)
    #[arg(short = 'o', long = "out")]
    out: Option<PathBuf>,

    /// Working directory for intermediates (chunks/docs/receipts/logs)
    #[arg(long = "work", default_value = DEFAULT_WORKDIR)]
    work: PathBuf,

    /// Resume from receipts (skip unchanged inputs)
    #[arg(long = "resume", default_value_t = true)]
    resume: bool,

    /// Force reprocessing (ignore receipts)
    #[arg(long = "force", default_value_t = false)]
    force: bool,

    /// Quantization for vectors: f32|f16
    #[arg(long = "quantize", default_value = "f32")]
    quantize: String,

    /// Compress metadata blocks: none|zstd (default zstd)
    #[arg(long = "compress", default_value = "zstd")]
    compress: String,

    /// Embedding model name for Autopilot (OpenAI). Requires OPENAI_API_KEY.
    #[arg(long = "model", default_value = DEFAULT_MODEL)]
    model: String,

    /// Emit only a final JSON summary
    #[arg(long = "json", default_value_t = false)]
    json: bool,
}

#[derive(Debug, Serialize)]
struct StageSummary {
    name: String,
    processed: usize,
    skipped: usize,
    ms: u128,
}

#[derive(Debug)]
struct StageOutcome {
    total: usize,
    changed: usize,
    processed: usize,
    cached: usize,
    failures: usize,
}

impl StageOutcome {
    fn skipped(&self) -> usize {
        self.cached + self.failures
    }
}

#[derive(Debug)]
struct StageResult {
    outcome: StageOutcome,
    duration_ms: u128,
}

#[derive(Debug)]
struct DocData {
    text: String,
    chunk_meta: JsonValue,
    embedding: Option<Vec<f32>>,
}

#[derive(Debug)]
struct LoadedFileDocs {
    input: PathBuf,
    output: PathBuf,
    docs: Vec<DocData>,
    parse_error: Option<anyhow::Error>,
}

#[derive(Debug)]
enum BufData {
    Vec(Vec<u8>),
    Mmap(Mmap),
}

fn stage_header(name: &str, detail: &str) {
    eprintln!("{}", style(format!("==> {name}: {detail}")).cyan().bold());
}

fn stage_footer(name: &str, outcome: &StageOutcome, duration_ms: u128) {
    let secs = (duration_ms as f64) / 1000.0;
    eprintln!(
        "    {} processed {}/{} (changed {}, cached {}, failures {}) in {:.2}s",
        style(name).bold(),
        outcome.processed,
        outcome.total,
        outcome.changed,
        outcome.cached,
        outcome.failures,
        secs
    );
}

fn progress_bar(total: usize, label: &str) -> Option<ProgressBar> {
    if total == 0 {
        None
    } else {
        let pb = ProgressBar::new(total as u64);
        pb.set_style(ProgressStyle::with_template("{spinner:.green} {msg} {pos}/{len}").unwrap());
        pb.set_message(label.to_string());
        pb.enable_steady_tick(std::time::Duration::from_millis(80));
        Some(pb)
    }
}

#[derive(Clone)]
struct ReceiptEntry {
    hash: u64,
    size: u64,
    mtime: String,
    kind: String,
    status: String,
}

#[derive(Debug, Serialize)]
struct FinalSummary {
    stages: Vec<StageSummary>,
    bundle_out: String,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    if let Some(input) = cli.input.clone() {
        autopilot(input, &cli)
    } else {
        print_help_hint();
        Ok(())
    }
}

fn print_help_hint() {
    eprintln!(
        "{} usage: nvs <INPUT_DIR> [--out BUNDLE_DIR] [--work DIR] [--resume|--force]",
        style("info").blue()
    );
}

fn ensure_dir(p: &Path) -> Result<()> {
    fs::create_dir_all(p).with_context(|| format!("create {}", p.display()))
}

fn autopilot(input: PathBuf, cli: &Cli) -> Result<()> {
    anyhow::ensure!(input.exists(), "input path not found: {}", input.display());

    let work_dir = cli.work.clone();
    ensure_dir(&work_dir)?;
    let chunks_dir = work_dir.join("chunks");
    ensure_dir(&chunks_dir)?;
    let docs_dir = work_dir.join("docs");
    ensure_dir(&docs_dir)?;
    let receipts_dir = work_dir.join("receipts");
    ensure_dir(&receipts_dir)?;

    let out_dir = if let Some(ref o) = cli.out {
        o.clone()
    } else {
        if input.is_dir() {
            input.join(DEFAULT_BUNDLE_DIRNAME)
        } else {
            PathBuf::from(DEFAULT_BUNDLE_DIRNAME)
        }
    };
    ensure_dir(&out_dir)?;

    // Discover files
    let mut pdfs = Vec::new();
    let mut htmls = Vec::new();
    if input.is_file() {
        if is_pdf(&input) {
            pdfs.push(input.clone());
        } else if is_html(&input) {
            htmls.push(input.clone());
        }
    } else {
        for entry in WalkDir::new(&input).into_iter().filter_map(|e| e.ok()) {
            if !entry.file_type().is_file() {
                continue;
            }
            let p = entry.path().to_path_buf();
            if is_pdf(&p) {
                pdfs.push(p);
            } else if is_html(&p) {
                htmls.push(p);
            }
        }
    }

    if pdfs.is_empty() && htmls.is_empty() {
        // Allow running directly on docs (skip chunk+embed)
        if looks_like_docs_dir(&input) {
            eprintln!("{} detected docs; skipping chunk+embed", style("·").dim());
        } else {
            return Err(anyhow!("no PDFs/HTML/docs found under {}", input.display()));
        }
    }

    let mut stages: Vec<StageSummary> = Vec::new();

    // Stage 1: chunk PDFs via Rust fast path
    if !pdfs.is_empty() {
        let result = chunk_pdfs_rust(&pdfs, &chunks_dir, cli)?;
        stages.push(StageSummary {
            name: "chunk-pdf".into(),
            processed: result.outcome.processed,
            skipped: result.outcome.skipped(),
            ms: result.duration_ms,
        });
    }

    // Stage 2: chunk HTML
    if !htmls.is_empty() {
        let result = chunk_html(&htmls, &chunks_dir, cli)?;
        stages.push(StageSummary {
            name: "chunk-html".into(),
            processed: result.outcome.processed,
            skipped: result.outcome.skipped(),
            ms: result.duration_ms,
        });
    }

    // Stage 3: embed (OpenAI)
    // Detect if chunks exist; if none and input looks like docs, skip embed
    let chunks_exist = fs::read_dir(&chunks_dir)
        .ok()
        .map(|mut it| it.next().is_some())
        .unwrap_or(false);
    let docs_input_dir = if chunks_exist { &chunks_dir } else { &input };
    let embedded_docs_dir = if chunks_exist {
        Some(docs_dir.as_path())
    } else {
        None
    };
    if chunks_exist {
        let result = embed_openai_dir(docs_input_dir, &docs_dir, cli)?;
        stages.push(StageSummary {
            name: "embed".into(),
            processed: result.outcome.processed,
            skipped: result.outcome.skipped(),
            ms: result.duration_ms,
        });
    }

    // Stage 4: pack (zstd default)
    let pack_input_dir = if let Some(_) = embedded_docs_dir {
        docs_dir.as_path()
    } else {
        input.as_path()
    };
    stage_header(
        "Pack",
        &format!("quantize={}, compress={}", cli.quantize, cli.compress),
    );
    let spinner_pack = spinner("Writing bundle");
    let t0 = Instant::now();
    let (docs_count, dim) = pack_bundle(
        &pack_input_dir,
        &out_dir,
        &cli.quantize,
        &cli.compress,
        &cli.model,
    )?;
    spinner_pack.finish_and_clear();
    let dtype = if cli.quantize.eq_ignore_ascii_case("f16") {
        "f16"
    } else {
        "f32"
    };
    let ms_pack = t0.elapsed().as_millis();
    eprintln!(
        "    wrote {} docs (dim {}, dtype {}, compression {}) in {:.2}s",
        docs_count,
        dim,
        dtype,
        cli.compress,
        (ms_pack as f64) / 1000.0
    );
    stages.push(StageSummary {
        name: "pack".into(),
        processed: docs_count,
        skipped: 0,
        ms: ms_pack,
    });

    // Stage 5: verify quick open
    stage_header("Verify", &out_dir.display().to_string());
    let spinner_verify = spinner("Opening bundle");
    let t0v = Instant::now();
    let _ = nvs_core::Bundle::open(&out_dir).context("open bundle for verify")?;
    spinner_verify.finish_and_clear();
    let ms_v = t0v.elapsed().as_millis();
    eprintln!("    bundle verified in {:.2}s", (ms_v as f64) / 1000.0);
    stages.push(StageSummary {
        name: "verify".into(),
        processed: 1,
        skipped: 0,
        ms: ms_v,
    });

    let summary = FinalSummary {
        stages,
        bundle_out: out_dir.display().to_string(),
    };
    if cli.json {
        println!("{}", serde_json::to_string_pretty(&summary)?);
    } else {
        eprintln!(
            "{} Bundle created at {}",
            style("✔").green(),
            style(&summary.bundle_out).bold()
        );
        for s in &summary.stages {
            eprintln!(
                "  {:<12} processed={} skipped={} time={:.2}s",
                s.name,
                s.processed,
                s.skipped,
                (s.ms as f64) / 1000.0
            );
        }
    }
    Ok(())
}

fn is_pdf(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .map(|s| s.eq_ignore_ascii_case("pdf"))
        .unwrap_or(false)
}
fn is_html(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .map(|s| matches!(s.to_ascii_lowercase().as_str(), "html" | "htm"))
        .unwrap_or(false)
}

fn looks_like_docs_dir(dir: &Path) -> bool {
    if !dir.is_dir() {
        return false;
    }
    for entry in WalkDir::new(dir)
        .max_depth(1)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_file()
            && entry.path().extension().and_then(|e| e.to_str()) == Some("json")
        {
            // peek first few bytes
            if let Ok(mut f) = fs::File::open(entry.path()) {
                let mut buf = [0u8; 1];
                if f.read(&mut buf).ok() == Some(1) {
                    if buf[0] == b'{' || buf[0] == b'[' {
                        return true;
                    }
                }
            }
        }
    }
    false
}

fn chunk_pdfs_rust(pdfs: &[PathBuf], out_dir: &Path, cli: &Cli) -> Result<StageResult> {
    use nvs_pdf::orchestrator::process_dir_rust;
    ensure_dir(out_dir)?;
    let total = pdfs.len();
    let receipt_path = out_dir
        .parent()
        .unwrap_or(out_dir)
        .join("receipts")
        .join("chunk-pdf.tsv");
    let mut receipt_map = load_receipt(&receipt_path)?;
    let to_process = select_files(pdfs, &receipt_map, cli)?;
    let changed = to_process.len();
    let cached = total.saturating_sub(changed);
    stage_header(
        "Chunk PDF",
        &format!("{total} file(s) — {changed} changed, {cached} cached (src backend)"),
    );
    let start = Instant::now();
    let mut failures = 0usize;
    if changed > 0 {
        let spinner = spinner("Processing PDFs (src fast path)");
        let (_agg, fail_count) = process_dir_rust(
            to_process.clone(),
            out_dir.to_path_buf(),
            nvs_pdf::PdfChunkOptions::default(),
            num_cpus::get().max(1),
        );
        spinner.finish_and_clear();
        failures = fail_count;
        if failures > 0 {
            eprintln!(
                "{} {} PDFs failed during chunking",
                style("! ").yellow(),
                failures
            );
        }
    } else {
        eprintln!("    all PDFs cached; nothing to do");
    }
    let processed = changed.saturating_sub(failures);
    let outcome = StageOutcome {
        total,
        changed,
        processed,
        cached,
        failures,
    };
    let duration_ms = start.elapsed().as_millis();
    stage_footer("Chunk PDF", &outcome, duration_ms);
    if failures == 0 {
        for p in to_process {
            set_receipt_entry(&mut receipt_map, &p, "pdf", "ok")?;
        }
        save_receipt(&receipt_path, &receipt_map)?;
    } else {
        eprintln!(
            "{} receipts left unchanged so failed files retry on next run",
            style("·").dim()
        );
    }
    Ok(StageResult {
        outcome,
        duration_ms,
    })
}

fn chunk_html(htmls: &[PathBuf], out_dir: &Path, cli: &Cli) -> Result<StageResult> {
    ensure_dir(out_dir)?;
    let total = htmls.len();
    let receipt_path = out_dir
        .parent()
        .unwrap_or(out_dir)
        .join("receipts")
        .join("chunk-html.tsv");
    let mut receipt_map = load_receipt(&receipt_path)?;
    let to_process = select_files(htmls, &receipt_map, cli)?;
    let changed = to_process.len();
    let cached = total.saturating_sub(changed);
    stage_header(
        "Chunk HTML",
        &format!("{total} file(s) — {changed} changed, {cached} cached"),
    );
    let start = Instant::now();
    let opts = nvs_html::HtmlChunkOptions {
        max_tokens: 256,
        min_tokens: 150,
        overlap_tokens: 50,
        section_limit: None,
    };
    let mut success_keys: HashSet<String> = HashSet::new();
    let mut processed = 0usize;
    let mut failures = 0usize;
    let pb = progress_bar(changed, "HTML chunks");
    for (idx, path) in to_process.iter().enumerate() {
        if let Some(ref pb) = pb {
            let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("html");
            pb.set_message(format!("{} ({}/{})", name, idx + 1, changed));
        }
        match nvs_html::parse_to_chunks_with_stats(path, &opts) {
            Ok((chunks, _stats)) => {
                let stem = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("output");
                let out = out_dir.join(format!("{}_chunks.json", stem));
                if let Some(parent) = out.parent() {
                    ensure_dir(parent)?;
                }
                nvs_html::write_chunks_json(path, &chunks, &out)?;
                processed += 1;
                success_keys.insert(path_key(path));
            }
            Err(e) => {
                failures += 1;
                eprintln!(
                    "{} failed to chunk {} — {}",
                    style("! ").yellow(),
                    path.display(),
                    e
                );
            }
        }
        if let Some(ref pb) = pb {
            pb.inc(1);
        }
    }
    if let Some(pb) = pb {
        pb.finish_and_clear();
    }

    let outcome = StageOutcome {
        total,
        changed,
        processed,
        cached,
        failures,
    };
    let duration_ms = start.elapsed().as_millis();
    stage_footer("Chunk HTML", &outcome, duration_ms);

    if changed > 0 {
        for p in &to_process {
            let key = path_key(p);
            if success_keys.contains(&key) {
                set_receipt_entry(&mut receipt_map, p, "html", "ok")?;
            } else {
                set_receipt_entry(&mut receipt_map, p, "html", "failed")?;
            }
        }
    }
    save_receipt(&receipt_path, &receipt_map)?;

    Ok(StageResult {
        outcome,
        duration_ms,
    })
}

fn embed_openai_dir(input_dir: &Path, out_dir: &Path, cli: &Cli) -> Result<StageResult> {
    let model = &cli.model;
    ensure_dir(out_dir)?;
    // Select backend via env: NVS_EMBED_BACKEND=openai|local (default local)
    let backend_choice = std::env::var("NVS_EMBED_BACKEND").unwrap_or_else(|_| "local".into());
    let backend_arc: Arc<dyn nvs_embed::EmbeddingBackend> = match backend_choice.as_str() {
        #[cfg(feature = "local-embed")]
        s if s.eq_ignore_ascii_case("local") => {
            eprintln!(
                "    {} using local CPU backend (gte-small)",
                style("·").dim()
            );
            let local = nvs_embed::LocalGTEBackendBuilder::new().build()?;
            Arc::new(local)
        }
        "local" => {
            eprintln!(
                "{} local backend requested but binary not built with 'local-embed' feature; falling back to OpenAI",
                style("! ").yellow()
            );
            let _key = std::env::var("OPENAI_API_KEY")
                .context("OPENAI_API_KEY not set; required for embedding")?;
            Arc::new(nvs_embed::OpenAIBackend::builder(model).build()?)
        }
        _ => {
            let _key = std::env::var("OPENAI_API_KEY")
                .context("OPENAI_API_KEY not set; required for embedding")?;
            Arc::new(nvs_embed::OpenAIBackend::builder(model).build()?)
        }
    };
    // Defaults optimized for fewer 429s with OpenAI; for local backend, parallelize across batches.
    let openai_defaults = nvs_embed::EmbedOptions {
        concurrency: 8,
        batch_size: 64,
        file_concurrency: 8,
        total_concurrency: 8,
    };
    let cores = num_cpus::get().max(1);
    let local_defaults = nvs_embed::EmbedOptions {
        concurrency: cores, // concurrent local batches
        batch_size: 64,
        file_concurrency: std::cmp::max(1, cores / 2),
        total_concurrency: cores,
    };
    let opts = if backend_choice.eq_ignore_ascii_case("local") {
        embed_options_from_env(local_defaults)
    } else {
        embed_options_from_env(openai_defaults)
    };
    let receipt_path = out_dir
        .parent()
        .unwrap_or(out_dir)
        .join("receipts")
        .join("embed.tsv");
    let mut receipt_map = load_receipt(&receipt_path)?;

    let chunk_files: Vec<PathBuf> = WalkDir::new(input_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_type().is_file() && e.path().extension().and_then(|s| s.to_str()) == Some("json")
        })
        .map(|e| e.path().to_path_buf())
        .collect();
    let total = chunk_files.len();
    if total == 0 {
        stage_header("Embed", "no chunk files detected; skipping");
        let outcome = StageOutcome {
            total: 0,
            changed: 0,
            processed: 0,
            cached: 0,
            failures: 0,
        };
        stage_footer("Embed", &outcome, 0);
        save_receipt(&receipt_path, &receipt_map)?;
        return Ok(StageResult {
            outcome,
            duration_ms: 0,
        });
    }

    let changed_vec = select_files(&chunk_files, &receipt_map, cli)?;
    let changed_set: HashSet<String> = changed_vec.iter().map(|p| path_key(p)).collect();
    let changed = changed_set.len();
    let cached = total.saturating_sub(changed);
    let model_label = if backend_choice.eq_ignore_ascii_case("local") {
        "local".to_string()
    } else {
        model.to_string()
    };
    stage_header(
        "Embed",
        &format!(
            "{total} chunk file(s) — {changed} changed, {cached} cached (model {model_label})"
        ),
    );
    let start = Instant::now();

    if changed == 0 {
        eprintln!("    all chunk files cached; nothing to do");
        let outcome = StageOutcome {
            total,
            changed,
            processed: 0,
            cached,
            failures: 0,
        };
        stage_footer("Embed", &outcome, 0);
        save_receipt(&receipt_path, &receipt_map)?;
        return Ok(StageResult {
            outcome,
            duration_ms: 0,
        });
    }

    let jobs: Vec<(PathBuf, PathBuf)> = chunk_files
        .iter()
        .map(|inp| {
            let rel = inp.strip_prefix(input_dir).unwrap_or(inp);
            let mut out_rel = rel
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(PathBuf::new);
            let stem = inp.file_stem().and_then(|s| s.to_str()).unwrap_or("output");
            out_rel.push(format!("{}.docs.json", stem));
            (inp.clone(), out_dir.join(out_rel))
        })
        .collect();

    let changed_jobs: Vec<(PathBuf, PathBuf)> = jobs
        .iter()
        .cloned()
        .filter(|(inp, _)| changed_set.contains(&path_key(inp)))
        .collect();
    let spinner_load = spinner("Loading and parsing chunk docs");
    let loaded = load_chunk_docs_parallel(&changed_jobs, 5_000_000)?;
    spinner_load.finish_and_clear();
    let mut files_data: Vec<LoadedFileDocs> = Vec::new();
    let mut failures = 0usize;

    for entry in loaded {
        if let Some(err) = entry.parse_error {
            failures += 1;
            eprintln!(
                "{} failed to parse {} — {}",
                style("! ").yellow(),
                entry.input.display(),
                err
            );
            set_receipt_entry(&mut receipt_map, &entry.input, "json", "failed")?;
            continue;
        }
        if entry.docs.is_empty() {
            failures += 1;
            eprintln!(
                "{} no chunks found in {}",
                style("! ").yellow(),
                entry.input.display()
            );
            set_receipt_entry(&mut receipt_map, &entry.input, "json", "failed")?;
            continue;
        }
        files_data.push(entry);
    }

    if files_data.is_empty() {
        let outcome = StageOutcome {
            total,
            changed,
            processed: 0,
            cached,
            failures,
        };
        let duration_ms = start.elapsed().as_millis();
        stage_footer("Embed", &outcome, duration_ms);
        save_receipt(&receipt_path, &receipt_map)?;
        return Ok(StageResult {
            outcome,
            duration_ms,
        });
    }

    // Debug/visibility: show queued doc count and concurrency settings.
    let total_docs: usize = files_data.iter().map(|f| f.docs.len()).sum();
    eprintln!(
        "    {} queued {} docs across {} files ({} parse failures); batch_size={}, concurrency={}, file_concurrency={}, total_concurrency={}",
        style("·").dim(),
        total_docs,
        files_data.len(),
        failures,
        opts.batch_size,
        opts.concurrency,
        opts.file_concurrency,
        opts.total_concurrency
    );

    let batch_size = std::cmp::max(1, opts.batch_size);
    let rt = tokio::runtime::Runtime::new()?;
    // Build batches (owned strings) with indices; then dispatch concurrently.
    #[derive(Debug)]
    struct Batch {
        indices: Vec<(usize, usize)>,
        texts: Vec<String>,
    }
    let mut batches: Vec<Batch> = Vec::new();
    let mut cur_idx: Vec<(usize, usize)> = Vec::with_capacity(batch_size);
    let mut cur_txt: Vec<String> = Vec::with_capacity(batch_size);
    for file_idx in 0..files_data.len() {
        let docs_len = files_data[file_idx].docs.len();
        for doc_idx in 0..docs_len {
            cur_idx.push((file_idx, doc_idx));
            cur_txt.push(files_data[file_idx].docs[doc_idx].text.clone());
            if cur_txt.len() == batch_size {
                batches.push(Batch {
                    indices: std::mem::take(&mut cur_idx),
                    texts: std::mem::take(&mut cur_txt),
                });
            }
        }
    }
    if !cur_txt.is_empty() {
        batches.push(Batch {
            indices: cur_idx,
            texts: cur_txt,
        });
    }

    eprintln!(
        "    {} dispatching {} batches with concurrency {}",
        style("·").dim(),
        batches.len(),
        opts.concurrency
    );

    let mut pb_opt = progress_bar(total_docs, &format!("Embedding ({})", model_label));
    let pb_for_async = pb_opt.as_ref().map(|p| p.clone());
    // Optional global rate limiter via env: NVS_EMBED_RPM or NVS_EMBED_RPS
    let rate_limiter = build_rate_limiter_from_env();
    if let Some(ref rl) = rate_limiter {
        let (rpm, rps) = rl.describe();
        eprintln!(
            "    {} global rate limit enabled: {} RPM (~{:.1} RPS)",
            style("·").dim(),
            rpm,
            rps
        );
    }
    let rate_for_async = rate_limiter.clone();
    let backend_for_async = backend_arc.clone();
    let concurrency = std::cmp::max(1, opts.concurrency);
    let results: Vec<(Vec<(usize, usize)>, Result<Vec<Vec<f32>>>)> = rt.block_on(async move {
        stream::iter(batches.into_iter())
            .map(move |batch| {
                let backend = backend_for_async.clone();
                let pb = pb_for_async.clone();
                let rate = rate_for_async.clone();
                async move {
                    if let Some(ref rl) = rate {
                        rl.acquire().await;
                    }
                    let slices: Vec<&str> = batch.texts.iter().map(|s| s.as_str()).collect();
                    let res = embed_batch_with_retry_async(backend, &slices).await;
                    if let Some(pb) = pb {
                        pb.inc(batch.indices.len() as u64);
                    }
                    (batch.indices, res)
                }
            })
            .buffer_unordered(concurrency)
            .collect::<Vec<_>>()
            .await
    });
    if let Some(pb) = pb_opt.take() {
        pb.finish_and_clear();
    }

    for (indices, res) in results {
        match res {
            Ok(embeddings) => {
                for ((file_idx, doc_idx), emb) in indices.iter().zip(embeddings.into_iter()) {
                    files_data[*file_idx].docs[*doc_idx].embedding = Some(emb);
                }
            }
            Err(err) => {
                eprintln!(
                    "{} batch failed ({} docs) — {}",
                    style("! ").yellow(),
                    indices.len(),
                    err
                );
            }
        }
    }

    let mut processed_files = 0usize;
    for entry in files_data.iter() {
        let success = entry.docs.iter().all(|d| d.embedding.is_some());
        if success {
            write_docs_output(&entry.output, &entry.docs)?;
            processed_files += 1;
            set_receipt_entry(&mut receipt_map, &entry.input, "json", "ok")?;
        } else {
            failures += 1;
            set_receipt_entry(&mut receipt_map, &entry.input, "json", "failed")?;
        }
    }

    let outcome = StageOutcome {
        total,
        changed,
        processed: processed_files,
        cached,
        failures,
    };
    let duration_ms = start.elapsed().as_millis();
    stage_footer("Embed", &outcome, duration_ms);
    save_receipt(&receipt_path, &receipt_map)?;

    Ok(StageResult {
        outcome,
        duration_ms,
    })
}

fn pack_bundle(
    input_dir: &Path,
    out_dir: &Path,
    quantize: &str,
    compress: &str,
    model: &str,
) -> Result<(usize, usize)> {
    use nvs_packer::bm25::write_bm25_and_terms;
    use nvs_packer::loader::{read_docs_fast, Doc};
    use nvs_packer::writer::{
        write_checksums, write_manifest, write_meta_and_index, write_vectors,
    };

    let mmap_threshold = 5_000_000usize;
    let (docs, receipts) = read_docs_fast(input_dir, mmap_threshold)?;
    anyhow::ensure!(
        !docs.is_empty(),
        "no documents found in {}",
        input_dir.display()
    );
    let dim = docs[0].embedding.len();
    // filter inconsistent dims
    let docs: Vec<Doc> = docs
        .into_iter()
        .filter(|d| d.embedding.len() == dim)
        .collect();
    anyhow::ensure!(
        !docs.is_empty(),
        "no valid documents with consistent dimension"
    );

    ensure_dir(out_dir)?;
    write_vectors(&docs, dim, out_dir, quantize)?;
    let (avgdl, _terms, _postings_entries, _total_tokens, _bm_stats) =
        write_bm25_and_terms(&docs, out_dir, 0)?;
    let block_size = 131_072usize;
    let zstd_level = 3;
    let _block_count =
        write_meta_and_index(&docs, block_size, out_dir, compress, zstd_level, false)?;
    let dtype = if quantize == "f16" { "f16" } else { "f32" };
    write_manifest(
        out_dir,
        docs.len(),
        dim,
        block_size,
        avgdl,
        model,
        dtype,
        compress,
    )?;
    write_checksums(out_dir)?;
    // pack receipts
    write_simple_receipts(out_dir, &receipts)?;
    Ok((docs.len(), dim))
}

fn write_simple_receipts(out_dir: &Path, receipts: &[(String, usize)]) -> Result<()> {
    use std::io::Write;
    let p = out_dir.join("receipts.txt");
    let mut f = fs::File::create(&p)?;
    for (name, count) in receipts {
        writeln!(f, "{}\t{}", name, count)?;
    }
    Ok(())
}

fn spinner(msg: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(ProgressStyle::with_template("{spinner:.green} {msg}").unwrap());
    pb.set_message(msg.to_string());
    pb.enable_steady_tick(std::time::Duration::from_millis(80));
    pb
}

fn file_xxh64(p: &Path) -> Result<u64> {
    let mut f = fs::File::open(p)?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf)?;
    Ok(xxh64(&buf, 0))
}

#[derive(Clone)]
struct FileInfo {
    hash: u64,
    size: u64,
    mtime: String,
}

fn gather_file_info(p: &Path) -> Result<FileInfo> {
    let meta = fs::metadata(p)?;
    let size = meta.len();
    let mtime = meta
        .modified()
        .ok()
        .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339())
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
    let hash = file_xxh64(p)?;
    Ok(FileInfo { hash, size, mtime })
}

fn load_chunk_docs_parallel(
    jobs: &[(PathBuf, PathBuf)],
    mmap_threshold: usize,
) -> Result<Vec<LoadedFileDocs>> {
    #[derive(Debug)]
    enum LoadItem {
        Data(PathBuf, PathBuf, BufData),
        LoadError(PathBuf, PathBuf, anyhow::Error),
    }

    let (tx, rx) = chan::bounded::<LoadItem>(std::cmp::min(128, std::cmp::max(1, jobs.len())));
    let jobs_clone: Vec<(PathBuf, PathBuf)> = jobs.to_vec();
    std::thread::spawn(move || {
        for (inp, out) in jobs_clone {
            let item = match load_file_buf(&inp, mmap_threshold) {
                Ok(buf) => LoadItem::Data(inp, out, buf),
                Err(e) => LoadItem::LoadError(inp, out, e),
            };
            if tx.send(item).is_err() {
                break;
            }
        }
    });

    let workers = std::cmp::max(1, num_cpus::get());
    let mut handles = Vec::new();
    for _ in 0..workers {
        let rx = rx.clone();
        handles.push(std::thread::spawn(move || {
            let mut local: Vec<LoadedFileDocs> = Vec::new();
            while let Ok(item) = rx.recv() {
                match item {
                    LoadItem::Data(inp, out, buf) => {
                        let parse_res = match buf {
                            BufData::Vec(v) => parse_chunk_bytes(&v),
                            BufData::Mmap(m) => parse_chunk_bytes(&m[..]),
                        };
                        match parse_res {
                            Ok(docs) => local.push(LoadedFileDocs {
                                input: inp,
                                output: out,
                                docs,
                                parse_error: None,
                            }),
                            Err(err) => local.push(LoadedFileDocs {
                                input: inp,
                                output: out,
                                docs: Vec::new(),
                                parse_error: Some(err),
                            }),
                        }
                    }
                    LoadItem::LoadError(inp, out, err) => {
                        local.push(LoadedFileDocs {
                            input: inp,
                            output: out,
                            docs: Vec::new(),
                            parse_error: Some(err),
                        });
                    }
                }
            }
            local
        }));
    }

    drop(rx);

    let mut results: Vec<LoadedFileDocs> = Vec::new();
    for handle in handles {
        let mut local = handle.join().unwrap();
        results.append(&mut local);
    }
    Ok(results)
}

fn load_file_buf(path: &Path, mmap_threshold: usize) -> Result<BufData> {
    let file = fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let meta = file.metadata()?;
    if meta.len() as usize > mmap_threshold {
        unsafe {
            Mmap::map(&file)
                .map(BufData::Mmap)
                .with_context(|| format!("mmap {}", path.display()))
        }
    } else {
        let buf = fs::read(path).with_context(|| format!("read {}", path.display()))?;
        Ok(BufData::Vec(buf))
    }
}

#[derive(Deserialize)]
struct ChunkRaw {
    text: String,
    #[serde(default)]
    meta: JsonValue,
}

fn parse_chunk_bytes(bytes: &[u8]) -> Result<Vec<DocData>> {
    let raw: Vec<ChunkRaw> = serde_json::from_slice(bytes).context("parse chunk array")?;
    let mut docs = Vec::with_capacity(raw.len());
    for item in raw {
        docs.push(DocData {
            text: item.text,
            chunk_meta: item.meta,
            embedding: None,
        });
    }
    Ok(docs)
}

fn load_receipt(tsv_path: &Path) -> Result<HashMap<String, ReceiptEntry>> {
    let mut map = HashMap::new();
    if !tsv_path.exists() {
        return Ok(map);
    }
    let data = fs::read_to_string(tsv_path)?;
    for line in data.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 5 {
            continue;
        }
        let hash = u64::from_str_radix(parts[1], 16).unwrap_or(0);
        let size = parts[2].parse::<u64>().unwrap_or(0);
        let mtime = parts[3].to_string();
        let kind = parts[4].to_string();
        let status = if parts.len() >= 6 {
            parts[5].to_string()
        } else {
            "ok".to_string()
        };
        map.insert(
            parts[0].to_string(),
            ReceiptEntry {
                hash,
                size,
                mtime,
                kind,
                status,
            },
        );
    }
    Ok(map)
}

fn save_receipt(tsv_path: &Path, map: &HashMap<String, ReceiptEntry>) -> Result<()> {
    use std::io::Write;
    if let Some(parent) = tsv_path.parent() {
        ensure_dir(parent)?;
    }
    let mut entries: Vec<_> = map.iter().collect();
    entries.sort_by_key(|(path, _)| *path);
    let mut f = fs::File::create(tsv_path)?;
    for (path, entry) in entries {
        writeln!(
            f,
            "{}\t{:016x}\t{}\t{}\t{}\t{}",
            path, entry.hash, entry.size, entry.mtime, entry.kind, entry.status
        )?;
    }
    Ok(())
}

fn set_receipt_entry(
    map: &mut HashMap<String, ReceiptEntry>,
    path: &Path,
    kind: &str,
    status: &str,
) -> Result<()> {
    match gather_file_info(path) {
        Ok(info) => {
            map.insert(
                path_key(path),
                ReceiptEntry {
                    hash: info.hash,
                    size: info.size,
                    mtime: info.mtime,
                    kind: kind.to_string(),
                    status: status.to_string(),
                },
            );
        }
        Err(_) => {
            map.remove(&path_key(path));
        }
    }
    Ok(())
}

fn select_files(
    files: &[PathBuf],
    receipt_map: &HashMap<String, ReceiptEntry>,
    cli: &Cli,
) -> Result<Vec<PathBuf>> {
    if cli.force || !cli.resume {
        return Ok(files.to_vec());
    }
    if receipt_map.is_empty() {
        return Ok(files.to_vec());
    }
    let mut out = Vec::new();
    for p in files {
        let key = path_key(p);
        match gather_file_info(p) {
            Ok(info) => match receipt_map.get(&key) {
                Some(entry)
                    if entry.status == "ok"
                        && entry.hash == info.hash
                        && entry.size == info.size => {}
                _ => out.push(p.clone()),
            },
            Err(_) => out.push(p.clone()),
        }
    }
    Ok(out)
}

fn path_key(p: &Path) -> String {
    p.canonicalize()
        .unwrap_or_else(|_| p.to_path_buf())
        .display()
        .to_string()
}

fn embed_batch_with_retry(
    rt: &tokio::runtime::Runtime,
    backend: Arc<dyn nvs_embed::EmbeddingBackend>,
    texts: &[&str],
) -> Result<Vec<Vec<f32>>> {
    if texts.is_empty() {
        return Ok(Vec::new());
    }
    match rt.block_on(async { backend.embed_batch(texts).await }) {
        Ok(res) => Ok(res),
        Err(err) => {
            if texts.len() == 1 {
                Err(err)
            } else {
                let mid = texts.len() / 2;
                let mut left = embed_batch_with_retry(rt, backend.clone(), &texts[..mid])?;
                let mut right = embed_batch_with_retry(rt, backend, &texts[mid..])?;
                left.append(&mut right);
                Ok(left)
            }
        }
    }
}

#[derive(Clone)]
struct RateLimiter {
    next: Arc<AsyncMutex<Instant>>,
    interval: std::time::Duration,
    rpm: u32,
}

impl RateLimiter {
    fn new_per_second(rps: f64) -> Self {
        let per = if rps <= 0.0 {
            std::time::Duration::from_secs(0)
        } else {
            std::time::Duration::from_secs_f64(1.0 / rps)
        };
        Self {
            next: Arc::new(AsyncMutex::new(Instant::now())),
            interval: per,
            rpm: (rps * 60.0) as u32,
        }
    }
    async fn acquire(&self) {
        if self.interval.as_nanos() == 0 {
            return;
        }
        let mut guard = self.next.lock().await;
        let now = Instant::now();
        let wait = if now < *guard {
            *guard - now
        } else {
            std::time::Duration::from_millis(0)
        };
        // schedule next slot
        let baseline = if now > *guard { now } else { *guard };
        *guard = baseline + self.interval;
        drop(guard);
        if wait.as_nanos() > 0 {
            tokio::time::sleep(wait).await;
        }
    }
    fn describe(&self) -> (u32, f64) {
        (self.rpm, (self.rpm as f64) / 60.0)
    }
}

fn build_rate_limiter_from_env() -> Option<Arc<RateLimiter>> {
    let rps_env = std::env::var("NVS_EMBED_RPS")
        .ok()
        .and_then(|s| s.parse::<f64>().ok());
    let rpm_env = std::env::var("NVS_EMBED_RPM")
        .ok()
        .and_then(|s| s.parse::<f64>().ok());
    let rps = if let Some(rps) = rps_env {
        Some(rps)
    } else if let Some(rpm) = rpm_env {
        Some(rpm / 60.0)
    } else {
        None
    }?;
    let rl = RateLimiter::new_per_second(rps.max(0.1));
    Some(Arc::new(rl))
}

fn embed_options_from_env(defaults: nvs_embed::EmbedOptions) -> nvs_embed::EmbedOptions {
    let batch = std::env::var("NVS_EMBED_BATCH")
        .ok()
        .and_then(|s| s.parse::<usize>().ok());
    let conc = std::env::var("NVS_EMBED_CONCURRENCY")
        .ok()
        .and_then(|s| s.parse::<usize>().ok());
    let fconc = std::env::var("NVS_EMBED_FILE_CONCURRENCY")
        .ok()
        .and_then(|s| s.parse::<usize>().ok());
    let tconc = std::env::var("NVS_EMBED_TOTAL_CONCURRENCY")
        .ok()
        .and_then(|s| s.parse::<usize>().ok());
    nvs_embed::EmbedOptions {
        batch_size: batch.unwrap_or(defaults.batch_size),
        concurrency: conc.unwrap_or(defaults.concurrency),
        file_concurrency: fconc.unwrap_or(defaults.file_concurrency),
        total_concurrency: tconc.unwrap_or(defaults.total_concurrency),
    }
}

async fn embed_batch_with_retry_async(
    backend: Arc<dyn nvs_embed::EmbeddingBackend>,
    texts: &[&str],
) -> Result<Vec<Vec<f32>>> {
    if texts.is_empty() {
        return Ok(Vec::new());
    }
    let mut out: Vec<Option<Vec<f32>>> = vec![None; texts.len()];
    let mut stack: Vec<(usize, usize)> = vec![(0, texts.len())];
    while let Some((start, len)) = stack.pop() {
        let end = start + len;
        let slice = &texts[start..end];
        match backend.embed_batch(slice).await {
            Ok(embs) => {
                for (i, e) in embs.into_iter().enumerate() {
                    out[start + i] = Some(e);
                }
            }
            Err(err) => {
                if len == 1 {
                    return Err(err);
                }
                let mid = len / 2;
                stack.push((start + mid, len - mid));
                stack.push((start, mid));
            }
        }
    }
    let mut res = Vec::with_capacity(texts.len());
    for v in out.into_iter() {
        res.push(v.context("missing embedding result after retry")?);
    }
    Ok(res)
}

#[derive(Serialize)]
struct OutputDoc {
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    text: String,
    metadata: JsonValue,
}

fn write_docs_output(path: &Path, docs: &[DocData]) -> Result<()> {
    use std::io::Write;
    let mut arr = Vec::with_capacity(docs.len());
    for doc in docs {
        let embedding = doc
            .embedding
            .as_ref()
            .context("missing embedding during write")?;
        let mut meta_map = JsonMap::new();
        meta_map.insert(
            "embedding".into(),
            JsonValue::Array(embedding.iter().map(|f| JsonValue::from(*f)).collect()),
        );
        meta_map.insert("chunk_meta".into(), doc.chunk_meta.clone());
        arr.push(OutputDoc {
            id: None,
            text: doc.text.clone(),
            metadata: JsonValue::Object(meta_map),
        });
    }
    if let Some(parent) = path.parent() {
        ensure_dir(parent)?;
    }
    let mut f = fs::File::create(path)?;
    serde_json::to_writer_pretty(&mut f, &arr)?;
    f.write_all(b"\n")?;
    Ok(())
}
