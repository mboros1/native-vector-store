#[inline]
pub fn dot(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len());
    #[cfg(target_arch = "x86_64")]
    {
        if x86_avx2_fma::get() {
            unsafe {
                return dot_avx2_fma(a, b);
            };
        } else if x86_avx2::get() {
            unsafe {
                return dot_avx2(a, b);
            };
        } else if x86_sse2::get() {
            unsafe {
                return dot_sse2(a, b);
            };
        }
        return dot_scalar(a, b);
    }
    #[cfg(target_arch = "aarch64")]
    {
        unsafe {
            return dot_neon(a, b);
        };
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        return dot_scalar(a, b);
    }
}

#[inline]
#[allow(dead_code)]
fn dot_scalar(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// Dot product between an f32 slice and a packed f16 row (as bytes, little-endian), length `dim*2`.
/// The `row` slice may be larger (e.g., includes padding); only the first `dim*2` bytes are used.
#[inline]
pub fn dot_f32_f16(a: &[f32], row: &[u8], dim: usize) -> f32 {
    debug_assert_eq!(a.len(), dim);
    debug_assert!(row.len() >= dim * 2);
    #[cfg(target_arch = "x86_64")]
    {
        if x86_avx2_fma::get() && x86_f16c::get() {
            unsafe { return dot_f32_f16_avx2_fma_f16c(a, row, dim); }
        } else if x86_avx2::get() && x86_f16c::get() {
            unsafe { return dot_f32_f16_avx2_f16c(a, row, dim); }
        } else if x86_sse2::get() && x86_f16c::get() {
            unsafe { return dot_f32_f16_sse_f16c(a, row, dim); }
        }
        return dot_f32_f16_scalar(a, row, dim);
    }
    #[cfg(target_arch = "aarch64")]
    {
        // Prefer fp16-assisted path when compiled with fp16 target feature; otherwise use scalar-convert NEON.
        #[cfg(target_feature = "fp16")]
        unsafe {
            return dot_f32_f16_neon_fp16(a, row, dim);
        }
        #[cfg(not(target_feature = "fp16"))]
        unsafe {
            return dot_f32_f16_neon(a, row, dim);
        }
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        return dot_f32_f16_scalar(a, row, dim);
    }
}

#[inline]
fn dot_f32_f16_scalar(a: &[f32], row: &[u8], dim: usize) -> f32 {
    use half::f16;
    let mut sum = 0f32;
    for i in 0..dim {
        let lo = row[2 * i] as u16;
        let hi = row[2 * i + 1] as u16;
        let bits = lo | (hi << 8);
        let bf = f16::from_bits(bits).to_f32();
        sum += a[i] * bf;
    }
    sum
}

// x86 feature detectors
#[cfg(target_arch = "x86_64")]
cpufeatures::new!(x86_avx2_fma, "avx2", "fma");
#[cfg(target_arch = "x86_64")]
cpufeatures::new!(x86_avx2, "avx2");
#[cfg(target_arch = "x86_64")]
cpufeatures::new!(x86_sse2, "sse2");
#[cfg(target_arch = "x86_64")]
cpufeatures::new!(x86_f16c, "f16c");

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
    while i < a.len() {
        sum += *a.get_unchecked(i) * *b.get_unchecked(i);
        i += 1;
    }
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
    while i < a.len() {
        sum += *a.get_unchecked(i) * *b.get_unchecked(i);
        i += 1;
    }
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
    while i < a.len() {
        sum += *a.get_unchecked(i) * *b.get_unchecked(i);
        i += 1;
    }
    sum
}

// x86: AVX2 + F16C + FMA path (8-wide converts)
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma,f16c")]
unsafe fn dot_f32_f16_avx2_fma_f16c(a: &[f32], row: &[u8], dim: usize) -> f32 {
    use core::arch::x86_64::*;
    let mut i = 0usize;
    let mut acc = _mm256_setzero_ps();
    while i + 8 <= dim {
        let va = _mm256_loadu_ps(a.as_ptr().add(i));
        let bytes = row.as_ptr().add(2 * i) as *const __m128i;
        let h = _mm_loadu_si128(bytes); // 8 x u16
        let vb = _mm256_cvtph_ps(h); // widen to 8 x f32
        acc = _mm256_fmadd_ps(va, vb, acc);
        i += 8;
    }
    let mut tmp = [0f32; 8];
    _mm256_storeu_ps(tmp.as_mut_ptr(), acc);
    let mut sum: f32 = tmp.iter().sum();
    // Handle tail (4-wide if possible)
    if i + 4 <= dim {
        let va = _mm_loadu_ps(a.as_ptr().add(i));
        let bytes = row.as_ptr().add(2 * i) as *const __m128i;
        // Only lower 4 lanes used
        let h = _mm_loadl_epi64(bytes as *const __m128i);
        let vb = _mm_cvtph_ps(h);
        let prod = _mm_mul_ps(va, vb);
        let mut t = [0f32; 4];
        _mm_storeu_ps(t.as_mut_ptr(), prod);
        sum += t.iter().sum::<f32>();
        i += 4;
    }
    while i < dim {
        let lo = *row.get_unchecked(2 * i) as u16;
        let hi = *row.get_unchecked(2 * i + 1) as u16;
        let bits = lo | (hi << 8);
        let bf = half::f16::from_bits(bits).to_f32();
        sum += *a.get_unchecked(i) * bf;
        i += 1;
    }
    sum
}

