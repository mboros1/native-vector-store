#[inline]
pub fn dot(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len());
    #[cfg(target_arch = "x86_64")]
    {
        if x86_avx2_fma::get() {
            return unsafe { dot_avx2_fma(a, b) };
        } else if x86_avx2::get() {
            return unsafe { dot_avx2(a, b) };
        } else if x86_sse2::get() {
            return unsafe { dot_sse2(a, b) };
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        return unsafe { dot_neon(a, b) };
    }
    dot_scalar(a, b)
}

#[inline]
fn dot_scalar(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

// x86 feature detectors
#[cfg(target_arch = "x86_64")]
cpufeatures::new!(x86_avx2_fma, "avx2", "fma");
#[cfg(target_arch = "x86_64")]
cpufeatures::new!(x86_avx2, "avx2");
#[cfg(target_arch = "x86_64")]
cpufeatures::new!(x86_sse2, "sse2");

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn dot_avx2(a: &[f32], b: &[f32]) -> f32 {
    use core::arch::x86_64::*;
    let mut i = 0usize;
    let mut acc = _mm256_setzero_ps();
    while i + 8 <= a.len() {
        let va = _mm256_loadu_ps(a.as_ptr().add(i));
        let vb = _mm256_loadu_ps(b.as_ptr().add(i));
        let prod = _mm256_mul_ps(va, vb);
        acc = _mm256_add_ps(acc, prod);
        i += 8;
    }
    let mut tmp = [0f32; 8];
    _mm256_storeu_ps(tmp.as_mut_ptr(), acc);
    let mut sum: f32 = tmp.iter().sum();
    while i < a.len() { sum += *a.get_unchecked(i) * *b.get_unchecked(i); i += 1; }
    sum
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
unsafe fn dot_avx2_fma(a: &[f32], b: &[f32]) -> f32 {
    use core::arch::x86_64::*;
    let mut i = 0usize;
    let mut acc = _mm256_setzero_ps();
    while i + 8 <= a.len() {
        let va = _mm256_loadu_ps(a.as_ptr().add(i));
        let vb = _mm256_loadu_ps(b.as_ptr().add(i));
        acc = _mm256_fmadd_ps(va, vb, acc);
        i += 8;
    }
    let mut tmp = [0f32; 8];
    _mm256_storeu_ps(tmp.as_mut_ptr(), acc);
    let mut sum: f32 = tmp.iter().sum();
    while i < a.len() { sum += *a.get_unchecked(i) * *b.get_unchecked(i); i += 1; }
    sum
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse2")]
unsafe fn dot_sse2(a: &[f32], b: &[f32]) -> f32 {
    use core::arch::x86_64::*;
    let mut i = 0usize;
    let mut acc = _mm_setzero_ps();
    while i + 4 <= a.len() {
        let va = _mm_loadu_ps(a.as_ptr().add(i));
        let vb = _mm_loadu_ps(b.as_ptr().add(i));
        let prod = _mm_mul_ps(va, vb);
        acc = _mm_add_ps(acc, prod);
        i += 4;
    }
    let mut tmp = [0f32; 4];
    _mm_storeu_ps(tmp.as_mut_ptr(), acc);
    let mut sum: f32 = tmp.iter().sum();
    while i < a.len() { sum += *a.get_unchecked(i) * *b.get_unchecked(i); i += 1; }
    sum
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn dot_neon(a: &[f32], b: &[f32]) -> f32 {
    use core::arch::aarch64::*;
    let mut i = 0usize;
    let mut acc = vdupq_n_f32(0.0);
    while i + 4 <= a.len() {
        let va = vld1q_f32(a.as_ptr().add(i));
        let vb = vld1q_f32(b.as_ptr().add(i));
        acc = vfmaq_f32(acc, va, vb); // FMA if available; on some CPUs this maps to mul+add
        i += 4;
    }
    let mut sum: f32 = vaddvq_f32(acc);
    while i < a.len() { sum += *a.get_unchecked(i) * *b.get_unchecked(i); i += 1; }
    sum
}
