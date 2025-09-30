use anyhow::Result;
use clap::Parser;
use crossbeam_channel as chan;
use indicatif::{ProgressBar, ProgressStyle};
use memmap2::Mmap;
use nvs_core::chunker;
// shared chunker
use nvs_pdf_core as core_probe;
use once_cell::sync::Lazy;
#[cfg(feature = "pdfium")]
use pdfium_render::prelude::Pdfium;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Parser, Debug)]
#[command(name = "chunk-pdf-cli")]
#[command(about = "Chunk a PDF into Docling-style JSON chunks (Rust version)", long_about = None)]
struct Cli {
    /// Input PDF (single file) or directory. Required unless --worker.
    #[arg(short = 'i', long = "input")]
    input: Option<PathBuf>,
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
    /// Threads for PDF extraction (0 = auto). Default 1 for robustness under xargs.
    #[arg(long = "threads", default_value_t = 1)]
    threads: usize,
    /// Recurse into subdirectories when input is a directory
    #[arg(long = "recursive", default_value_t = true)]
    recursive: bool,
    /// Parallel file workers (0 = auto)
    #[arg(long = "file-threads", default_value_t = 0)]
    file_threads: usize,
    /// Use process-level parallelism: number of worker processes (0 = disabled)
    #[arg(long = "process-workers", default_value_t = 0)]
    process_workers: usize,
    /// Emit single-file stats as JSON to stdout (internal)
    #[arg(long = "emit-stats-json", default_value_t = false, hide = true)]
    emit_stats_json: bool,
    /// Worker mode: read lines "<input>\t<output>" from stdin and emit stats JSON per line
    #[arg(long = "worker", default_value_t = false, hide = true)]
    worker: bool,
    /// Probe PDFs (analyze filters/ToUnicode/encryption) instead of chunking
    #[arg(long = "probe", default_value_t = false)]
    probe: bool,
    /// Backend selection (auto|pdfium|src) — src is experimental fast-path
    #[arg(long = "backend", default_value = "pdfium")]
    backend: String,
    /// Emit detailed fast-path debug report (Rust backend)
    #[arg(long = "debug-fast", default_value_t = false)]
    debug_fast: bool,
}

struct FileResult {
    path: PathBuf,
    out_path: PathBuf,
    pages: usize,
    chunks: usize,
    extract_ms: u128,
    extract_bind_open_ms: u128,
    extract_pages_ms: u128,
    chunk_ms: u128,
    total_ms: u128,
    // other_ms removed (unused)
    // PDFium-only breakdown
    annotate_ms: u128,
    group_ms: u128,
    pack_ms: u128,
    overlap_ms: u128,
    merge_ms: u128,
    split_ms: u128,
    final_ms: u128,
    write_ms: u128,
    // Rust-backend breakdown
    extract_io_ms: u128,
    extract_build_ms: u128,
    extract_tree_ms: u128,
    extract_pages_ms_rust: u128,
    extract_interpret_ms: u128,
    extract_decode_ms: u128,
    extract_fonts_ms: u128,
    extract_resources_ms: u128,
    extract_streams_ms: u128,
    extract_normalize_ms: u128,
}

fn process_one(
    input: &PathBuf,
    out_dir_or_file: &PathBuf,
    opts: &nvs_pdf::PdfChunkOptions,
    backend: &str,
) -> Result<FileResult> {
    let t_total = Instant::now();
    // Try Rust fast-path when requested (auto/src); fallback to PDFium when backend=auto
    let prefer_rust = matches!(backend, "src" | "auto")
        || matches!(
            std::env::var("NVS_PDF_BACKEND").ok().as_deref(),
            Some("src") | Some("auto")
        );
    let mut pages_opt = None;
    let mut rust_breakdown: Option<nvs_pdf_core::FastExtractBreakdown> = None;
    let mut extract_ms_local: u128 = 0;
    if prefer_rust {
        if cli_debug_fast_enabled() {
            let t_fast = Instant::now();
            if let Ok((pages_opt_res, report)) =
                nvs_pdf_core::fast_extract_pages_with_debug(input, opts.page_limit)
            {
                if let Some(p) = pages_opt_res {
                    pages_opt = Some(p);
                }
                extract_ms_local = t_fast.elapsed().as_millis();
                // Write debug JSON next to output file
                if let Some(out_json_path) = derive_out_path(out_dir_or_file, input) {
                    let dbg_path = out_json_path.with_extension("fastdebug.json");
                    let _ = fs::write(
                        &dbg_path,
                        serde_json::to_string_pretty(&report).unwrap_or_else(|_| "{}".to_string()),
                    );
                }
            }
        } else {
            if let Ok((pages_opt_res, breakdown)) =
                nvs_pdf_core::fast_extract_pages_with_stats(input, opts.page_limit)
            {
                if let Some(pages) = pages_opt_res {
                    pages_opt = Some(pages);
                }
                extract_ms_local = breakdown.total_ms;
                rust_breakdown = Some(breakdown);
            }
        }
    }
    let (chunks, stats, extract_ms_local, rust_breakdown) = if let Some(pages) = pages_opt {
        // Chunk using Rust fast-path pages
        let co = chunker::ChunkOptions {
            max_tokens: opts.max_tokens,
            min_tokens: opts.min_tokens,
            overlap_tokens: opts.overlap_tokens,
        };
        let (chunks, cstats) = chunker::chunk_pages_with_stats(&pages, &GLOBAL_TOKENIZER, &co);
        let chunk_ms = cstats.total_ms;
        let total_ms = chunk_ms; // write measured separately; extract accounted separately in extract_ms_local
        let chunk_count = chunks.len();
        (
            chunks,
            nvs_pdf::ChunkStats {
                pages: pages.len(),
                chunks: chunk_count,
                extract_ms: 0,
                extract_bind_open_ms: 0,
                extract_pages_ms: 0,
                chunk_ms,
                total_ms,
                annotate_ms: cstats.annotate_ms,
                group_ms: cstats.group_ms,
                pack_ms: cstats.pack_ms,
                overlap_ms: cstats.overlap_ms,
                merge_ms: cstats.merge_ms,
                split_ms: cstats.split_ms,
                final_ms: cstats.final_ms,
            },
            extract_ms_local,
            rust_breakdown,
        )
    } else {
        if backend == "src" {
            // user forced src backend, but fast-path not supported for this file
            anyhow::bail!("src backend unsupported for this PDF (falling back disabled)");
        }
        let (chunks, stats) = nvs_pdf::parse_to_chunks_with_stats(input, opts)?;
        let extract_ms = stats.extract_ms;
        (chunks, stats, extract_ms, None)
    };
    let out_path = if out_dir_or_file.is_dir() {
        let stem = input
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("output");
        out_dir_or_file.join(format!("{}_chunks.json", stem))
    } else {
        out_dir_or_file.clone()
    };
    let tw = Instant::now();
    nvs_pdf::write_chunks_json(input, &chunks, &out_path)?;
    let write_ms = tw.elapsed().as_millis();
    let measured_total_ms = t_total.elapsed().as_millis();
    let (
        extract_io_ms,
        extract_build_ms,
        extract_tree_ms,
        extract_pages_ms_rust,
        extract_interpret_ms,
        extract_decode_ms,
        extract_fonts_ms,
        extract_resources_ms,
        extract_streams_ms,
        extract_normalize_ms,
    ) = if let Some(rb) = rust_breakdown.clone() {
        (
            rb.io_ms,
            rb.build_ms,
            rb.tree_ms,
            rb.pages_ms,
            rb.interpret_ms,
            rb.decode_ms,
            rb.fonts_ms,
            rb.resources_ms,
            rb.streams_ms,
            rb.normalize_ms,
        )
    } else {
        (0, 0, 0, 0, 0, 0, 0, 0, 0, 0)
    };
    Ok(FileResult {
        path: input.clone(),
        out_path,
        pages: stats.pages,
        chunks: chunks.len(),
        extract_ms: if stats.extract_ms == 0 {
            extract_ms_local
        } else {
            stats.extract_ms
        },
        extract_bind_open_ms: stats.extract_bind_open_ms,
        extract_pages_ms: stats.extract_pages_ms,
        chunk_ms: stats.chunk_ms,
        total_ms: measured_total_ms,

        annotate_ms: stats.annotate_ms,
        group_ms: stats.group_ms,
        pack_ms: stats.pack_ms,
        overlap_ms: stats.overlap_ms,
        merge_ms: stats.merge_ms,
        split_ms: stats.split_ms,
        final_ms: stats.final_ms,
        write_ms,
        extract_io_ms,
        extract_build_ms,
        extract_tree_ms,
        extract_pages_ms_rust,
        extract_interpret_ms,
        extract_decode_ms,
        extract_fonts_ms,
        extract_resources_ms,
        extract_streams_ms,
        extract_normalize_ms,
    })
}

