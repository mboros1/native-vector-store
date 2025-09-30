use anyhow::{Context, Result};
use clap::Parser;
use futures::stream::{self, StreamExt};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::time::Duration;
use tokio::time::sleep;

#[derive(Parser, Debug)]
#[command(name = "vsx-scrape")]
#[command(about = "Scrape Open VSX marketplace and emit a JSON dump", long_about = None)]
struct Cli {
    /// Search query string (use '*' to get many items)
    #[arg(long = "query", default_value = "*")]
    query: String,
    /// Number of search pages to fetch (each page_size items)
    #[arg(long = "pages", default_value_t = 3)]
    pages: usize,
    /// Page size per search request
    #[arg(long = "page-size", default_value_t = 100)]
    page_size: usize,
    /// Keep paging until the API returns no results (ignores --pages)
    #[arg(long = "all", default_value_t = false)]
    all: bool,
    /// When --all and query is "*", use seeded crawl over [0-9a-z] prefixes
    #[arg(long = "seeded", default_value_t = true)]
    seeded: bool,
    /// Maximum pages per seed when running with --seeded
    #[arg(long = "max-pages-per-seed", default_value_t = 10)]
    max_pages_per_seed: usize,
    /// Max concurrent detail fetches per page
    #[arg(long = "concurrency", default_value_t = 4)]
    concurrency: usize,
    /// Milliseconds to sleep between HTTP requests (politeness)
    #[arg(long = "delay-ms", default_value_t = 0)]
    delay_ms: u64,
    /// Optional output file path (otherwise prints to stdout)
    #[arg(long = "output")]
    output: Option<PathBuf>,
    /// Pretty print JSON output
    #[arg(long = "pretty", default_value_t = false)]
    pretty: bool,
    /// Emit newline-delimited full JSON detail objects (one per line)
    #[arg(long = "ndjson-full", default_value_t = false)]
    ndjson_full: bool,
    /// Maximum number of items to output (0 = unlimited)
    #[arg(long = "max-items", default_value_t = 0usize)]
    max_items: usize,
    /// Max retries on HTTP errors (429/5xx)
    #[arg(long = "retries", default_value_t = 4)]
    retries: u32,
    /// Initial backoff on retry in milliseconds
    #[arg(long = "backoff-ms", default_value_t = 250)]
    backoff_ms: u64,
    /// Maximum backoff delay in milliseconds
    #[arg(long = "max-backoff-ms", default_value_t = 4000)]
    max_backoff_ms: u64,
}

#[derive(Deserialize, Debug, Clone)]
struct SearchHit {
    name: String,      // extension name
    namespace: String, // publisher
    description: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
struct ExtensionDetail {
    name: String,
    namespace: String,
    displayName: Option<String>,
    description: Option<String>,
    downloadCount: Option<u64>,
    averageRating: Option<f32>,
}

#[derive(Deserialize, Debug, Clone)]
struct VersionInfo {
    version: String,
}

#[derive(Serialize, Debug)]
struct PluginRow {
    id: String, // "publisher.name"
    name: String,
    publisher: String,
    description: String,
    downloads: u64,
    rating: f32,
    latest_version: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let client = Client::builder()
        .user_agent("nvs-vsx-scraper/0.1")
        .timeout(Duration::from_secs(20))
        .build()?;

    let mut out: Vec<PluginRow> = Vec::new();
    let mut out_full: Vec<Value> = Vec::new();
    // For NDJSON, stream write as we go so ^C leaves partial file instead of empty
    let mut writer: Option<BufWriter<File>> = if cli.ndjson_full {
        if let Some(path) = &cli.output {
            Some(BufWriter::new(File::create(path).with_context(|| {
                format!("create output {}", path.display())
            })?))
        } else {
            None
        }
    } else {
        None
    };
    let delay = Duration::from_millis(cli.delay_ms);

    let mut seen: HashSet<String> = HashSet::new();

    // When crawling "everything", a single wildcard may not return results.
    // If user passes query "*" and --all and --seeded, iterate seeds [0-9a-z].
    let seeds: Vec<String> = if cli.all && cli.seeded && cli.query == "*" {
        let mut s = Vec::new();
        for c in '0'..='9' {
            s.push(c.to_string());
        }
        for c in 'a'..='z' {
            s.push(c.to_string());
        }
        s
    } else {
        vec![cli.query.clone()]
    };

