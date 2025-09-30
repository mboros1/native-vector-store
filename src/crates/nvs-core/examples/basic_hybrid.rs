use nvs_core::VectorStore;

// Run with:
//   cargo run -p nvs-core --example basic_hybrid -- <BUNDLE_DIR>
fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let dir = args.next().expect("usage: basic_hybrid <BUNDLE_DIR>");
    let store = VectorStore::open(dir)?;

    // Dummy embedding and query text; replace with real embedding/query
    let dim = store.dimensions();
    let embedding = vec![0.0f32; dim];
    let q = "example query";
    let hits = store.search_hybrid(&embedding, q, 5, 0.5);

    println!("Top-{} results:", hits.len());
    for (id, score) in hits.iter() {
        if let Some(doc) = store.get_document_parsed(*id) {
            println!(
                "- id={} score={:.4} text={}...",
                doc.id,
                score,
                doc.text.chars().take(60).collect::<String>()
            );
        }
    }
    Ok(())
}
