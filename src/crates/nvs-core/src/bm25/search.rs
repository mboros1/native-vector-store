use crate::Bundle;

pub fn search(bundle: &Bundle, query: &str, k: usize) -> Vec<(u32, f32)> {
    let tok = crate::tokenizer::SimpleTokenizer::with_options(crate::tokenizer::TokenizerOptions {
        lowercase: true,
        split_contractions: true,
        remove_stopwords: true,
        remove_punctuation: true,
    });
    let clean = crate::tokenizer::preprocess_bm25(query);
    let mut tf: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    for t in tok.split(&clean) {
        if let Some(norm) = crate::tokenizer::bm25_normalize_token(&t) {
            *tf.entry(norm).or_insert(0) += 1;
        }
    }
    search_terms(
        bundle,
        &tf.keys().map(|s| s.as_str()).collect::<Vec<_>>(),
        k,
    )
}

pub fn search_terms(bundle: &Bundle, query_terms: &[&str], k: usize) -> Vec<(u32, f32)> {
    let postings = &bundle.postings;
    let avgdl = bundle.manifest.bm25.avgdl;
    let mut scores: std::collections::HashMap<u32, f32> = std::collections::HashMap::new();
    for term in query_terms {
        if let Some((off, len, df)) = lookup_term(bundle, term) {
            let idf = calc_idf(bundle.manifest.num_docs as f32, df as f32);
            let mut i = off as usize;
            let mut prev: u32 = 0;
            for _ in 0..len {
                let delta = u32::from_le_bytes(postings[i..i + 4].try_into().unwrap());
                let tf = u32::from_le_bytes(postings[i + 4..i + 8].try_into().unwrap());
                i += 8;
                let doc_id = prev.wrapping_add(delta);
                prev = doc_id;
                let dl = bundle.doclen.get(doc_id as usize).copied().unwrap_or(0) as f32;
                let k1 = 1.2f32;
                let b = 0.75f32;
                let denom = tf as f32 + k1 * (1.0 - b + b * (dl / avgdl as f32));
                let inc = idf * (tf as f32 * (k1 + 1.0)) / denom;
                *scores.entry(doc_id).or_insert(0.0) += inc;
            }
        }
    }
    let mut v: Vec<(u32, f32)> = scores.into_iter().collect();
    v.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });
    v.truncate(k);
    v
}

fn lookup_term(bundle: &Bundle, term: &str) -> Option<(u64, u32, u32)> {
    let tid = *bundle.terms.get(term)?;
    let ent = bundle.lexicon.get(tid)?;
    Some((ent.offset, ent.length, ent.df))
}

fn calc_idf(n: f32, df: f32) -> f32 {
    ((n - df + 0.5) / (df + 0.5) + 1.0).ln()
}

#[cfg(test)]
mod bm25_search_tests {

}