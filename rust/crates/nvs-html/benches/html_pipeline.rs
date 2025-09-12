use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use std::fs;

fn bench_pipeline(c: &mut Criterion) {
    let dir = tempfile::tempdir().unwrap();
    let html_path = dir.path().join("sample.html");
    let html = "<html><body><h1>Title</h1><p>Hello</p><h2>Sub</h2><p>World</p></body></html>";
    fs::write(&html_path, html).unwrap();
    let opts = nvs_html::HtmlChunkOptions { max_tokens: 128, min_tokens: 1, overlap_tokens: 8, section_limit: None };
    let mut group = c.benchmark_group("html_pipeline");
    group.throughput(Throughput::Bytes(html.len() as u64));
    group.bench_function("parse_to_chunks_with_stats", |b| {
        b.iter(|| {
            let _ = nvs_html::parse_to_chunks_with_stats(&html_path, &opts).unwrap();
        })
    });
    group.finish();
}

criterion_group!(benches, bench_pipeline);
criterion_main!(benches);

