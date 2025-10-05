use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use nvs_core::{Bundle, VectorStore};
use nvs_embed::EmbeddingBackend;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;
use serde::Serialize;
use tokio::runtime::Runtime;

#[derive(Parser, Debug)]
#[command(
    name = "nvs-cli",
    about = "Interactive CLI for Native Vector Store bundles"
)]
struct Cli {
    /// Bundle to open at startup (works with REPL or when no subcommand provided)
    #[arg(short, long)]
    bundle: Option<PathBuf>,
    /// Override embedding model name used for vector queries
    #[arg(long = "embed-model")]
    embed_model: Option<String>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Run a single query against a bundle and exit
    Query(QueryArgs),
    /// Print bundle statistics
    Stats(StatsArgs),
    /// Start interactive REPL (default when no subcommand is provided)
    Repl(ReplArgs),
    /// Quick: embed a single chunks JSON, build a bundle, and run test queries
    Quick(QuickArgs),
}

#[derive(Args, Debug)]
struct QueryArgs {
    /// Path to the bundle directory
    #[arg(short, long)]
    bundle: Option<PathBuf>,
    /// Override embedding model for this invocation
    #[arg(long = "model")]
    embed_model: Option<String>,
    /// Text query to execute
    query: String,
    /// Number of results to return
    #[arg(short = 'k', long = "top", default_value_t = 10)]
    top_k: usize,
    /// Search mode to use
    #[arg(long, default_value = "hybrid")]
    mode: SearchMode,
    /// Vector contribution when hybrid search is selected (0.0 - 1.0)
    #[arg(long, default_value_t = 0.6)]
    weight: f32,
    /// Emit results as JSON array
    #[arg(long)]
    json: bool,
}

#[derive(Args, Debug)]
struct StatsArgs {
    /// Path to the bundle directory
    #[arg(short, long)]
    bundle: Option<PathBuf>,
}

#[derive(Args, Debug)]
struct ReplArgs {
    /// Path to the bundle directory to open before starting the REPL
    #[arg(short, long)]
    bundle: Option<PathBuf>,
    /// Override embedding model used when querying
    #[arg(long = "model")]
    embed_model: Option<String>,
}

#[derive(Args, Debug)]
struct QuickArgs {
    /// Path to a chunks JSON file (array of {text, meta})
    #[arg(long = "chunks", value_name = "FILE")]
    chunks_file: PathBuf,
    /// Output bundle directory
    #[arg(short = 'o', long = "out", default_value = ".nvs-bundle-quick")]
    out: PathBuf,
    /// Quantization for vectors
    #[arg(long = "quantize", default_value = "f32")]
    quantize: String,
    /// Compress metadata blocks
    #[arg(long = "compress", default_value = "zstd")]
    compress: String,
    /// Model name to record in manifest
    #[arg(long = "model", default_value = "gte-small-local")]
    model: String,
    /// Optional queries to run (repeatable). If none, derives keywords from the chunks file.
    #[arg(long = "query", num_args=0.., value_name = "TEXT")]
    queries: Vec<String>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum SearchMode {
    Hybrid,
    Vector,
    Bm25,
}

impl SearchMode {
    fn requires_embedding(self) -> bool {
        matches!(self, SearchMode::Hybrid | SearchMode::Vector)
    }
}

impl fmt::Display for SearchMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SearchMode::Hybrid => write!(f, "hybrid"),
            SearchMode::Vector => write!(f, "vector"),
            SearchMode::Bm25 => write!(f, "bm25"),
        }
    }
}

#[derive(Debug, Clone)]
struct SessionConfig {
    top_k: usize,
    mode: SearchMode,
    weight: f32,
    json: bool,
    embed_model: Option<String>,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            top_k: 10,
            mode: SearchMode::Hybrid,
            weight: 0.6,
            json: false,
            embed_model: None,
        }
    }
}

struct StoreState {
    path: PathBuf,
    store: VectorStore,
    bundle: Arc<Bundle>,
    embed_backend: Option<Arc<dyn EmbeddingBackend>>, // lazily constructed
    embed_model: String,
}

impl StoreState {
    fn set_embed_model(&mut self, model: String) {
        self.embed_model = model;
        self.embed_backend = None;
    }

