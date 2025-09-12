use criterion::{criterion_group, criterion_main, BatchSize, Criterion, Throughput};

fn bench_encode(c: &mut Criterion) {
    let tm = tokenmonster::GreedyTokenizer::from_cl100k_bin();
    let text = "hello world, this is a small benchmark to tokenize".repeat(64);
    let mut group = c.benchmark_group("tokenmonster");
    group.throughput(Throughput::Bytes(text.len() as u64));
    group.bench_function("encode", |b| {
        b.iter_batched(|| text.clone(), |s| tm.encode(&s), BatchSize::SmallInput)
    });
    group.bench_function("count", |b| {
        b.iter_batched(|| text.clone(), |s| tm.count_tokens(&s), BatchSize::SmallInput)
    });
    group.finish();
}

criterion_group!(benches, bench_encode);
criterion_main!(benches);

