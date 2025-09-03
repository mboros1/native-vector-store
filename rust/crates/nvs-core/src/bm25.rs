use std::collections::HashMap;

use crate::bundle::Bundle;

pub fn search(bundle: &Bundle, query: &str, k: usize) -> Vec<(u32, f32)> {
    let tok = crate::tokenizer::SimpleTokenizer::with_options(crate::tokenizer::TokenizerOptions{
        lowercase: true,
        split_contractions: true,
        remove_stopwords: true,
        remove_punctuation: false,  // Keep punctuation for BM25 to maintain compatibility
    });
    let terms = tok.split(query);
    let view: Vec<&str> = terms.iter().map(|s| s.as_str()).collect();
    search_terms(bundle, &view, k)
}

pub fn search_terms(bundle: &Bundle, query_terms: &[&str], k: usize) -> Vec<(u32, f32)> {
    let n = bundle.manifest.num_docs as usize;
    if n == 0 || k == 0 || query_terms.is_empty() { return Vec::new(); }
    let avgdl = bundle.manifest.bm25.avgdl as f32;
    let k1 = bundle.manifest.bm25.k1 as f32;
    let b = bundle.manifest.bm25.b as f32;

    let mut acc: HashMap<u32, f32> = HashMap::with_capacity(1024);
    for &qt in query_terms {
        if let Some(&tid) = bundle.terms.get(qt) {
            if tid >= bundle.lexicon.len() { continue; }
            let lex = &bundle.lexicon[tid];
            // Use BM25 IDF with +1 to keep values positive, aligning with common IR practice (e.g., Lucene)
            let idf = (1.0 + (bundle.manifest.num_docs as f32 - lex.df as f32 + 0.5) / (lex.df as f32 + 0.5)).ln();
            let mut prev = 0u32;
            let mut off = lex.offset as usize;
            for _ in 0..lex.length {
                if off + 8 > bundle.postings.len() { break; }
                let delta = u32::from_le_bytes(bundle.postings[off..off+4].try_into().unwrap());
                let tf = u32::from_le_bytes(bundle.postings[off+4..off+8].try_into().unwrap());
                off += 8;
                let doc = prev.wrapping_add(delta);
                prev = doc;
                let dl = bundle.doclen.get(doc as usize).copied().unwrap_or(0) as f32;
                let tfc = (tf as f32 * (k1 + 1.0)) / (tf as f32 + k1 * (1.0 - b + b * dl / avgdl));
                *acc.entry(doc).or_insert(0.0) += idf * tfc;
            }
        }
    }
    use ordered_float::OrderedFloat;
    type HeapItem = std::cmp::Reverse<(OrderedFloat<f32>, u32)>;
    let mut heap: std::collections::BinaryHeap<HeapItem> = std::collections::BinaryHeap::new();
    for (&doc, &score) in acc.iter() {
        let item = std::cmp::Reverse((OrderedFloat(score), doc));
        if heap.len() < k { heap.push(item); }
        else if let Some(mut top) = heap.peek_mut() { if item.0 .0 > top.0 .0 { *top = item; } }
    }
    // into_sorted_vec() with Reverse comparator yields descending order by (score, doc).
    // Map to (doc, score) and ensure deterministic tie-break: score desc, id asc.
    let mut v: Vec<(u32, f32)> = heap
        .into_sorted_vec()
        .into_iter()
        .map(|r| {
            let (s, d) = r.0; // (OrderedFloat(score), doc)
            (d, s.0)
        })
        .collect();
    v.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal).then_with(|| a.0.cmp(&b.0)));
    v
}