    fn ensure_backend(&mut self) -> Result<Arc<dyn EmbeddingBackend>> {
        if let Some(b) = &self.embed_backend {
            return Ok(b.clone());
        }
        let choice = std::env::var("NVS_EMBED_BACKEND").unwrap_or_else(|_| "local".into());
        // Prefer local backend for offline use; fall back to OpenAI only when requested.
        let arc: Arc<dyn EmbeddingBackend> = if choice.eq_ignore_ascii_case("local") {
            // Use local CPU backend; model name is not required for local.
            let be = nvs_embed::LocalGTEBackendBuilder::new().build()?;
            Arc::new(be)
        } else {
            // OpenAI backend path requires a model name.
            let model = self.embed_model.trim();
            if model.is_empty() || model.eq_ignore_ascii_case("unknown") {
                return Err(anyhow!(
                    "no embedding model configured—use `.set model <name>` or pass `--embed-model`"
                ));
            }
            let be = nvs_embed::OpenAIBackend::builder(model).build()?;
            Arc::new(be)
        };
        self.embed_backend = Some(arc.clone());
        Ok(arc)
    }
}

struct Session {
    config: SessionConfig,
    store: Option<StoreState>,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            config: SessionConfig::default(),
            store: None,
        }
    }
}

#[derive(Debug, Serialize)]
struct OutputRow {
    rank: usize,
    score: f32,
    doc_id: u32,
    id: Option<String>,
    text: Option<String>,
    metadata: Option<String>,
}

fn main() -> Result<()> {
    let Cli {
        bundle: global_bundle,
        embed_model: global_model,
        command,
    } = Cli::parse();

    match command {
        Some(Commands::Query(args)) => {
            run_query_command(global_bundle.clone(), global_model.clone(), args)
        }
        Some(Commands::Stats(args)) => {
            run_stats_command(global_bundle.clone(), global_model.clone(), args)
        }
        Some(Commands::Repl(args)) => {
            run_repl_command(global_bundle.clone(), global_model.clone(), Some(args))
        }
        Some(Commands::Quick(args)) => run_quick_command(args),
        None => run_repl_command(global_bundle.clone(), global_model.clone(), None),
    }
}

fn run_quick_command(args: QuickArgs) -> Result<()> {
    use nvs_embed::{EmbedOptions, LocalGTEBackendBuilder};
    use nvs_packer::writer::{write_checksums, write_manifest, write_meta_and_index, write_vectors};
    use nvs_packer::bm25::write_bm25_and_terms;
    use nvs_packer::loader::{read_docs_fast, Doc};

    // Prepare paths
    anyhow::ensure!(args.chunks_file.exists(), "chunks file not found: {}", args.chunks_file.display());
    let work_dir = args.chunks_file.parent().unwrap_or(Path::new(".")).join(".nvs-quick");
    std::fs::create_dir_all(&work_dir)?;
    let docs_out = work_dir.join("docs.json");

    // Local embed backend and embed chunks -> docs.json
    let backend = LocalGTEBackendBuilder::new().build()?;
    let rt = Runtime::new()?;
    let opts = EmbedOptions { concurrency: num_cpus::get(), batch_size: 16, file_concurrency: 1, total_concurrency: num_cpus::get() };
    rt.block_on(nvs_embed::embed_chunks_file_quiet(Arc::new(backend.clone()), &args.chunks_file, &docs_out, &opts))?;

    // Pack bundle from docs.json in work dir
    std::fs::create_dir_all(&args.out)?;
    let (docs, _receipts) = read_docs_fast(&work_dir, 5_000_000)?;
    anyhow::ensure!(!docs.is_empty(), "no docs produced by embed stage");
    let dim = docs[0].embedding.len();
    let docs: Vec<Doc> = docs.into_iter().filter(|d| d.embedding.len() == dim).collect();
    write_vectors(&docs, dim, &args.out, &args.quantize)?;
    let (avgdl, _terms, _postings_entries, _total_tokens, _bm_stats) = write_bm25_and_terms(&docs, &args.out, 0)?;
    let block_size = 131_072usize;
    let zstd_level = 3;
    let _blocks = write_meta_and_index(&docs, block_size, &args.out, &args.compress, zstd_level, false)?;
    let dtype = if args.quantize.eq_ignore_ascii_case("f16") { "f16" } else { "f32" };
    write_manifest(&args.out, docs.len(), dim, block_size, avgdl, &args.model, dtype, &args.compress)?;
    write_checksums(&args.out)?;

    // Open and test queries
    let store = VectorStore::from_bundle(Bundle::open(&args.out)?);
    eprintln!("✔ Bundle ready at {} (docs={}, dim={})", args.out.display(), store.size(), store.dimensions());
    let queries = if args.queries.is_empty() { derive_queries_from_chunks(&args.chunks_file, 4)? } else { args.queries };
    let backend_q = LocalGTEBackendBuilder::new().build()?;
    for q in queries.iter() {
        let v = rt.block_on(backend_q.embed_batch(&[q.as_str()]))?;
        let qv = &v[0];
        let vs = store.search_vector(qv, 5);
        let bm = store.search_bm25(q, 5);
        let hy = store.search_hybrid(qv, q, 5, 0.6);
        println!("\nQuery: {}", q);
        print_hits("Vector", &store, &vs);
        print_hits("BM25", &store, &bm);
        print_hits("Hybrid", &store, &hy);
    }
    Ok(())
}

