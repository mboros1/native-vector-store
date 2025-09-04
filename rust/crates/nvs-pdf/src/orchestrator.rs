use crate::chunker;
use crate::ChunkOptions;
use crossbeam_channel as chan;
use memmap2::Mmap;
use once_cell::sync::Lazy;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

// Aggregated metrics for a directory run (Rust backend).
#[derive(Default, Clone, Debug)]
pub struct Aggregates {
    pub pages: usize,
    pub chunks: usize,
    pub extract_ms: u128,
    pub chunk_ms: u128,
    pub total_ms: u128,
    pub annotate_ms: u128,
    pub group_ms: u128,
    pub pack_ms: u128,
    pub overlap_ms: u128,
    pub merge_ms: u128,
    pub split_ms: u128,
    pub final_ms: u128,
    pub write_ms: u128,
    // Rust-backend extract breakdown
    pub rust_io_ms: u128,
    pub rust_build_ms: u128,
    pub rust_tree_ms: u128,
    pub rust_pages_ms: u128,
    pub rust_interpret_ms: u128,
    pub rust_decode_ms: u128,
    pub rust_fonts_ms: u128,
    pub rust_resources_ms: u128,
    pub rust_streams_ms: u128,
    pub rust_normalize_ms: u128,
}

static GLOBAL_TOKENIZER: Lazy<tokenmonster::GreedyTokenizer> =
    Lazy::new(|| tokenmonster::GreedyTokenizer::from_cl100k_bin());

// Process a list of PDFs using the Rust fast-path directly from bytes with a
// pre-mmap producer and thread worker pool. Returns aggregates and failures.
pub fn process_dir_rust(
    pdfs: Vec<PathBuf>,
    out_dir: PathBuf,
    opts: ChunkOptions,
    n_workers: usize,
) -> (Aggregates, usize) {
    let (tx, rx) = chan::bounded::<(PathBuf, Mmap)>(64);
    // Producer
    {
        let pdfs_cloned = pdfs.clone();
        let txc = tx.clone();
        std::thread::spawn(move || {
            for p in pdfs_cloned {
                match std::fs::File::open(&p).and_then(|f| {
                    unsafe { Mmap::map(&f) }
                        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
                }) {
                    Ok(m) => {
                        if txc.send((p, m)).is_err() {
                            break;
                        }
                    }
                    Err(_) => {
                        // skip unreadable files
                    }
                }
            }
        });
    }
    drop(tx);

    let failures = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let agg = Arc::new(Mutex::new(Aggregates::default()));
    // Workers
    let mut handles = Vec::new();
    for _ in 0..n_workers.max(1) {
        let rx = rx.clone();
        let out_dir = out_dir.clone();
        let opts = opts.clone();
        let agg = agg.clone();
        let failures = failures.clone();
        handles.push(std::thread::spawn(move || {
            while let Ok((pdf_path, mmap)) = rx.recv() {
                let (pages_opt, br) = match nvs_pdf_core::fast_extract_pages_from_bytes_with_stats(
                    &mmap,
                    opts.page_limit,
                ) {
                    Ok(v) => v,
                    Err(_e) => {
                        failures.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        continue;
                    }
                };
                let page_count = pages_opt.as_ref().map(|v| v.len()).unwrap_or(0);
                let pages: Vec<(String, i32)> = pages_opt.unwrap_or_default();
                let (chunks, cstats) =
                    chunker::chunk_pages_with_stats(&pages, &*GLOBAL_TOKENIZER, &opts);
                // Write JSON
                let out_path = {
                    let stem = pdf_path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("output");
                    out_dir.join(format!("{}_chunks.json", stem))
                };
                let tw = std::time::Instant::now();
                let _ = crate::json::write_chunks_json(&pdf_path, &chunks, &out_path);
                let write_ms = tw.elapsed().as_millis();
                let extract_ms = br.total_ms;
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
                    a.write_ms += write_ms as u128;
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
            }
        }));
    }
    for h in handles {
        let _ = h.join();
    }
    let failures_out = failures.load(std::sync::atomic::Ordering::Relaxed);
    let agg_final = Arc::try_unwrap(agg).unwrap().into_inner().unwrap();
    (agg_final, failures_out)
}
