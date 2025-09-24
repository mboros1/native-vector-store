use criterion::{criterion_group, criterion_main, BatchSize, Criterion};

fn bench_bm25_pipeline(c: &mut Criterion) {
    let docs = vec![
        nvs_packer::loader::Doc {
            id: "d1".into(),
            text: "alpha beta beta".into(),
            embedding: vec![0.0],
            meta: None,
        },
        nvs_packer::loader::Doc {
            id: "d2".into(),
            text: "beta gamma".into(),
            embedding: vec![0.0],
            meta: None,
        },
    ];
    c.bench_function("bm25_and_terms_small", |b| {
        b.iter_batched(
            || docs.clone(),
            |d| {
                let dir = tempfile::tempdir().unwrap();
                let _ = nvs_packer::bm25::write_bm25_and_terms(&d, dir.path(), 4).unwrap();
            },
            BatchSize::SmallInput,
        )
    });
}

criterion_group!(benches, bench_bm25_pipeline);
criterion_main!(benches);
