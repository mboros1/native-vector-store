use criterion::{criterion_group, criterion_main, Criterion, Throughput};

fn bench_fast_extract(c: &mut Criterion) {
    // Minimal PDF-like bytes; extractor may return None but should be fast.
    let pdf = b"%PDF-1.7\n1 0 obj<<>>endobj\nxref\n0 1\n0000000000 65535 f \ntrailer<<>>startxref\n0\n%%EOF";
    let mut group = c.benchmark_group("pdf_fast_extract");
    group.throughput(Throughput::Bytes(pdf.len() as u64));
    group.bench_function("from_bytes", |b| {
        b.iter(|| {
            let _ = nvs_pdf_core::fast_extract_pages_from_bytes_with_stats(pdf, Some(1));
        })
    });
    group.finish();
}

criterion_group!(benches, bench_fast_extract);
criterion_main!(benches);