    let mut written_count: usize = 0;
    for seed in seeds {
        let mut page: usize = 0;
        loop {
            let offset = page * cli.page_size;
            let raw: Value = get_search_json(
                &client,
                seed.as_str(),
                cli.page_size,
                offset,
                cli.retries,
                cli.backoff_ms,
                cli.max_backoff_ms,
            )
            .await
            .with_context(|| format!("decode search page offset={} seed={}", offset, seed))?;
            let hits: Vec<SearchHit> = match extract_hits(raw) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("warn: seed '{}' page {} decode error: {}", seed, page, e);
                    Vec::new()
                }
            };
            if hits.is_empty() {
                break;
            }

            let ndjson_full = cli.ndjson_full;
            let client_for_stream = client.clone();
            let mut stream = stream::iter(hits.into_iter().map(move |h| {
                let client = client_for_stream.clone();
                let delay = delay.clone();
                let retries = cli.retries;
                let b_ms = cli.backoff_ms;
                let max_b = cli.max_backoff_ms;
                async move {
                    let id = format!("{}.{}", h.namespace, h.name);
                    if delay.as_millis() > 0 {
                        sleep(delay).await;
                    }
                    let detail_url = format!("https://open-vsx.org/api/{}/{}", h.namespace, h.name);
                    let detail_val = get_url_json(&client, &detail_url, retries, b_ms, max_b)
                        .await
                        .with_context(|| format!("detail {}", id))?;
                    let detail: ExtensionDetail = serde_json::from_value(detail_val.clone())
                        .with_context(|| format!("decode detail {}", id))?;

                    if delay.as_millis() > 0 {
                        sleep(delay).await;
                    }
                    let versions_url = format!(
                        "https://open-vsx.org/api/{}/{}/versions",
                        h.namespace, h.name
                    );
                    let versions_val = get_url_json(&client, &versions_url, retries, b_ms, max_b)
                        .await
                        .unwrap_or_else(|_| Value::Array(vec![]));
                    let versions: Vec<VersionInfo> =
                        serde_json::from_value(versions_val.clone()).unwrap_or_default();
                    let latest = versions
                        .first()
                        .map(|v| v.version.clone())
                        .unwrap_or_else(|| "unknown".into());

                    let row = PluginRow {
                        id: id.clone(),
                        name: detail.displayName.clone().unwrap_or_else(|| h.name.clone()),
                        publisher: h.namespace.clone(),
                        description: detail
                            .description
                            .clone()
                            .or(h.description.clone())
                            .unwrap_or_default(),
                        downloads: detail.downloadCount.unwrap_or(0),
                        rating: detail.averageRating.unwrap_or(0.0),
                        latest_version: latest,
                    };
                    let full = if ndjson_full {
                        let mut root = match detail_val {
                            Value::Object(map) => map,
                            _ => serde_json::Map::new(),
                        };
                        root.insert("id".to_string(), Value::String(id.clone()));
                        root.insert("publisher".to_string(), Value::String(h.namespace.clone()));
                        root.insert("name_fallback".to_string(), Value::String(h.name.clone()));
                        root.insert("versions".to_string(), versions_val);
                        Some(Value::Object(root))
                    } else {
                        None
                    };
                    Ok::<_, anyhow::Error>((row, full))
                }
            }))
            .buffer_unordered(cli.concurrency);

            let mut reached_limit = false;
            while let Some(item) = stream.next().await {
                match item {
                    Ok((row, full)) => {
                        if seen.insert(row.id.clone()) {
                            // Emit according to mode
                            if cli.ndjson_full {
                                if let Some(v) = full {
                                    if let Some(w) = writer.as_mut() {
                                        let line = serde_json::to_string(&v)?;
                                        w.write_all(line.as_bytes())?;
                                        w.write_all(b"\n")?;
                                    } else {
                                        println!("{}", serde_json::to_string(&v)?);
                                    }
                                }
                            } else {
                                out.push(row);
                            }
                            written_count += 1;
                            if cli.max_items > 0 && written_count >= cli.max_items {
                                reached_limit = true;
                                break;
                            }
                        }
                    }
                    Err(e) => eprintln!("warn: {}", e),
                }
            }
            if reached_limit {
                break;
            }