fn print_hits(label: &str, store: &VectorStore, hits: &[(u32, f32)]) {
    println!("{}:", label);
    for (rank, (id, score)) in hits.iter().enumerate() {
        if let Some(doc) = store.get_document_value(*id) {
            let title = doc.metadata.get("title").and_then(|v| v.as_str()).unwrap_or("");
            let snippet = doc.text.chars().take(80).collect::<String>();
            println!("  {:>2}. {:>5.3}  id={}  {}", rank+1, score, doc.id, title);
            println!("      {}", snippet.replace('\n', " "));
        }
    }
}

fn derive_queries_from_chunks(path: &Path, k: usize) -> Result<Vec<String>> {
    use std::io::Read;
    use nvs_core::tokenizer::{preprocess_bm25, SimpleTokenizer, TokenizerOptions, bm25_normalize_token};
    let mut s = String::new();
    std::fs::File::open(path)?.read_to_string(&mut s)?;
    let arr: Vec<serde_json::Value> = serde_json::from_str(&s)?;
    let mut buf = String::new();
    for v in arr.iter().take(8) {
        if let Some(t) = v.get("text").and_then(|x| x.as_str()) { buf.push_str(t); buf.push(' '); }
    }
    let tok = SimpleTokenizer::with_options(TokenizerOptions{ lowercase:true, split_contractions:true, remove_stopwords:true, remove_punctuation:true });
    let clean = preprocess_bm25(&buf);
    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for t in tok.split(&clean) {
        if let Some(norm) = bm25_normalize_token(&t) { *counts.entry(norm).or_insert(0) += 1; }
    }
    let mut items: Vec<(String, usize)> = counts.into_iter().collect();
    items.sort_by(|a,b| b.1.cmp(&a.1));
    Ok(items.into_iter().take(k).map(|(s,_)| s).collect())
}

fn run_query_command(
    global_bundle: Option<PathBuf>,
    global_model: Option<String>,
    mut args: QueryArgs,
) -> Result<()> {
    let bundle_path = resolve_bundle_path(args.bundle.take().or(global_bundle))?;
    let model_override = args.embed_model.take().or(global_model);
    let mut store = open_store(&bundle_path, model_override)?;
    let runtime = Runtime::new()?;
    let rows = perform_query(
        &mut store,
        QueryRequest {
            text: args.query,
            top_k: args.top_k,
            mode: args.mode,
            weight: args.weight,
        },
        &runtime,
    )?;
    print_results(&rows, args.json);
    Ok(())
}

fn run_stats_command(
    global_bundle: Option<PathBuf>,
    global_model: Option<String>,
    mut args: StatsArgs,
) -> Result<()> {
    let bundle_path = resolve_bundle_path(args.bundle.take().or(global_bundle))?;
    let store = open_store(&bundle_path, global_model)?;
    print_stats(&store);
    Ok(())
}

