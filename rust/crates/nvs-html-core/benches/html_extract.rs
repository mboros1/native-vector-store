use criterion::{criterion_group, criterion_main, Criterion, Throughput};

fn bench_extract(c: &mut Criterion) {
    let html = br#"<html><body><h1>Title</h1><p>Hello</p><h2>Sub</h2><p>World</p></body></html>"#;
    let mut group = c.benchmark_group("html_extract");
    group.throughput(Throughput::Bytes(html.len() as u64));
    group.bench_function("sections_from_bytes", |b| {
        b.iter(|| {
            let _ = nvs_html_core::fast_extract_sections_from_bytes_with_stats(html, None).unwrap();
        })
    });
    group.finish();
}

criterion_group!(benches, bench_extract);
criterion_main!(benches);

