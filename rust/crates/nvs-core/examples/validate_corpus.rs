use nvs_core::VectorStore;
use serde::Deserialize;
use std::fs;

#[derive(Deserialize)]
struct Q {
    topic: Option<String>,
    query: String,
    embedding: Vec<f32>,
}

fn usage() {
    eprintln!("Usage: validate_corpus --bundle <bundle_dir> --queries <queries.json> [--k 5]");
}

fn parse_args() -> (String, String, usize) {
    let mut bundle = None;
    let mut queries = None;
    let mut k: usize = 5;
    let mut i = 1;
    let args: Vec<String> = std::env::args().collect();
    while i < args.len() {
        match args[i].as_str() {
            "--bundle" => { i+=1; bundle = args.get(i).cloned(); },
            "--queries" => { i+=1; queries = args.get(i).cloned(); },
            "--k" => { i+=1; if let Some(v) = args.get(i) { k = v.parse().unwrap_or(5); } },
            _ => {}
        }
        i+=1;
    }
    let b = bundle.unwrap_or_else(|| { usage(); std::process::exit(1) });
    let q = queries.unwrap_or_else(|| { usage(); std::process::exit(1) });
    (b, q, k)
}

fn main() {
    let (bundle_dir, queries_path, k) = parse_args();
    let store = VectorStore::open(&bundle_dir).expect("open bundle");
    println!("Opened bundle: size={} dim={}", store.size(), store.dimensions());

    // Load queries
    let data = fs::read_to_string(&queries_path).expect("read queries");
    let qs: Vec<Q> = serde_json::from_str(&data).expect("parse queries");

    for q in qs {
        println!("\n=== Query: {} ===", q.query);

        if q.embedding.len() != store.dimensions() { 
            eprintln!("  ! embedding dim {} != store.dim {}", q.embedding.len(), store.dimensions());
            continue;
        }

        let vres = store.search_vector(&q.embedding, k);
        println!("Vector top-{}:", k);
        for (rank, (id, score)) in vres.iter().enumerate() { 
            let (doc_id, text, _meta) = store.get_document(*id).unwrap_or_default();
            println!("  {:>2}. {:<24}  score={:.4}  {}", rank+1, doc_id, score, &text.chars().take(80).collect::<String>());
        }

        let bres = store.search_bm25(&q.query, k);
        println!("BM25 top-{}:", k);
        for (rank, (id, score)) in bres.iter().enumerate() { 
            let (doc_id, text, _meta) = store.get_document(*id).unwrap_or_default();
            println!("  {:>2}. {:<24}  score={:.4}  {}", rank+1, doc_id, score, &text.chars().take(80).collect::<String>());
        }

        let hres = store.search_hybrid(&q.embedding, &q.query, k, 0.5);
        println!("Hybrid(0.5) top-{}:", k);
        for (rank, (id, score)) in hres.iter().enumerate() { 
            let (doc_id, text, _meta) = store.get_document(*id).unwrap_or_default();
            println!("  {:>2}. {:<24}  score={:.4}  {}", rank+1, doc_id, score, &text.chars().take(80).collect::<String>());
        }
    }
}