fn cli_debug_fast_enabled() -> bool {
    // read from env set by CLI, since process_one doesn't have direct CLI reference here normally. We'll use an env bridge.
    std::env::var("NVS_DEBUG_FAST").ok().as_deref() == Some("1")
}

fn derive_out_path(out_dir_or_file: &PathBuf, input: &PathBuf) -> Option<PathBuf> {
    if out_dir_or_file.is_dir() {
        let stem = input.file_stem()?.to_str()?;
        Some(out_dir_or_file.join(format!("{}_chunks.json", stem)))
    } else {
        Some(out_dir_or_file.clone())
    }
}

// Global immutable tokenizer shared by all threads
static GLOBAL_TOKENIZER: Lazy<tokenmonster::GreedyTokenizer> =
    Lazy::new(|| tokenmonster::GreedyTokenizer::from_cl100k_bin());

#[cfg(feature = "pdfium")]
fn process_with_thread_state(
    pdf: &PathBuf,
    out_dir: &PathBuf,
    opts: &nvs_pdf::PdfChunkOptions,
    pdfium: &Pdfium,
) -> Result<FileResult> {
    let t_total = Instant::now();
    let (pages, estats) =
        nvs_pdf::extract::extract_text_pages_with_pdfium(pdfium, pdf, opts.page_limit)?;
    let co = chunker::ChunkOptions {
        max_tokens: opts.max_tokens,
        min_tokens: opts.min_tokens,
        overlap_tokens: opts.overlap_tokens,
    };
    let (chunks, cstats) = chunker::chunk_pages_with_stats(&pages, &GLOBAL_TOKENIZER, &co);
    let out_path = if out_dir.is_dir() {
        let stem = pdf.file_stem().and_then(|s| s.to_str()).unwrap_or("output");
        out_dir.join(format!("{}_chunks.json", stem))
    } else {
        out_dir.clone()
    };
    let tw = Instant::now();
    nvs_pdf::write_chunks_json(pdf, &chunks, &out_path)?;
    let write_ms = tw.elapsed().as_millis();
    let measured_total_ms = t_total.elapsed().as_millis();
    Ok(FileResult {
        path: pdf.clone(),
        out_path,
        pages: pages.len(),
        chunks: chunks.len(),
        extract_ms: estats.total_ms,
        extract_bind_open_ms: estats.bind_open_ms,
        extract_pages_ms: estats.pages_ms,
        chunk_ms: cstats.total_ms,
        total_ms: measured_total_ms,

        annotate_ms: cstats.annotate_ms,
        group_ms: cstats.group_ms,
        pack_ms: cstats.pack_ms,
        overlap_ms: cstats.overlap_ms,
        merge_ms: cstats.merge_ms,
        split_ms: cstats.split_ms,
        final_ms: cstats.final_ms,
        write_ms,
        extract_io_ms: 0,
        extract_build_ms: 0,
        extract_tree_ms: 0,
        extract_pages_ms_rust: 0,
        extract_interpret_ms: 0,
        extract_decode_ms: 0,
        extract_fonts_ms: 0,
        extract_resources_ms: 0,
        extract_streams_ms: 0,
        extract_normalize_ms: 0,
    })
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    // Worker mode: process jobs from stdin
    if cli.worker {
        use std::io::{BufRead, BufReader};
        let opts = nvs_pdf::PdfChunkOptions {
            max_tokens: cli.max_chunk_size,
            min_tokens: cli.min_chunk_size,
            overlap_tokens: cli.overlap,
            thread_count: 1,
            page_limit: cli.page_limit,
        };
        let stdin = std::io::stdin();
        let reader = BufReader::new(stdin.lock());
        if cli.debug_fast {
            std::env::set_var("NVS_DEBUG_FAST", "1");
        }
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let mut parts = line.splitn(2, '\t');
            let inp = parts.next().unwrap().trim();
            let out = parts.next().unwrap_or("").trim();
            let input = PathBuf::from(inp);
            let out_path = if out.is_empty() {
                let stem = input
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("output");
                input
                    .parent()
                    .unwrap_or_else(|| std::path::Path::new("."))
                    .join(format!("{}_chunks.json", stem))
            } else {
                PathBuf::from(out)
            };
            match process_one(&input, &out_path, &opts, &cli.backend) {
                Ok(res) => {
                    println!("{{\"file\":\"{}\",\"pages\":{},\"chunks\":{},\"extract_ms\":{},\"extract_bind_open_ms\":{},\"extract_pages_ms\":{},\"chunk_ms\":{},\"annotate_ms\":{},\"group_ms\":{},\"pack_ms\":{},\"overlap_ms\":{},\"merge_ms\":{},\"split_ms\":{},\"final_ms\":{},\"write_ms\":{},\"total_ms\":{},\"extract_io_ms\":{},\"extract_build_ms\":{},\"extract_tree_ms\":{},\"extract_pages_ms_rust\":{},\"extract_interpret_ms\":{},\"extract_decode_ms\":{},\"extract_fonts_ms\":{},\"extract_resources_ms\":{},\"extract_streams_ms\":{},\"extract_normalize_ms\":{}}}",
                             input.display(), res.pages, res.chunks, res.extract_ms, res.extract_bind_open_ms, res.extract_pages_ms,
                             res.chunk_ms, res.annotate_ms, res.group_ms, res.pack_ms, res.overlap_ms, res.merge_ms, res.split_ms, res.final_ms, res.write_ms, res.total_ms,
                             res.extract_io_ms, res.extract_build_ms, res.extract_tree_ms, res.extract_pages_ms_rust, res.extract_interpret_ms, res.extract_decode_ms, res.extract_fonts_ms, res.extract_resources_ms, res.extract_streams_ms, res.extract_normalize_ms);
                }
                Err(e) => {
                    eprintln!("! failed {} — {}", input.display(), e);
                    // still continue
                }
            }
        }
        return Ok(());
    }
    let opts = nvs_pdf::PdfChunkOptions {
        max_tokens: cli.max_chunk_size,
        min_tokens: cli.min_chunk_size,
        overlap_tokens: cli.overlap,
        thread_count: cli.threads,
        page_limit: cli.page_limit,
    };

    // Probe mode: summarize corpus characteristics
    if cli.probe {
        let Some(ref input_path) = cli.input else {
            anyhow::bail!("--input (file or dir) required for --probe")
        };
        let mut results = Vec::new();
        if input_path.is_file() {
            let r = core_probe::probe_path(input_path)?;
            results.push(r);
        } else if input_path.is_dir() {
            for entry in walkdir::WalkDir::new(input_path)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                if entry.file_type().is_file() {
                    if let Some(ext) = entry.path().extension().and_then(|e| e.to_str()) {
                        if ext.eq_ignore_ascii_case("pdf") {
                            if let Ok(r) = core_probe::probe_path(entry.path()) {
                                results.push(r);
                            }
                        }
                    }
                }
            }
        }
        let sum = core_probe::summarize(&results);
        println!("{}", serde_json::to_string_pretty(&sum)?);
        return Ok(());
    }

    if let Some(ref input_path) = cli.input {
        if input_path.is_file() {
            if cli.debug_fast {
                std::env::set_var("NVS_DEBUG_FAST", "1");
            }
            let out = if let Some(o) = cli.output.clone() {
                o
            } else {
                let stem = input_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("output");
                let parent = input_path
                    .parent()
                    .unwrap_or_else(|| std::path::Path::new("."));
                parent.join(format!("{}_chunks.json", stem))
            };
            let t0 = Instant::now();
            match process_one(input_path, &out, &opts, cli.backend.as_str()) {
                Ok(res) => {
                    if cli.emit_stats_json {
                        // Emit machine-readable stats for supervisor mode
                        println!("{{\"pages\":{},\"chunks\":{},\"extract_ms\":{},\"extract_bind_open_ms\":{},\"extract_pages_ms\":{},\"chunk_ms\":{},\"annotate_ms\":{},\"group_ms\":{},\"pack_ms\":{},\"overlap_ms\":{},\"merge_ms\":{},\"split_ms\":{},\"final_ms\":{},\"write_ms\":{},\"total_ms\":{}}}",
                                 res.pages, res.chunks, res.extract_ms, res.extract_bind_open_ms, res.extract_pages_ms,
                                 res.chunk_ms, res.annotate_ms, res.group_ms, res.pack_ms, res.overlap_ms, res.merge_ms, res.split_ms, res.final_ms, res.write_ms, res.total_ms);
                    } else {
                        println!(
                            "{} | pages={} chunks={} time={}ms -> {}",
                            res.path.display(),
                            res.pages,
                            res.chunks,
                            res.total_ms,
                            res.out_path.display()
                        );
                    }
                }
                Err(e) => {
                    eprintln!("Failed {} — {}", input_path.display(), e);
                    std::process::exit(1);
                }
            }
            if !cli.emit_stats_json {
                eprintln!("Done in {}ms", t0.elapsed().as_millis());
            }
            return Ok(());
        }
    }

    if let Some(ref input_path) = cli.input {
        if input_path.is_dir() {
            if cli.debug_fast {
                std::env::set_var("NVS_DEBUG_FAST", "1");
            }
            let out_dir = if let Some(o) = cli.output.clone() {
                o
            } else {
                input_path.clone()
            };
            fs::create_dir_all(&out_dir)?;
            let mut pdfs: Vec<PathBuf> = Vec::new();
            if cli.recursive {
                for entry in walkdir::WalkDir::new(input_path)
                    .into_iter()
                    .filter_map(|e| e.ok())
                {
                    if entry.file_type().is_file() {
                        if let Some(ext) = entry.path().extension().and_then(|e| e.to_str()) {
                            if ext.eq_ignore_ascii_case("pdf") {
                                pdfs.push(entry.path().to_path_buf());
                            }
                        }
                    }
                }
            } else {
                for entry in fs::read_dir(input_path)? {
                    if let Ok(e) = entry {
                        let p = e.path();
                        if p.is_file() {
                            if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                                if ext.eq_ignore_ascii_case("pdf") {
                                    pdfs.push(p);
                                }
                            }
                        }
                    }
                }
            }
            pdfs.sort();

            let n_workers = if cli.file_threads == 0 {
                num_cpus::get()
            } else {
                cli.file_threads
            };
            let total = pdfs.len() as u64;
            let pb = ProgressBar::new(total);
            pb.set_style(
                ProgressStyle::with_template(
                    "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}",
                )?
                .progress_chars("=>-"),
            );
            #[derive(Default)]
            struct Aggregates {
                pages: usize,
                chunks: usize,
                extract_ms: u128,
                chunk_ms: u128,
                total_ms: u128,
                annotate_ms: u128,
                group_ms: u128,
                pack_ms: u128,
                overlap_ms: u128,
                merge_ms: u128,
                split_ms: u128,
                final_ms: u128,
                write_ms: u128,
                bind_open_ms: u128,
                pages_extract_ms: u128,
                rust_io_ms: u128,
                rust_build_ms: u128,
                rust_tree_ms: u128,
                rust_pages_ms: u128,
                rust_interpret_ms: u128,
                rust_decode_ms: u128,
                rust_fonts_ms: u128,
                rust_resources_ms: u128,
                rust_streams_ms: u128,
                rust_normalize_ms: u128,
            }
            let agg = Arc::new(Mutex::new(Aggregates::default()));
            let failures = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let start = Instant::now();
            #[cfg(feature = "pdfium")]
            thread_local! {
                static TLS_STATE: RefCell<Option<Pdfium>> = RefCell::new(None);
            }
            // If process_workers > 0, use process-level parallelism; else fall back to thread-level
            if cli.process_workers > 0 {
                use std::io::{BufRead, BufReader, Write};
                // Spawn N persistent workers and reuse them
                let workers = cli.process_workers;
                let exe = std::env::current_exe()?;
                let mut children: Vec<(std::process::Child, std::process::ChildStdin)> = Vec::new();
                // Reader thread to collect outputs
                let (tx, rx) = std::sync::mpsc::channel::<(PathBuf, serde_json::Value)>();
                for _ in 0..workers {
                    let mut cmd = std::process::Command::new(&exe);
                    cmd.arg("--worker")
                        .arg("--threads")
                        .arg("1")
                        .arg("--max-chunk-size")
                        .arg(cli.max_chunk_size.to_string())
                        .arg("--min-chunk-size")
                        .arg(cli.min_chunk_size.to_string())
                        .arg("--overlap")
                        .arg(cli.overlap.to_string())
                        .arg("--backend")
                        .arg({
                            #[cfg(not(feature = "pdfium"))]
                            {
                                String::from("src")
                            }
                            #[cfg(feature = "pdfium")]
                            {
                                cli.backend.clone()
                            }
                        });
                    if let Some(pl) = cli.page_limit {
                        cmd.arg("--page-limit").arg(pl.to_string());
                    }
                    cmd.stdin(std::process::Stdio::piped())
                        .stdout(std::process::Stdio::piped())
                        .stderr(std::process::Stdio::inherit());
                    let mut child = cmd.spawn()?;
                    let stdin = child.stdin.take().unwrap();
                    let stdout = child.stdout.take().unwrap();
                    // Spawn reader thread for this child
                    let txc = tx.clone();
                    std::thread::spawn(move || {
                        let reader = BufReader::new(stdout);
                        for line in reader.lines() {
                            if let Ok(text) = line {
                                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                                    let file = v
                                        .get("file")
                                        .and_then(|x| x.as_str())
                                        .map(PathBuf::from)
                                        .unwrap_or_default();
                                    let _ = txc.send((file, v));
                                }
                            }
                        }
                    });
                    children.push((child, stdin));
                }
                // Distribute tasks round-robin
                let mut idx = 0usize;
                for pdf in &pdfs {
                    let stem = pdf.file_stem().and_then(|s| s.to_str()).unwrap_or("output");
                    let out_path = out_dir.join(format!("{}_chunks.json", stem));
                    let line = format!("{}\t{}\n", pdf.display(), out_path.display());
                    let writer = &mut children[idx].1;
                    writer.write_all(line.as_bytes())?;
                    writer.flush()?;
                    idx = (idx + 1) % workers;
                }
                drop(tx);
                // Read results until done
                let mut received = 0u64;
                while received < total {
                    match rx.recv() {
                        Ok((_file, v)) => {
                            let pages =
                                v.get("pages").and_then(|x| x.as_u64()).unwrap_or(0) as usize;
                            let chunks_v =
                                v.get("chunks").and_then(|x| x.as_u64()).unwrap_or(0) as usize;
                            let extract_ms =
                                v.get("extract_ms").and_then(|x| x.as_u64()).unwrap_or(0) as u128;
                            let chunk_ms =
                                v.get("chunk_ms").and_then(|x| x.as_u64()).unwrap_or(0) as u128;
                            let total_ms =
                                v.get("total_ms").and_then(|x| x.as_u64()).unwrap_or(0) as u128;
                            let annotate_ms =
                                v.get("annotate_ms").and_then(|x| x.as_u64()).unwrap_or(0) as u128;
                            let group_ms =
                                v.get("group_ms").and_then(|x| x.as_u64()).unwrap_or(0) as u128;
                            let pack_ms =
                                v.get("pack_ms").and_then(|x| x.as_u64()).unwrap_or(0) as u128;
                            let overlap_ms =
                                v.get("overlap_ms").and_then(|x| x.as_u64()).unwrap_or(0) as u128;
                            let merge_ms =
                                v.get("merge_ms").and_then(|x| x.as_u64()).unwrap_or(0) as u128;
                            let split_ms =
                                v.get("split_ms").and_then(|x| x.as_u64()).unwrap_or(0) as u128;
                            let final_ms =
                                v.get("final_ms").and_then(|x| x.as_u64()).unwrap_or(0) as u128;
                            let write_ms =
                                v.get("write_ms").and_then(|x| x.as_u64()).unwrap_or(0) as u128;
                            let bind_open_ms =
                                v.get("extract_bind_open_ms")
                                    .and_then(|x| x.as_u64())
                                    .unwrap_or(0) as u128;
                            let pages_extract_ms =
                                v.get("extract_pages_ms")
                                    .and_then(|x| x.as_u64())
                                    .unwrap_or(0) as u128;
                            let rust_build_ms =
                                v.get("extract_build_ms")
                                    .and_then(|x| x.as_u64())
                                    .unwrap_or(0) as u128;
                            let rust_tree_ms =
                                v.get("extract_tree_ms")
                                    .and_then(|x| x.as_u64())
                                    .unwrap_or(0) as u128;
                            let rust_interpret_ms =
                                v.get("extract_interpret_ms")
                                    .and_then(|x| x.as_u64())
                                    .unwrap_or(0) as u128;
                            let rust_decode_ms =
                                v.get("extract_decode_ms")
                                    .and_then(|x| x.as_u64())
                                    .unwrap_or(0) as u128;
                            let rust_fonts_ms =
                                v.get("extract_fonts_ms")
                                    .and_then(|x| x.as_u64())
                                    .unwrap_or(0) as u128;
                            {
                                let mut a = agg
                                    .lock()
                                    .map_err(|_| anyhow::anyhow!("aggregation mutex poisoned"))?;
                                a.pages += pages;
                                a.chunks += chunks_v;
                                a.extract_ms += extract_ms;
                                a.chunk_ms += chunk_ms;
                                a.total_ms += total_ms;
                                a.annotate_ms += annotate_ms;
                                a.group_ms += group_ms;
                                a.pack_ms += pack_ms;
                                a.overlap_ms += overlap_ms;
                                a.merge_ms += merge_ms;
                                a.split_ms += split_ms;
                                a.final_ms += final_ms;
                                a.write_ms += write_ms;
                                a.bind_open_ms += bind_open_ms;
                                a.pages_extract_ms += pages_extract_ms;
                                a.rust_io_ms +=
                                    v.get("extract_io_ms").and_then(|x| x.as_u64()).unwrap_or(0)
                                        as u128;
                                a.rust_build_ms += rust_build_ms;
                                a.rust_tree_ms += rust_tree_ms;
                                a.rust_pages_ms +=
                                    v.get("extract_pages_ms_rust")
                                        .and_then(|x| x.as_u64())
                                        .unwrap_or(0) as u128;
                                a.rust_interpret_ms += rust_interpret_ms;
                                a.rust_decode_ms += rust_decode_ms;
                                a.rust_fonts_ms += rust_fonts_ms;
                                a.rust_resources_ms +=
                                    v.get("extract_resources_ms")
                                        .and_then(|x| x.as_u64())
                                        .unwrap_or(0) as u128;
                                a.rust_streams_ms +=
                                    v.get("extract_streams_ms")
                                        .and_then(|x| x.as_u64())
                                        .unwrap_or(0) as u128;
                                a.rust_normalize_ms +=
                                    v.get("extract_normalize_ms")
                                        .and_then(|x| x.as_u64())
                                        .unwrap_or(0) as u128;
                            }
                            received += 1;
                            pb.inc(1);
                            pb.set_message(format!(
                                "pages={} chunks={} time={}ms",
                                pages, chunks_v, total_ms
                            ));
                        }
                        Err(_) => break,
                    }
                }
                // Close workers
                for (mut child, _stdin) in children {
                    let _ = child.kill();
                    let _ = child.wait();
                }
                let failures_out = (total - received) as usize;
                // Print summary
                pb.finish_with_message("done");
                let elapsed = start.elapsed().as_secs_f64();
                let agg_final = match Arc::try_unwrap(agg) {
                    Ok(m) => m
                        .into_inner()
                        .map_err(|_| anyhow::anyhow!("aggregation mutex poisoned"))?,
                    Err(_) => unreachable!("aggregation ownership"),
                };
                println!("\nSummary:");
                println!("  PDFs: {}", total);
                println!("  Pages: {}", agg_final.pages);
                println!("  Chunks: {}", agg_final.chunks);
                if failures_out > 0 {
                    println!("  Failures: {}", failures_out);
                }
                println!("  Wall time: {:.2}s", elapsed);
                let docs_per_sec = if elapsed > 0.0 {
                    (total as f64) / elapsed
                } else {
                    0.0
                };
                println!("  Docs/sec: {:.2}", docs_per_sec);

                let sum_cpu_ms = agg_final.extract_ms + agg_final.chunk_ms + agg_final.write_ms;
                let wall_ms = (elapsed * 1000.0) as u128;
                let eff_par = if wall_ms > 0 {
                    (sum_cpu_ms as f64) / (wall_ms as f64)
                } else {
                    0.0
                };
                println!("\nAccumulated CPU ms (sum over files):");
                if cli.backend == "src" || cli.backend == "auto" {
                    println!("  Extract: {}ms (build={}ms, page_tree={}ms, interpret={}ms, decode={}ms, fonts={}ms)", agg_final.extract_ms, agg_final.rust_build_ms, agg_final.rust_tree_ms, agg_final.rust_interpret_ms, agg_final.rust_decode_ms, agg_final.rust_fonts_ms);
                } else {
                    println!(
                        "  Extract: {}ms (bind+open={}ms, pages={}ms)",
                        agg_final.extract_ms, agg_final.bind_open_ms, agg_final.pages_extract_ms
                    );
                }
                println!("  Chunk:   {}ms (annotate={}ms, group={}ms, pack={}ms, overlap={}ms, merge={}ms, split={}ms, final={}ms)",
                         agg_final.chunk_ms, agg_final.annotate_ms, agg_final.group_ms, agg_final.pack_ms, agg_final.overlap_ms, agg_final.merge_ms, agg_final.split_ms, agg_final.final_ms);
                println!("  Write:   {}ms", agg_final.write_ms);
                println!("  Sum:     {}ms", sum_cpu_ms);
                println!("  Effective parallelism: {:.2}x", eff_par);

                if eff_par > 0.0 {
                    println!("\nWall-normalized stage times (estimates):");
                    if cli.backend == "src" || cli.backend == "auto" {
                        println!("  Extract: ~{:.0}ms (io~{:.0}ms, build~{:.0}ms, page_tree~{:.0}ms, interpret~{:.0}ms, decode~{:.0}ms, fonts~{:.0}ms)",
                                 agg_final.extract_ms as f64 / eff_par,
                                 agg_final.rust_io_ms as f64 / eff_par,
                                 agg_final.rust_build_ms as f64 / eff_par,
                                 agg_final.rust_tree_ms as f64 / eff_par,
                                 agg_final.rust_interpret_ms as f64 / eff_par,
                                 agg_final.rust_decode_ms as f64 / eff_par,
                                 agg_final.rust_fonts_ms as f64 / eff_par,
                        );
                    } else {
                        println!(
                            "  Extract: ~{:.0}ms (bind+open~{:.0}ms, pages~{:.0}ms)",
                            agg_final.extract_ms as f64 / eff_par,
                            agg_final.bind_open_ms as f64 / eff_par,
                            agg_final.pages_extract_ms as f64 / eff_par
                        );
                    }
                    println!("  Chunk:   ~{:.0}ms (annotate~{:.0}ms, group~{:.0}ms, pack~{:.0}ms, overlap~{:.0}ms, merge~{:.0}ms, split~{:.0}ms, final~{:.0}ms)",
                             agg_final.chunk_ms as f64 / eff_par,
                             agg_final.annotate_ms as f64 / eff_par,
                             agg_final.group_ms as f64 / eff_par,
                             agg_final.pack_ms as f64 / eff_par,
                             agg_final.overlap_ms as f64 / eff_par,
                             agg_final.merge_ms as f64 / eff_par,
                             agg_final.split_ms as f64 / eff_par,
                             agg_final.final_ms as f64 / eff_par,
                    );
                    println!("  Write:   ~{:.0}ms", agg_final.write_ms as f64 / eff_par);
                }

                if total > 0 {
                    println!("\nAverages per doc:");
                    if cli.backend == "src" || cli.backend == "auto" {
                        println!("  Extract: {}ms (io={}ms, build={}ms, page_tree={}ms, interpret={}ms, decode={}ms, fonts={}ms)",
                                 (agg_final.extract_ms as f64 / total as f64) as u64,
                                 (agg_final.rust_io_ms as f64 / total as f64) as u64,
                                 (agg_final.rust_build_ms as f64 / total as f64) as u64,
                                 (agg_final.rust_tree_ms as f64 / total as f64) as u64,
                                 (agg_final.rust_interpret_ms as f64 / total as f64) as u64,
                                 (agg_final.rust_decode_ms as f64 / total as f64) as u64,
                                 (agg_final.rust_fonts_ms as f64 / total as f64) as u64,
                        );
                    } else {
                        println!(
                            "  Extract: {}ms (bind+open={}ms, pages={}ms)",
                            (agg_final.extract_ms as f64 / total as f64) as u64,
                            (agg_final.bind_open_ms as f64 / total as f64) as u64,
                            (agg_final.pages_extract_ms as f64 / total as f64) as u64
                        );
                    }
                    println!("  Chunk:   {}ms (annotate={}ms, group={}ms, pack={}ms, overlap={}ms, merge={}ms, split={}ms, final={}ms)",
                             (agg_final.chunk_ms as f64 / total as f64) as u64,
                             (agg_final.annotate_ms as f64 / total as f64) as u64,
                             (agg_final.group_ms as f64 / total as f64) as u64,
                             (agg_final.pack_ms as f64 / total as f64) as u64,
                             (agg_final.overlap_ms as f64 / total as f64) as u64,
                             (agg_final.merge_ms as f64 / total as f64) as u64,
                             (agg_final.split_ms as f64 / total as f64) as u64,
                             (agg_final.final_ms as f64 / total as f64) as u64,
                    );
                    println!(
                        "  Write:   {}ms",
                        (agg_final.write_ms as f64 / total as f64) as u64
                    );
                }
                return Ok(());
            } else {
                // Fast directory orchestrator for Rust backend: pre-mmap + worker queue
                if cli.backend == "src" {
                    let (tx, rx) = chan::bounded::<(PathBuf, Mmap)>(64);
                    // Producer: mmap files and enqueue
                    {
                        let pdfs_cloned = pdfs.clone();
                        let txc = tx.clone();
                        std::thread::spawn(move || {
                            for p in pdfs_cloned {
                                match fs::File::open(&p).and_then(|f| {
                                    unsafe { Mmap::map(&f) }.map_err(|e| {
                                        std::io::Error::new(std::io::ErrorKind::Other, e)
                                    })
                                }) {
                                    Ok(m) => {
                                        if txc.send((p, m)).is_err() {
                                            break;
                                        }
                                    }
                                    Err(_) => { /* skip unreadable files */ }
                                }
                            }
                            // drop tx: close channel
                        });
                    }
                    // Important: drop our sender so that channel closes when producer finishes
                    drop(tx);
                    // Workers: extract from bytes, chunk, write, aggregate
                    let mut handles = Vec::new();
                    for _ in 0..n_workers {
                        let rx = rx.clone();
                        let out_dir = out_dir.clone();
                        let opts = opts.clone();
                        let agg = agg.clone();
                        let pb = pb.clone();
                        let failures = failures.clone();
                        handles.push(std::thread::spawn(move || {
                            while let Ok((pdf_path, mmap)) = rx.recv() {
                                let (pages_opt, br) =
                                    match nvs_pdf_core::fast_extract_pages_from_bytes_with_stats(
                                        &mmap,
                                        opts.page_limit,
                                    ) {
                                        Ok(v) => v,
                                        Err(e) => {
                                            pb.println(format!(
                                                "! failed {} — {}",
                                                pdf_path.display(),
                                                e
                                            ));
                                            failures
                                                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                            continue;
                                        }
                                    };
                                let page_count = pages_opt.as_ref().map(|v| v.len()).unwrap_or(0);
                                let pages: Vec<(String, i32)> = pages_opt.unwrap_or_default();
                                let co = chunker::ChunkOptions {
                                    max_tokens: opts.max_tokens,
                                    min_tokens: opts.min_tokens,
                                    overlap_tokens: opts.overlap_tokens,
                                };
                                let (chunks, cstats) = chunker::chunk_pages_with_stats(
                                    &pages,
                                    &*GLOBAL_TOKENIZER,
                                    &co,
                                );
                                let out_path = {
                                    let stem = pdf_path
                                        .file_stem()
                                        .and_then(|s| s.to_str())
                                        .unwrap_or("output");
                                    out_dir.join(format!("{}_chunks.json", stem))
                                };
                                let tw = Instant::now();
                                let _ = nvs_pdf::write_chunks_json(&pdf_path, &chunks, &out_path);
                                let write_ms = tw.elapsed().as_millis();
                                let extract_ms = br.total_ms; // io_ms is 0 for from-bytes variant
                                let total_ms = extract_ms + cstats.total_ms + write_ms;
                                {
                                    let mut a = agg.lock().unwrap();
                                    a.pages += page_count;
                                    a.chunks += chunks.len();
                                    a.extract_ms += extract_ms;
                                    a.chunk_ms += cstats.total_ms;
                                    a.total_ms += total_ms;
                                    a.annotate_ms += cstats.annotate_ms;
                                    a.group_ms += cstats.group_ms;
                                    a.pack_ms += cstats.pack_ms;
                                    a.overlap_ms += cstats.overlap_ms;
                                    a.merge_ms += cstats.merge_ms;
                                    a.split_ms += cstats.split_ms;
                                    a.final_ms += cstats.final_ms;
                                    a.write_ms += write_ms;
                                    a.rust_io_ms += br.io_ms;
                                    a.rust_build_ms += br.build_ms;
                                    a.rust_tree_ms += br.tree_ms;
                                    a.rust_pages_ms += br.pages_ms;
                                    a.rust_interpret_ms += br.interpret_ms;
                                    a.rust_decode_ms += br.decode_ms;
                                    a.rust_fonts_ms += br.fonts_ms;
                                    a.rust_resources_ms += br.resources_ms;
                                    a.rust_streams_ms += br.streams_ms;
                                    a.rust_normalize_ms += br.normalize_ms;
                                }
                                pb.inc(1);
                                pb.set_message(format!(
                                    "{} pages={} chunks={} time~{}ms",
                                    pdf_path.file_name().and_then(|s| s.to_str()).unwrap_or("?"),
                                    page_count,
                                    chunks.len(),
                                    total_ms
                                ));
                            }
                        }));
                    }
                    for h in handles {
                        let _ = h.join();
                    }
                } else {
                    #[cfg(feature = "pdfium")]
                    ThreadPoolBuilder::new()
                        .num_threads(n_workers)
                        .start_handler(|_| {
                            TLS_STATE.with(|cell| {
                                use pdfium_render::prelude::*;
                                static GUARD: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));
                                let _g = GUARD.lock().unwrap();
                                let bundle_dir_build =
                                    option_env!("PDFIUM_BUNDLE_DIR").map(|s| s.to_string());
                                let lib_path_build =
                                    option_env!("PDFIUM_LIBRARY_PATH").map(|s| s.to_string());
                                let lib_dir_build =
                                    option_env!("PDFIUM_LIB_DIR").map(|s| s.to_string());
                                let bindings = if let Some(lib_path) = lib_path_build
                                    .or_else(|| std::env::var("PDFIUM_LIBRARY_PATH").ok())
                                {
                                    let p = std::path::Path::new(&lib_path);
                                    Pdfium::bind_to_library(p).unwrap()
                                } else if let Some(lib_dir) = lib_dir_build
                                    .or_else(|| std::env::var("PDFIUM_LIB_DIR").ok())
                                    .or(bundle_dir_build)
                                {
                                    let dir = std::path::Path::new(&lib_dir);
                                    let name = Pdfium::pdfium_platform_library_name_at_path(dir);
                                    Pdfium::bind_to_library(name).unwrap()
                                } else {
                                    Pdfium::bind_to_system_library().unwrap()
                                };
                                let pdfium = Pdfium::new(bindings);
                                *cell.borrow_mut() = Some(pdfium);
                            });
                        })
                        .build()?
                        .install(|| {
                            if cli.backend == "src" {
                                pdfs.par_iter().for_each(|pdf| {
                                    let res = process_one(pdf, &out_dir, &opts, &cli.backend);
                                    match res {
                                        Ok(res) => {
                                            {
                                                let mut a = agg.lock().unwrap();
                                                a.pages += res.pages;
                                                a.chunks += res.chunks;
                                                a.extract_ms += res.extract_ms;
                                                a.chunk_ms += res.chunk_ms;
                                                a.total_ms += res.total_ms;
                                                a.annotate_ms += res.annotate_ms;
                                                a.group_ms += res.group_ms;
                                                a.pack_ms += res.pack_ms;
                                                a.overlap_ms += res.overlap_ms;
                                                a.merge_ms += res.merge_ms;
                                                a.split_ms += res.split_ms;
                                                a.final_ms += res.final_ms;
                                                a.write_ms += res.write_ms;
                                                a.bind_open_ms += res.extract_bind_open_ms;
                                                a.pages_extract_ms += res.extract_pages_ms;
                                                a.rust_io_ms += res.extract_io_ms;
                                                a.rust_build_ms += res.extract_build_ms;
                                                a.rust_tree_ms += res.extract_tree_ms;
                                                a.rust_pages_ms += res.extract_pages_ms_rust;
                                                a.rust_interpret_ms += res.extract_interpret_ms;
                                                a.rust_decode_ms += res.extract_decode_ms;
                                                a.rust_fonts_ms += res.extract_fonts_ms;
                                                a.rust_resources_ms += res.extract_resources_ms;
                                                a.rust_streams_ms += res.extract_streams_ms;
                                                a.rust_normalize_ms += res.extract_normalize_ms;
                                            }
                                            pb.inc(1);
                                            pb.set_message(format!(
                                                "{} pages={} chunks={} time={}ms",
                                                pdf.file_name()
                                                    .and_then(|s| s.to_str())
                                                    .unwrap_or("?"),
                                                res.pages,
                                                res.chunks,
                                                res.total_ms
                                            ));
                                        }
                                        Err(e) => {
                                            pb.println(format!(
                                                "! failed {} — {}",
                                                pdf.display(),
                                                e
                                            ));
                                            pb.inc(1);
                                            failures
                                                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                        }
                                    }
                                });
                            } else {
                                pdfs.par_iter().for_each(|pdf| {
                                    let res = TLS_STATE.with(|cell| {
                                        let state = cell.borrow();
                                        let pdfium = state.as_ref().expect("thread state");
                                        process_with_thread_state(pdf, &out_dir, &opts, pdfium)
                                    });
                                    match res {
                                        Ok(res) => {
                                            {
                                                let mut a = agg.lock().unwrap();
                                                a.pages += res.pages;
                                                a.chunks += res.chunks;
                                                a.extract_ms += res.extract_ms;
                                                a.chunk_ms += res.chunk_ms;
                                                a.total_ms += res.total_ms;
                                                a.annotate_ms += res.annotate_ms;
                                                a.group_ms += res.group_ms;
                                                a.pack_ms += res.pack_ms;
                                                a.overlap_ms += res.overlap_ms;
                                                a.merge_ms += res.merge_ms;
                                                a.split_ms += res.split_ms;
                                                a.final_ms += res.final_ms;
                                                a.write_ms += res.write_ms;
                                                a.bind_open_ms += res.extract_bind_open_ms;
                                                a.pages_extract_ms += res.extract_pages_ms;
                                                a.rust_build_ms += res.extract_build_ms;
                                                a.rust_tree_ms += res.extract_tree_ms;
                                                a.rust_interpret_ms += res.extract_interpret_ms;
                                                a.rust_decode_ms += res.extract_decode_ms;
                                                a.rust_fonts_ms += res.extract_fonts_ms;
                                            }
                                            pb.inc(1);
                                            pb.set_message(format!(
                                                "{} pages={} chunks={} time={}ms",
                                                pdf.file_name()
                                                    .and_then(|s| s.to_str())
                                                    .unwrap_or("?"),
                                                res.pages,
                                                res.chunks,
                                                res.total_ms
                                            ));
                                        }
                                        Err(e) => {
                                            pb.println(format!(
                                                "! failed {} — {}",
                                                pdf.display(),
                                                e
                                            ));
                                            pb.inc(1);
                                            failures
                                                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                        }
                                    }
                                });

                                #[cfg(not(feature = "pdfium"))]
                                {
                                    // Fallback without pdfium: simple parallel src backend processing
                                    let backend = String::from("src");
                                    pdfs.par_iter().for_each(|pdf| {
                                        let res = process_one(pdf, &out_dir, &opts, &backend);
                                        match res {
                                            Ok(res) => {
                                                let mut a = agg.lock().unwrap();
                                                a.pages += res.pages;
                                                a.chunks += res.chunks;
                                                a.extract_ms += res.extract_ms;
                                                a.chunk_ms += res.chunk_ms;
                                                a.total_ms += res.total_ms;
                                                a.annotate_ms += res.annotate_ms;
                                                a.group_ms += res.group_ms;
                                                a.pack_ms += res.pack_ms;
                                                a.overlap_ms += res.overlap_ms;
                                                a.merge_ms += res.merge_ms;
                                                a.split_ms += res.split_ms;
                                                a.final_ms += res.final_ms;
                                                a.write_ms += res.write_ms;
                                                pb.inc(1);
                                                pb.set_message(format!(
                                                    "{} pages={} chunks={} time={}ms",
                                                    pdf.file_name()
                                                        .and_then(|s| s.to_str())
                                                        .unwrap_or("?"),
                                                    res.pages,
                                                    res.chunks,
                                                    res.total_ms
                                                ));
                                            }
                                            Err(e) => {
                                                pb.println(format!(
                                                    "! failed {} — {}",
                                                    pdf.display(),
                                                    e
                                                ));
                                                pb.inc(1);
                                                failures.fetch_add(
                                                    1,
                                                    std::sync::atomic::Ordering::Relaxed,
                                                );
                                            }
                                        }
                                    });
                                }
                            }
                        });
                }
            }
            pb.finish_with_message("done");
            let elapsed = start.elapsed().as_secs_f64();
            let agg_final = match Arc::try_unwrap(agg) {
                Ok(m) => m
                    .into_inner()
                    .map_err(|_| anyhow::anyhow!("aggregation mutex poisoned"))?,
                Err(_) => unreachable!("aggregation ownership"),
            };
            println!("\nSummary:");
            println!("  PDFs: {}", total);
            println!("  Pages: {}", agg_final.pages);
            println!("  Chunks: {}", agg_final.chunks);
            let failures_out = failures.load(std::sync::atomic::Ordering::Relaxed);
            if failures_out > 0 {
                println!("  Failures: {}", failures_out);
            }
            println!("  Wall time: {:.2}s", elapsed);
            let docs_per_sec = if elapsed > 0.0 {
                (total as f64) / elapsed
            } else {
                0.0
            };
            println!("  Docs/sec: {:.2}", docs_per_sec);

            // Accumulated CPU time (sum over files; can exceed wall time due to parallelism)
            // Include 'other' (per-file orchestration and overhead) if we captured it
            let mut sum_cpu_ms = agg_final.extract_ms + agg_final.chunk_ms + agg_final.write_ms;
            // Try to recompute aggregated 'other_ms' by re-running iter to collect it
            // We can't replay here; instead, we approximate 'other' from total_ms - (extract+chunk+write)
            // using the aggregated totals available.
            let other_ms_agg = if agg_final.total_ms
                > (agg_final.extract_ms + agg_final.chunk_ms + agg_final.write_ms)
            {
                agg_final.total_ms
                    - (agg_final.extract_ms + agg_final.chunk_ms + agg_final.write_ms)
            } else {
                0
            };
            sum_cpu_ms += other_ms_agg;
            let wall_ms = (elapsed * 1000.0) as u128;
            let eff_par = if wall_ms > 0 {
                (sum_cpu_ms as f64) / (wall_ms as f64)
            } else {
                0.0
            };
            println!("\nAccumulated CPU ms (sum over files):");
            if cli.backend == "src" || cli.backend == "auto" {
                let contents_ms = agg_final.rust_pages_ms.saturating_sub(
                    agg_final.rust_interpret_ms
                        + agg_final.rust_decode_ms
                        + agg_final.rust_fonts_ms,
                );
                println!("  Extract: {}ms (io={}ms, build={}ms, page_tree={}ms, contents={}ms, resources={}ms, streams={}ms, normalize={}ms, interpret={}ms, decode={}ms, fonts={}ms)",
                         agg_final.extract_ms,
                         agg_final.rust_io_ms,
                         agg_final.rust_build_ms,
                         agg_final.rust_tree_ms,
                         contents_ms,
                         agg_final.rust_resources_ms,
                         agg_final.rust_streams_ms,
                         agg_final.rust_normalize_ms,
                         agg_final.rust_interpret_ms,
                         agg_final.rust_decode_ms,
                         agg_final.rust_fonts_ms
                );
            } else {
                println!(
                    "  Extract: {}ms (bind+open={}ms, pages={}ms)",
                    agg_final.extract_ms, agg_final.bind_open_ms, agg_final.pages_extract_ms
                );
            }
            let chunk_sub_ms = agg_final.annotate_ms
                + agg_final.group_ms
                + agg_final.pack_ms
                + agg_final.overlap_ms
                + agg_final.merge_ms
                + agg_final.split_ms
                + agg_final.final_ms;
            let chunk_other_ms = agg_final.chunk_ms.saturating_sub(chunk_sub_ms);
            println!("  Chunk:   {}ms (annotate={}ms, group={}ms, pack={}ms, overlap={}ms, merge={}ms, split={}ms, final={}ms, other={}ms)",
                     agg_final.chunk_ms, agg_final.annotate_ms, agg_final.group_ms, agg_final.pack_ms, agg_final.overlap_ms, agg_final.merge_ms, agg_final.split_ms, agg_final.final_ms, chunk_other_ms);
            println!("  Write:   {}ms", agg_final.write_ms);
            println!("  Other:   {}ms (orchestration, bookkeeping)", other_ms_agg);
            println!("  Sum:     {}ms", sum_cpu_ms);
            println!("  Effective parallelism: {:.2}x", eff_par);

            // Wall-normalized estimates
            if eff_par > 0.0 {
                println!("\nWall-normalized stage times (estimates):");
                if cli.backend == "src" || cli.backend == "auto" {
                    let contents_ms = agg_final.rust_pages_ms.saturating_sub(
                        agg_final.rust_interpret_ms
                            + agg_final.rust_decode_ms
                            + agg_final.rust_fonts_ms,
                    );
                    println!("  Extract: ~{:.0}ms (io~{:.0}ms, build~{:.0}ms, page_tree~{:.0}ms, contents~{:.0}ms, resources~{:.0}ms, streams~{:.0}ms, normalize~{:.0}ms, interpret~{:.0}ms, decode~{:.0}ms, fonts~{:.0}ms)",
                             agg_final.extract_ms as f64 / eff_par,
                             agg_final.rust_io_ms as f64 / eff_par,
                             agg_final.rust_build_ms as f64 / eff_par,
                             agg_final.rust_tree_ms as f64 / eff_par,
                             contents_ms as f64 / eff_par,
                             agg_final.rust_resources_ms as f64 / eff_par,
                             agg_final.rust_streams_ms as f64 / eff_par,
                             agg_final.rust_normalize_ms as f64 / eff_par,
                             agg_final.rust_interpret_ms as f64 / eff_par,
                             agg_final.rust_decode_ms as f64 / eff_par,
                             agg_final.rust_fonts_ms as f64 / eff_par,
                    );
                } else {
                    println!(
                        "  Extract: ~{:.0}ms (bind+open~{:.0}ms, pages~{:.0}ms)",
                        agg_final.extract_ms as f64 / eff_par,
                        agg_final.bind_open_ms as f64 / eff_par,
                        agg_final.pages_extract_ms as f64 / eff_par
                    );
                }
                let chunk_other_norm =
                    agg_final.chunk_ms.saturating_sub(chunk_sub_ms) as f64 / eff_par;
                println!("  Chunk:   ~{:.0}ms (annotate~{:.0}ms, group~{:.0}ms, pack~{:.0}ms, overlap~{:.0}ms, merge~{:.0}ms, split~{:.0}ms, final~{:.0}ms, other~{:.0}ms)",
                         agg_final.chunk_ms as f64 / eff_par,
                         agg_final.annotate_ms as f64 / eff_par,
                         agg_final.group_ms as f64 / eff_par,
                         agg_final.pack_ms as f64 / eff_par,
                         agg_final.overlap_ms as f64 / eff_par,
                         agg_final.merge_ms as f64 / eff_par,
                         agg_final.split_ms as f64 / eff_par,
                         agg_final.final_ms as f64 / eff_par,
                         chunk_other_norm,
                );
                println!("  Write:   ~{:.0}ms", agg_final.write_ms as f64 / eff_par);
                println!("  Other:   ~{:.0}ms", other_ms_agg as f64 / eff_par);
            }

            // Averages per doc for quick comparisons
            if total > 0 {
                println!("\nAverages per doc:");
                if cli.backend == "src" || cli.backend == "auto" {
                    let contents_ms = agg_final.rust_pages_ms.saturating_sub(
                        agg_final.rust_interpret_ms
                            + agg_final.rust_decode_ms
                            + agg_final.rust_fonts_ms,
                    );
                    println!("  Extract: {}ms (io={}ms, build={}ms, page_tree={}ms, contents={}ms, resources={}ms, streams={}ms, normalize={}ms, interpret={}ms, decode={}ms, fonts={}ms)",
                             (agg_final.extract_ms as f64 / total as f64) as u64,
                             (agg_final.rust_io_ms as f64 / total as f64) as u64,
                             (agg_final.rust_build_ms as f64 / total as f64) as u64,
                             (agg_final.rust_tree_ms as f64 / total as f64) as u64,
                             (contents_ms as f64 / total as f64) as u64,
                             (agg_final.rust_resources_ms as f64 / total as f64) as u64,
                             (agg_final.rust_streams_ms as f64 / total as f64) as u64,
                             (agg_final.rust_normalize_ms as f64 / total as f64) as u64,
                             (agg_final.rust_interpret_ms as f64 / total as f64) as u64,
                             (agg_final.rust_decode_ms as f64 / total as f64) as u64,
                             (agg_final.rust_fonts_ms as f64 / total as f64) as u64,
                    );
                } else {
                    println!(
                        "  Extract: {}ms (bind+open={}ms, pages={}ms)",
                        (agg_final.extract_ms as f64 / total as f64) as u64,
                        (agg_final.bind_open_ms as f64 / total as f64) as u64,
                        (agg_final.pages_extract_ms as f64 / total as f64) as u64
                    );
                }
                let chunk_other_avg = (chunk_other_ms as f64 / total as f64) as u64;
                println!("  Chunk:   {}ms (annotate={}ms, group={}ms, pack={}ms, overlap={}ms, merge={}ms, split={}ms, final={}ms, other={}ms)",
                         (agg_final.chunk_ms as f64 / total as f64) as u64,
                         (agg_final.annotate_ms as f64 / total as f64) as u64,
                         (agg_final.group_ms as f64 / total as f64) as u64,
                         (agg_final.pack_ms as f64 / total as f64) as u64,
                         (agg_final.overlap_ms as f64 / total as f64) as u64,
                         (agg_final.merge_ms as f64 / total as f64) as u64,
                         (agg_final.split_ms as f64 / total as f64) as u64,
                         (agg_final.final_ms as f64 / total as f64) as u64,
                         chunk_other_avg,
                );
                println!(
                    "  Write:   {}ms",
                    (agg_final.write_ms as f64 / total as f64) as u64
                );
            }
            return Ok(());
        }
    }

    anyhow::bail!("--input is required unless --worker; provide a file or directory path");
}