            page += 1;
            if !cli.all && page >= cli.pages {
                break;
            }
            if cli.seeded && page >= cli.max_pages_per_seed {
                break;
            }
        }
        if cli.ndjson_full {
            if let Some(w) = writer.as_mut() {
                w.flush()?;
            }
        }
        if cli.max_items > 0 && written_count >= cli.max_items {
            break;
        }
    }

    // Output
    if cli.ndjson_full {
        // NDJSON of full objects (detail + versions)
        // Already streamed lines above. Nothing more to do.
    } else {
        if let Some(path) = cli.output {
            let mut f =
                File::create(&path).with_context(|| format!("create output {}", path.display()))?;
            if cli.pretty {
                let s = serde_json::to_string_pretty(&out)?;
                f.write_all(s.as_bytes())?;
            } else {
                let s = serde_json::to_string(&out)?;
                f.write_all(s.as_bytes())?;
            }
        } else {
            if cli.pretty {
                println!("{}", serde_json::to_string_pretty(&out)?);
            } else {
                println!("{}", serde_json::to_string(&out)?);
            }
        }
    }

    Ok(())
}

fn extract_hits(v: Value) -> Result<Vec<SearchHit>> {
    if v.is_array() {
        let hits: Vec<SearchHit> = serde_json::from_value(v)?;
        return Ok(hits);
    }
    if let Some(obj) = v.as_object() {
        for key in ["extensions", "items", "results", "data"] {
            if let Some(arr) = obj.get(key) {
                if arr.is_array() {
                    let hits: Vec<SearchHit> = serde_json::from_value(arr.clone())?;
                    return Ok(hits);
                }
            }
        }
        if let Some(Value::String(err)) = obj.get("error").or_else(|| obj.get("message")).cloned() {
            anyhow::bail!("api error: {}", err);
        }
        anyhow::bail!("unexpected search response object; expected array or {{extensions: []}}");
    }
    anyhow::bail!("unexpected search response type");
}

async fn get_search_json(
    client: &Client,
    query: &str,
    size: usize,
    offset: usize,
    retries: u32,
    base_ms: u64,
    max_ms: u64,
) -> Result<Value> {
    let mut attempt = 0;
    loop {
        let resp = client
            .get("https://open-vsx.org/api/-/search")
            .query(&[
                ("query", query),
                ("size", &size.to_string()),
                ("offset", &offset.to_string()),
            ])
            .send()
            .await;
        match resp {
            Ok(r) => {
                if r.status() == StatusCode::TOO_MANY_REQUESTS {
                    backoff_sleep(attempt, base_ms, max_ms, r.headers().get("retry-after")).await;
                } else if r.status().is_server_error() {
                    backoff_sleep(attempt, base_ms, max_ms, None).await;
                } else {
                    let ok = r.error_for_status()?;
                    return Ok(ok.json().await?);
                }
            }
            Err(_) => {
                backoff_sleep(attempt, base_ms, max_ms, None).await;
            }
        }
        attempt += 1;
        if attempt > retries {
            anyhow::bail!("search request failed after retries");
        }
    }
}

async fn get_url_json(
    client: &Client,
    url: &str,
    retries: u32,
    base_ms: u64,
    max_ms: u64,
) -> Result<Value> {
    let mut attempt = 0;
    loop {
        let resp = client.get(url).send().await;
        match resp {
            Ok(r) => {
                if r.status() == StatusCode::TOO_MANY_REQUESTS {
                    backoff_sleep(attempt, base_ms, max_ms, r.headers().get("retry-after")).await;
                } else if r.status().is_server_error() {
                    backoff_sleep(attempt, base_ms, max_ms, None).await;
                } else {
                    let ok = r.error_for_status()?;
                    return Ok(ok.json().await?);
                }
            }
            Err(_) => {
                backoff_sleep(attempt, base_ms, max_ms, None).await;
            }
        }
        attempt += 1;
        if attempt > retries {
            anyhow::bail!("request {} failed after retries", url);
        }
    }
}

async fn backoff_sleep(
    attempt: u32,
    base_ms: u64,
    max_ms: u64,
    retry_after: Option<&reqwest::header::HeaderValue>,
) {
    if let Some(val) = retry_after {
        if let Ok(s) = val.to_str() {
            if let Ok(secs) = s.parse::<u64>() {
                sleep(Duration::from_secs(secs)).await;
                return;
            }
        }
    }
    let shift = (attempt.min(20)) as u32;
    let pow = 1u64.checked_shl(shift).unwrap_or(u64::MAX);
    let mut delay = base_ms.saturating_mul(pow);
    if delay > max_ms {
        delay = max_ms;
    }
    sleep(Duration::from_millis(delay)).await;
}