// x86: AVX2 + F16C (no FMA)
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,f16c")]
unsafe fn dot_f32_f16_avx2_f16c(a: &[f32], row: &[u8], dim: usize) -> f32 {
    use core::arch::x86_64::*;
    let mut i = 0usize;
    let mut acc = _mm256_setzero_ps();
    while i + 8 <= dim {
        let va = _mm256_loadu_ps(a.as_ptr().add(i));
        let bytes = row.as_ptr().add(2 * i) as *const __m128i;
        let h = _mm_loadu_si128(bytes);
        let vb = _mm256_cvtph_ps(h);
        let prod = _mm256_mul_ps(va, vb);
        acc = _mm256_add_ps(acc, prod);
        i += 8;
    }
    let mut tmp = [0f32; 8];
    _mm256_storeu_ps(tmp.as_mut_ptr(), acc);
    let mut sum: f32 = tmp.iter().sum();
    if i + 4 <= dim {
        let va = _mm_loadu_ps(a.as_ptr().add(i));
        let bytes = row.as_ptr().add(2 * i) as *const __m128i;
        let h = _mm_loadl_epi64(bytes as *const __m128i);
        let vb = _mm_cvtph_ps(h);
        let prod = _mm_mul_ps(va, vb);
        let mut t = [0f32; 4];
        _mm_storeu_ps(t.as_mut_ptr(), prod);
        sum += t.iter().sum::<f32>();
        i += 4;
    }
    while i < dim {
        let lo = *row.get_unchecked(2 * i) as u16;
        let hi = *row.get_unchecked(2 * i + 1) as u16;
        let bits = lo | (hi << 8);
        let bf = half::f16::from_bits(bits).to_f32();
        sum += *a.get_unchecked(i) * bf;
        i += 1;
    }
    sum
}

// x86: SSE2 + F16C (4-wide converts)
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse2,f16c")]
unsafe fn dot_f32_f16_sse_f16c(a: &[f32], row: &[u8], dim: usize) -> f32 {
    use core::arch::x86_64::*;
    let mut i = 0usize;
    let mut acc = _mm_setzero_ps();
    while i + 4 <= dim {
        let va = _mm_loadu_ps(a.as_ptr().add(i));
        let bytes = row.as_ptr().add(2 * i) as *const __m128i;
        let h = _mm_loadl_epi64(bytes as *const __m128i);
        let vb = _mm_cvtph_ps(h);
        let prod = _mm_mul_ps(va, vb);
        acc = _mm_add_ps(acc, prod);
        i += 4;
    }
    let mut tmp = [0f32; 4];
    _mm_storeu_ps(tmp.as_mut_ptr(), acc);
    let mut sum: f32 = tmp.iter().sum();
    while i < dim {
        let lo = *row.get_unchecked(2 * i) as u16;
        let hi = *row.get_unchecked(2 * i + 1) as u16;
        let bits = lo | (hi << 8);
        let bf = half::f16::from_bits(bits).to_f32();
        sum += *a.get_unchecked(i) * bf;
        i += 1;
    }
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
    while i < a.len() {
        sum += *a.get_unchecked(i) * *b.get_unchecked(i);
        i += 1;
    }
    sum
}

// aarch64 NEON path: multiply-add in f32 lanes; f16->f32 conversion is scalar per-lane.
#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn dot_f32_f16_neon(a: &[f32], row: &[u8], dim: usize) -> f32 {
    use core::arch::aarch64::*;
    let mut i = 0usize;
    let mut acc = vdupq_n_f32(0.0);
    while i + 4 <= dim {
        let va = vld1q_f32(a.as_ptr().add(i));
        // Convert 4 half elements to f32 scalars, then load to NEON
        let mut tmp = [0f32; 4];
        for j in 0..4 {
            let lo = *row.get_unchecked(2 * (i + j)) as u16;
            let hi = *row.get_unchecked(2 * (i + j) + 1) as u16;
            let bits = lo | (hi << 8);
            tmp[j] = half::f16::from_bits(bits).to_f32();
        }
        let vb = vld1q_f32(tmp.as_ptr());
        acc = vfmaq_f32(acc, va, vb);
        i += 4;
    }
    let mut sum = vaddvq_f32(acc);
    while i < dim {
        let lo = *row.get_unchecked(2 * i) as u16;
        let hi = *row.get_unchecked(2 * i + 1) as u16;
        let bits = lo | (hi << 8);
        let bf = half::f16::from_bits(bits).to_f32();
        sum += *a.get_unchecked(i) * bf;
        i += 1;
    }
    sum
}

// aarch64 NEON + fp16 target: placeholder enabling easy upgrade to true fp16 vector converts when stable.
#[cfg(all(target_arch = "aarch64", target_feature = "fp16"))]
#[target_feature(enable = "neon,fp16")]
unsafe fn dot_f32_f16_neon_fp16(a: &[f32], row: &[u8], dim: usize) -> f32 {
    // NOTE: Rust stable does not yet expose convenient NEON fp16 vector convert intrinsics.
    // This path intentionally reuses the scalar-convert + NEON-FMA approach, while allowing
    // future replacement with native fp16 vector conversion (e.g., vcvt_f32_f16) once available.
    dot_f32_f16_neon(a, row, dim)
}