fn run_repl_command(
    global_bundle: Option<PathBuf>,
    global_model: Option<String>,
    args: Option<ReplArgs>,
) -> Result<()> {
    let runtime = Runtime::new()?;
    let mut session = Session::default();
    session.config.embed_model = global_model.clone();
    let mut initial_bundle = global_bundle;
    if let Some(mut repl_args) = args {
        if let Some(model) = repl_args.embed_model.take() {
            session.config.embed_model = Some(model.clone());
        }
        if let Some(path) = repl_args.bundle.take() {
            initial_bundle = Some(path);
        }
    }
    if let Some(path) = initial_bundle {
        match open_store(&path, session.config.embed_model.clone()) {
            Ok(store) => {
                print_open_banner(&store);
                session.store = Some(store);
            }
            Err(err) => {
                eprintln!("failed to open bundle {}:\n  {:#}", path.display(), err);
            }
        }
    }

    let mut editor = DefaultEditor::new()?;
    loop {
        let prompt = build_prompt(session.store.as_ref());
        match editor.readline(&prompt) {
            Ok(line) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let _ = editor.add_history_entry(trimmed);
                match handle_repl_line(trimmed, &mut session, &runtime) {
                    Ok(ReplAction::Continue) => {}
                    Ok(ReplAction::Exit) => break,
                    Err(err) => eprintln!("{err}"),
                }
            }
            Err(ReadlineError::Interrupted) => {
                // Ctrl-C
                println!("^C");
                continue;
            }
            Err(ReadlineError::Eof) => {
                println!();
                break;
            }
            Err(e) => {
                return Err(e.into());
            }
        }
    }
    Ok(())
}

fn open_store(path: &Path, model_override: Option<String>) -> Result<StoreState> {
    let store =
        VectorStore::open(path).with_context(|| format!("open bundle at {}", path.display()))?;
    let bundle = store.bundle();
    let mut model = bundle.manifest.embedding.model.clone();
    if let Some(override_model) = model_override {
        let trimmed = override_model.trim();
        if !trimmed.is_empty() {
            model = trimmed.to_string();
        }
    }
    Ok(StoreState {
        path: path.to_path_buf(),
        store,
        bundle,
        embed_backend: None,
        embed_model: model,
    })
}

fn print_open_banner(store: &StoreState) {
    let manifest = &store.bundle.manifest;
    let manifest_model = &manifest.embedding.model;
    let effective_model = store.embed_model.trim();
    if manifest_model == effective_model {
        println!(
            "Opened {} — {} docs, dim {}, model {}, dtype {}",
            store.path.display(),
            manifest.num_docs,
            manifest.dim,
            manifest_model,
            manifest.embedding.dtype
        );
    } else {
        println!(
            "Opened {} — {} docs, dim {}, model {} (override {}), dtype {}",
            store.path.display(),
            manifest.num_docs,
            manifest.dim,
            manifest_model,
            effective_model,
            manifest.embedding.dtype
        );
    }
}

fn resolve_bundle_path(path: Option<PathBuf>) -> Result<PathBuf> {
    path.ok_or_else(|| anyhow!("bundle path required"))
}

#[derive(Debug)]
struct QueryRequest {
    text: String,
    top_k: usize,
    mode: SearchMode,
    weight: f32,
}

fn perform_query(
    store_state: &mut StoreState,
    request: QueryRequest,
    runtime: &Runtime,
) -> Result<Vec<OutputRow>> {
    if request.top_k == 0 {
        return Err(anyhow!("top_k must be greater than zero"));
    }
    if request.mode.requires_embedding() && request.text.trim().is_empty() {
        return Err(anyhow!("query text cannot be empty"));
    }

    let results = match request.mode {
        SearchMode::Hybrid => {
            let backend = store_state.ensure_backend()?;
            let vector = runtime.block_on(async { backend.embed(&request.text).await })?;
            store_state
                .store
                .search_hybrid(&vector, &request.text, request.top_k, request.weight)
        }
        SearchMode::Vector => {
            let backend = store_state.ensure_backend()?;
            let vector = runtime.block_on(async { backend.embed(&request.text).await })?;
            store_state.store.search_vector(&vector, request.top_k)
        }
        SearchMode::Bm25 => store_state.store.search_bm25(&request.text, request.top_k),
    };

    let mut rows = Vec::new();
    for (idx, (doc_id, score)) in results.into_iter().enumerate() {
        if let Some((doc_identifier, text, metadata)) = store_state.store.get_document(doc_id) {
            rows.push(OutputRow {
                rank: idx + 1,
                score,
                doc_id,
                id: Some(doc_identifier),
                text: Some(text),
                metadata: Some(metadata),
            });
        } else {
            rows.push(OutputRow {
                rank: idx + 1,
                score,
                doc_id,
                id: None,
                text: None,
                metadata: None,
            });
        }
    }
    Ok(rows)
}

