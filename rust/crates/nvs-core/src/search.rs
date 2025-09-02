use rayon::prelude::*;
use std::cmp::Ordering;

use crate::simd::dot;

#[derive(Clone, Copy)]
struct Score { s: f32, id: u32 }

impl PartialEq for Score { fn eq(&self, o: &Self) -> bool { self.s == o.s } }
impl Eq for Score {}
impl PartialOrd for Score { fn partial_cmp(&self, o: &Self) -> Option<Ordering> { self.s.partial_cmp(&o.s) } }
impl Ord for Score { fn cmp(&self, o: &Self) -> Ordering { self.partial_cmp(o).unwrap_or(Ordering::Equal) } }

struct TopK {
    k: usize,
    heap: std::collections::BinaryHeap<std::cmp::Reverse<Score>>,
}

impl TopK {
    fn new(k: usize) -> Self { Self { k, heap: Default::default() } }
    fn push(&mut self, sc: Score) {
        if self.heap.len() < self.k {
            self.heap.push(std::cmp::Reverse(sc));
        } else if let Some(mut top) = self.heap.peek_mut() {
            if sc.s > top.0.s { *top = std::cmp::Reverse(sc); }
        }
    }
    fn merge(mut self, other: Self) -> Self {
        for rev in other.heap.into_iter() { self.push(rev.0); }
        self
    }
}

pub fn search_parallel(
    query: &[f32],
    n: usize,
    dim: usize,
    row_stride_f32: usize,
    store: &[f32],
    k: usize,
) -> Vec<(u32, f32)> {
    (0..n as u32).into_par_iter()
        .with_min_len(1024)
        .fold(|| TopK::new(k), |mut tk, id| {
            let start = (id as usize) * row_stride_f32;
            let row = &store[start .. start + dim];
            let s = dot(query, row);
            tk.push(Score { s, id });
            tk
        })
        .reduce(|| TopK::new(k), |a, b| a.merge(b))
        .heap
        .into_sorted_vec()
        .into_iter()
        .rev()
        .map(|rev| (rev.0.id, rev.0.s))
        .collect()
}
