pub fn fuse_rrf(
    vector: &[(u32, f32)],
    bm25: &[(u32, f32)],
    k: usize,
    vector_weight: f32,
) -> Vec<(u32, f32)> {
    use std::collections::HashMap;
    let c = 60.0f32;
    let mut combined: HashMap<u32, f32> = HashMap::new();
    for (i, (doc, _)) in vector.iter().enumerate() {
        let rrf = 1.0f32 / (c + (i as f32) + 1.0);
        *combined.entry(*doc).or_insert(0.0) += vector_weight * rrf;
    }
    let one_minus = 1.0f32 - vector_weight;
    for (i, (doc, _)) in bm25.iter().enumerate() {
        let rrf = 1.0f32 / (c + (i as f32) + 1.0);
        *combined.entry(*doc).or_insert(0.0) += one_minus * rrf;
    }
    let mut items: Vec<(u32, f32)> = combined.into_iter().collect();
    items.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });
    if items.len() > k {
        items.truncate(k);
    }
    items
}