fn print_results(rows: &[OutputRow], json: bool) {
    if json {
        match serde_json::to_string_pretty(rows) {
            Ok(s) => println!("{}", s),
            Err(err) => eprintln!("failed to render json: {err}"),
        }
        return;
    }

    for row in rows {
        let id_display = row
            .id
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or("<no-id>");
        let snippet = row
            .text
            .as_deref()
            .map(truncate_snippet)
            .unwrap_or_else(|| "<no-text>".to_string());
        println!(
            "#{:<3} {:>7.4} {:<30} {}",
            row.rank, row.score, id_display, snippet
        );
    }
}

fn truncate_snippet(text: &str) -> String {
    let cleaned = text.replace(['\n', '\r'], " ");
    let trimmed = cleaned.trim();
    if trimmed.chars().count() <= 80 {
        return trimmed.to_string();
    }
    let snippet: String = trimmed.chars().take(80).collect();
    format!("{}…", snippet)
}

fn print_stats(store: &StoreState) {
    let manifest = &store.bundle.manifest;
    println!("Bundle: {}", store.path.display());
    println!("  Docs: {}", manifest.num_docs);
    println!("  Dimensions: {}", manifest.dim);
    println!(
        "  Embedding: {} ({})",
        manifest.embedding.model, manifest.embedding.dtype
    );
    let effective = store.embed_model.trim();
    if !effective.is_empty() && effective != manifest.embedding.model {
        println!("    override: {}", effective);
    }
    println!(
        "  Meta blocks: {} ({} bytes each)",
        store.bundle.meta_block_count, store.bundle.meta_block_size
    );
    println!(
        "  BM25: avgdl {:.2}, k1 {:.2}, b {:.2}",
        manifest.bm25.avgdl, manifest.bm25.k1, manifest.bm25.b
    );
}

enum ReplAction {
    Continue,
    Exit,
}

fn handle_repl_line(line: &str, session: &mut Session, runtime: &Runtime) -> Result<ReplAction> {
    match parse_repl_command(line)? {
        ReplCommand::Open(path) => {
            let store = open_store(&path, session.config.embed_model.clone())?;
            print_open_banner(&store);
            session.store = Some(store);
        }
        ReplCommand::Stats => {
            if let Some(store) = session.store.as_ref() {
                print_stats(store);
            } else {
                eprintln!("no bundle loaded — use .open <path>");
            }
        }
        ReplCommand::SetTopK(k) => {
            if k == 0 {
                eprintln!("top k must be greater than zero");
            } else {
                session.config.top_k = k;
                println!("top k set to {k}");
            }
        }
        ReplCommand::SetMode(mode) => {
            session.config.mode = mode;
            println!("search mode set to {mode}");
        }
        ReplCommand::SetWeight(weight) => {
            if (0.0..=1.0).contains(&weight) {
                session.config.weight = weight;
                println!("vector weight set to {weight:.2}");
            } else {
                eprintln!("weight must be between 0.0 and 1.0");
            }
        }
        ReplCommand::SetModel(model) => {
            let trimmed = model.trim();
            if trimmed.is_empty() {
                eprintln!("model name cannot be empty");
            } else {
                let value = trimmed.to_string();
                session.config.embed_model = Some(value.clone());
                if let Some(store) = session.store.as_mut() {
                    store.set_embed_model(value.clone());
                }
                println!("embedding model set to {value}");
            }
        }
        ReplCommand::SetJson(enabled) => {
            session.config.json = enabled;
            println!(
                "json output {}",
                if enabled { "enabled" } else { "disabled" }
            );
        }
        ReplCommand::Query(text) => {
            let store = session
                .store
                .as_mut()
                .ok_or_else(|| anyhow!("no bundle loaded — use .open <path>"))?;
            let rows = perform_query(
                store,
                QueryRequest {
                    text,
                    top_k: session.config.top_k,
                    mode: session.config.mode,
                    weight: session.config.weight,
                },
                runtime,
            )?;
            print_results(&rows, session.config.json);
        }
        ReplCommand::Help => {
            print_help();
        }
        ReplCommand::Exit => {
            return Ok(ReplAction::Exit);
        }
        ReplCommand::Empty => {}
    }
    Ok(ReplAction::Continue)
}

