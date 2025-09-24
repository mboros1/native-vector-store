use criterion::{criterion_group, criterion_main, BatchSize, Criterion, Throughput};
use nvs_core::search::search_parallel;
use rand::{rngs::StdRng, Rng, SeedableRng};

fn gen_normed_vectors(n: usize, dim: usize, seed: u64) -> Vec<f32> {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut data = vec![0f32; n * dim];
    for i in 0..n {
        let mut norm = 0f32;
        for j in 0..dim {
            let v = rng.gen::<f32>() * 2.0 - 1.0;
            data[i * dim + j] = v;
            norm += v * v;
        }
        let inv = norm.sqrt().recip();
        for j in 0..dim {
            data[i * dim + j] *= inv;
        }
    }
    data
}

fn bench_vector_search(c: &mut Criterion) {
    let dim = std::env::var("NVS_DIM")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1536usize);
    let n = std::env::var("NVS_N")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(50_000usize);
    let k = std::env::var("NVS_K")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10usize);
    let seed = 42u64;

    // Allocate aligned rows: 64-byte stride
    let row_bytes = dim * 4;
    let aligned = row_bytes.div_ceil(64) * 64;
    let row_stride_f32 = aligned / 4;
    let mut store = vec![0f32; n * row_stride_f32];

    // Generate normalized vectors and copy into padded buffer
    let data = gen_normed_vectors(n, dim, seed);
    for i in 0..n {
        store[i * row_stride_f32..i * row_stride_f32 + dim]
            .copy_from_slice(&data[i * dim..(i + 1) * dim]);
    }

    // Prepare a few queries
    let queries = gen_normed_vectors(16, dim, seed ^ 0xDEADBEEF);

    let mut group = c.benchmark_group("vector_search");
    group.throughput(Throughput::Elements(n as u64));
    group.sample_size(20);
    group.bench_function(format!("search_n{}_d{}_k{}", n, dim, k), |b| {
        b.iter_batched(
            || {
                // pick a random query
                let qid = rand::random::<usize>() % 16;
                &queries[qid * dim..(qid + 1) * dim]
            },
            |q| {
                let _topk = search_parallel(q, n, dim, row_stride_f32, &store, k);
            },
            BatchSize::SmallInput,
        )
    });
    group.finish();
}

criterion_group!(benches, bench_vector_search);
criterion_main!(benches);
