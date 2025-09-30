use anyhow::Result;
use clap::Parser;
use nvs_embed::EmbeddingBackend;
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(Parser, Debug)]
#[command(name = "bench-local-embed")]
#[command(about = "Benchmark cold/warm latencies for local embedding backend", long_about = None)]
struct Cli {
    /// Optional local model dir with tokenizer.json, model.safetensors, config.json
    #[arg(long = "local-model-dir")]
    local_model_dir: Option<PathBuf>,
    /// HF model id (if not using local model dir)
    #[arg(long = "local-model-id", default_value = "thenlper/gte-small")]
    local_model_id: String,
    /// Maximum sequence length for local backend
    #[arg(long = "max-len", default_value_t = 256)]
    max_len: usize,
    /// Number of warm single-query runs to average
    #[arg(long = "repeats", default_value_t = 30)]
    repeats: usize,
    /// Comma-separated short queries (defaults to a 3D-printing set)
    #[arg(long = "queries")]
    queries: Option<String>,
}

fn parse_queries(input: Option<String>) -> Vec<String> {
    if let Some(s) = input {
        let v: Vec<String> = s
            .split(',')
            .map(|x| x.trim())
            .filter(|x| !x.is_empty())
            .map(|x| x.to_string())
            .collect();
        if !v.is_empty() {
            return v;
        }
    }
    // Default realistic plugin search terms (3D printing)
    vec![
        "bambu".into(),
        "slicer".into(),
        "high speed filament".into(),
        "orca slicer".into(),
        "klipper".into(),
        "prusa".into(),
        "cura".into(),
        "octoprint".into(),
        "nozzle temperature".into(),
        "PLA".into(),
        "PETG".into(),
        "support material".into(),
        "carbon fiber".into(),
        "flow calibration".into(),
    ]
}

fn fmt_dur(d: Duration) -> String {
    // Prefer milliseconds with one decimal if >= 1ms; otherwise microseconds
    if d.as_millis() >= 1 {
        format!("{:.1} ms", (d.as_nanos() as f64) / 1_000_000.0)
    } else {
        format!("{} µs", d.as_micros())
    }
}

fn percentile(sorted: &[Duration], p: f64) -> Duration {
    if sorted.is_empty() {
        return Duration::from_nanos(0);
    }
    let idx = ((p.clamp(0.0, 100.0) / 100.0) * ((sorted.len() - 1) as f64)).round() as usize;
    sorted[idx]
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    if let Some(dir) = &cli.local_model_dir {
        std::env::set_var("NVS_LOCAL_EMBED_MODEL_DIR", dir);
    }

    let queries = parse_queries(cli.queries);
    let first_query = queries.first().cloned().unwrap_or_else(|| "bambu".into());

    println!("Local embedding benchmark (gte-small, CPU)");
    println!("- queries: {}", queries.join(", "));
    println!("- repeats: {} warm single-query runs", cli.repeats);
    println!("- max_len: {}", cli.max_len);

    // Cold: model load
    let t0 = Instant::now();
    let backend = nvs_embed::LocalGTEBackendBuilder::new()
        .model_id(cli.local_model_id)
        .max_len(cli.max_len)
        .build()?;
    let t_model = t0.elapsed();

    // Cold: first single-query embed
    let t1 = Instant::now();
    let _emb = backend.embed(&first_query).await?;
    let t_first = t1.elapsed();

    // Warm: repeated single-query embeds, cycling through queries
    let mut warm_durs = Vec::with_capacity(cli.repeats);
    for i in 0..cli.repeats {
        let q = &queries[i % queries.len()];
        let ts = Instant::now();
        let _ = backend.embed(q).await?;
        warm_durs.push(ts.elapsed());
    }
    warm_durs.sort();
    let avg = if warm_durs.is_empty() {
        Duration::from_nanos(0)
    } else {
        let total: Duration = warm_durs.iter().copied().sum();
        total / (warm_durs.len() as u32)
    };

    // Warm: one batch embed of the whole set
    let tb = Instant::now();
    let _ = backend
        .embed_batch(&queries.iter().map(|s| s.as_str()).collect::<Vec<_>>())
        .await?;
    let t_batch = tb.elapsed();

    println!("");
    println!("Results:");
    println!("- model load (cold): {}", fmt_dur(t_model));
    println!("- first single embed (cold): {}", fmt_dur(t_first));
    if !warm_durs.is_empty() {
        println!(
            "- warm single embed: min {} | p50 {} | p95 {} | max {} | avg {}",
            fmt_dur(warm_durs[0]),
            fmt_dur(percentile(&warm_durs, 50.0)),
            fmt_dur(percentile(&warm_durs, 95.0)),
            fmt_dur(*warm_durs.last().unwrap()),
            fmt_dur(avg)
        );
    }
    println!(
        "- warm batch ({} queries) total: {} ({} each avg)",
        queries.len(),
        fmt_dur(t_batch),
        fmt_dur(t_batch / (queries.len() as u32))
    );

    Ok(())
}
