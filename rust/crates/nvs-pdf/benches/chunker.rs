use criterion::{criterion_group, criterion_main, Criterion};

fn bench_chunker(c: &mut Criterion) {
    let pages = vec![
        ("# Title\nIntro para.".to_string(), 0),
        ("## Section\nBody body body.".to_string(), 1),
    ];
    let tok = tokenmonster::GreedyTokenizer::from_cl100k_bin();
    let opts = nvs_core::chunker::ChunkOptions { max_tokens: 128, min_tokens: 1, overlap_tokens: 8 };
    c.bench_function("pdf_chunker_core", |b| {
        b.iter(|| {
            let _ = nvs_core::chunker::chunk_pages_with_stats(&pages, &tok, &opts);
        })
    });
}

criterion_group!(benches, bench_chunker);
criterion_main!(benches);