fn print_help() {
    println!("Commands:");
    println!("  .open/.load <path>  open a bundle from disk");
    println!("  .stats              display bundle statistics");
    println!("  .set k <N>          set default top-k results");
    println!("  .set mode <m>       choose search mode: hybrid, vector, bm25");
    println!("  .set weight <f>     set hybrid vector weight (0.0-1.0)");
    println!("  .set model <name>   override embedding model for queries");
    println!("  .set json on|off    toggle JSON output");
    println!("  .query <text>       run a query (also works without the prefix)");
    println!("  .help               show this help message");
    println!("  .exit               quit");
}

#[derive(Debug)]
enum ReplCommand {
    Open(PathBuf),
    Stats,
    SetTopK(usize),
    SetMode(SearchMode),
    SetWeight(f32),
    SetModel(String),
    SetJson(bool),
    Query(String),
    Help,
    Exit,
    Empty,
}

fn parse_repl_command(input: &str) -> Result<ReplCommand> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(ReplCommand::Empty);
    }
    if !trimmed.starts_with('.') {
        return Ok(ReplCommand::Query(trimmed.to_string()));
    }

    let body = trimmed.trim_start_matches('.').trim_start();
    if body.is_empty() {
        return Err(anyhow!("missing command after '.'"));
    }
    let mut parts = body.splitn(2, char::is_whitespace);
    let cmd = parts.next().unwrap();
    let rest = parts.next().unwrap_or("").trim();

    match cmd {
        "open" | "load" => {
            if rest.is_empty() {
                Err(anyhow!("usage: .open <path>"))
            } else {
                let path = PathBuf::from(strip_quotes(rest));
                Ok(ReplCommand::Open(path))
            }
        }
        "stats" => Ok(ReplCommand::Stats),
        "query" => {
            if rest.is_empty() {
                Err(anyhow!("usage: .query <text>"))
            } else {
                Ok(ReplCommand::Query(rest.to_string()))
            }
        }
        "set" => parse_set_command(rest),
        "help" => Ok(ReplCommand::Help),
        "exit" | "quit" => Ok(ReplCommand::Exit),
        other => Err(anyhow!("unknown command: .{other}")),
    }
}

fn parse_set_command(rest: &str) -> Result<ReplCommand> {
    let mut parts = rest.split_whitespace();
    let key = parts
        .next()
        .ok_or_else(|| anyhow!("usage: .set <key> <value>"))?;
    let value = parts
        .next()
        .ok_or_else(|| anyhow!("usage: .set {key} <value>"))?;

    match key {
        "k" => {
            let parsed: usize = value.parse().context("invalid number for k")?;
            Ok(ReplCommand::SetTopK(parsed))
        }
        "mode" => {
            let mode = match value.to_ascii_lowercase().as_str() {
                "hybrid" => SearchMode::Hybrid,
                "vector" => SearchMode::Vector,
                "bm25" => SearchMode::Bm25,
                other => {
                    return Err(anyhow!("invalid mode: {other}"));
                }
            };
            Ok(ReplCommand::SetMode(mode))
        }
        "weight" => {
            let parsed: f32 = value.parse().context("invalid weight value")?;
            Ok(ReplCommand::SetWeight(parsed))
        }
        "json" => {
            let enabled = matches!(value.to_ascii_lowercase().as_str(), "on" | "true" | "1");
            Ok(ReplCommand::SetJson(enabled))
        }
        "model" => Ok(ReplCommand::SetModel(strip_quotes(value))),
        other => Err(anyhow!("unknown setting: {other}")),
    }
}

fn build_prompt(store: Option<&StoreState>) -> String {
    match store {
        Some(s) => {
            let name = s
                .path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("bundle");
            format!("nvs({name})> ")
        }
        None => "nvs> ".to_string(),
    }
}

fn strip_quotes(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.len() >= 2 {
        let bytes = trimmed.as_bytes();
        if (bytes[0] == b'"' && bytes[trimmed.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[trimmed.len() - 1] == b'\'')
        {
            return trimmed[1..trimmed.len() - 1].to_string();
        }
    }
    trimmed.to_string()
}
