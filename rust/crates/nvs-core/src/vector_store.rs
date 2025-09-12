use std::sync::Arc;

use crate::bundle::Bundle;
use crate::{bm25, hybrid};
use rayon::prelude::*;
use std::cmp::Ordering;

pub struct VectorStore {
    bundle: Arc<Bundle>,
}

impl VectorStore {
    pub fn open<P: AsRef<std::path::Path>>(path: P) -> crate::errors::Result<Self> {
        let b = Bundle::open(path)?;
        Ok(Self {
            bundle: Arc::new(b),
        })
    }

    pub fn from_bundle(bundle: Bundle) -> Self {
        Self {
            bundle: Arc::new(bundle),
        }
    }

    pub fn size(&self) -> usize {
        self.bundle.manifest.num_docs as usize
    }
    pub fn dimensions(&self) -> usize {
        self.bundle.manifest.dim as usize
    }

    pub fn get_document(&self, doc_id: u32) -> Option<(String, String, String)> {
        self.bundle.get_document(doc_id)
    }

    pub fn search_vector(&self, query: &[f32], k: usize) -> Vec<(u32, f32)> {
        if k == 0 || query.len() != self.dimensions() {
            return Vec::new();
        }
        let dtype = self.bundle.manifest.embedding.dtype.to_lowercase();
        if dtype == "f16" {
            self.search_vector_f16(query, k)
        } else {
            let store_f32 = self.bundle.vectors_as_f32();
            let mut res = crate::search::search_parallel(
                query,
                self.size(),
                self.dimensions(),
                self.bundle.row_stride_f32(),
                store_f32,
                k,
            );
            res.sort_by(|a, b| {
                b.1.partial_cmp(&a.1)
                    .unwrap_or(Ordering::Equal)
                    .then_with(|| a.0.cmp(&b.0))
            });
            res
        }
    }

    fn search_vector_f16(&self, query: &[f32], k: usize) -> Vec<(u32, f32)> {
        use half::f16;
        let n = self.size();
        let dim = self.dimensions();
        let stride = self.bundle.row_stride_bytes();
        let base = self.bundle.vectors_raw();
        // Per-thread heap and buffer
        #[derive(Default)]
        struct Tk {
            k: usize,
            heap: std::collections::BinaryHeap<
                std::cmp::Reverse<(ordered_float::OrderedFloat<f32>, u32)>,
            >,
        }
        impl Tk {
            fn new(k: usize) -> Self {
                Self {
                    k,
                    heap: Default::default(),
                }
            }
            fn push(&mut self, s: f32, id: u32) {
                let item = std::cmp::Reverse((ordered_float::OrderedFloat(s), id));
                if self.heap.len() < self.k {
                    self.heap.push(item);
                } else if let Some(mut top) = self.heap.peek_mut() {
                    if item.0.0 > top.0.0 {
                        *top = item;
                    }
                }
            }
            fn merge(mut self, other: Self) -> Self {
                for it in other.heap.into_iter() {
                    self.push(it.0.0.0, it.0.1);
                }
                self
            }
        }
        let (heap, _) = (0..n as u32)
            .into_par_iter()
            .with_min_len(1024)
            .fold(
                || (Tk::new(k), vec![0f32; dim]),
                |(mut tk, mut buf), id| {
                    let start = (id as usize) * stride;
                    let row = &base[start..start + dim * 2];
                    for j in 0..dim {
                        let lo = row[j * 2] as u16;
                        let hi = row[j * 2 + 1] as u16;
                        let bits = lo | (hi << 8);
                        buf[j] = f16::from_bits(bits).to_f32();
                    }
                    let s = crate::simd::dot(query, &buf);
                    tk.push(s, id);
                    (tk, buf)
                },
            )
            .reduce(|| (Tk::new(k), vec![]), |a, b| (a.0.merge(b.0), vec![]));
        let mut out: Vec<(u32, f32)> = heap
            .heap
            .into_sorted_vec()
            .into_iter()
            .map(|r| (r.0.1, r.0.0.0))
            .collect();
        // Ensure deterministic order: score desc, id asc
        out.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(Ordering::Equal)
                .then_with(|| a.0.cmp(&b.0))
        });
        out
    }

    pub fn search_bm25(&self, query: &str, k: usize) -> Vec<(u32, f32)> {
        bm25::search(&self.bundle, query, k)
    }

    pub fn search_hybrid(
        &self,
        query_vec: &[f32],
        query_text: &str,
        k: usize,
        mut vector_weight: f32,
    ) -> Vec<(u32, f32)> {
        if k == 0 || query_vec.len() != self.dimensions() {
            return Vec::new();
        }
        if vector_weight.is_nan() {
            vector_weight = 0.5;
        }
        if vector_weight < 0.0 {
            vector_weight = 0.0;
        }
        if vector_weight > 1.0 {
            vector_weight = 1.0;
        }
        let kk = std::cmp::min(k * 2, self.size());
        let vres = self.search_vector(query_vec, kk);
        let bres = self.search_bm25(query_text, kk);
        hybrid::fuse_rrf(&vres, &bres, k, vector_weight)
    }
}
