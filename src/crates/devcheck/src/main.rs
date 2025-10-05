use serde::Serialize;
use std::time::{Duration, Instant};

#[derive(Serialize)]
struct CpuInfo {
    arch: &'static str,
    os: &'static str,
    brand: Option<String>,
    features: FeatureFlags,
    logical_cores: usize,
}

#[derive(Default, Serialize, Clone, Copy)]
struct FeatureFlags {
    avx2: bool,
    avx512f: bool,
    sse4_2: bool,
    neon: bool,
    dotprod: bool,
}

#[derive(Serialize)]
struct SimdResult {
    len: usize,
    scalar_ms: f64,
    simd_ms: Option<f64>,
    speedup: Option<f64>,
    verified_equal: bool,
}

#[derive(Serialize)]
struct DevcheckOut {
    cpu: CpuInfo,
    simd_sum: SimdResult,
    rustc_version: String,
    caps: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SimdMode { Auto, Off }

fn rustc_version() -> String {
    option_env!("RUSTC_VERSION").unwrap_or("unknown").to_string()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let json_flag = args.iter().any(|a| a == "--json");

    // parse --len and --simd
    let mut len: usize = 8_000_000;
    let mut simd_mode = SimdMode::Auto;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--len" => {
                if let Some(v) = args.get(i + 1) { if let Ok(n) = v.parse() { len = n; } }
                i += 1;
            }
            "--simd" => {
                if let Some(v) = args.get(i + 1) { if v == "off" { simd_mode = SimdMode::Off; } }
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }

    let cpu = gather_cpu();
    let caps = caps_string(&cpu.features, cpu.arch);
    let simd = simd_bench(len, simd_mode);

    let out = DevcheckOut { cpu, simd_sum: simd, rustc_version: rustc_version(), caps };

    if json_flag {
        println!("{}", serde_json::to_string_pretty(&out).unwrap());
    } else {
        print_human(&out);
    }
}

fn print_human(out: &DevcheckOut) {
    println!("OS/Arch   : {}/{}", out.cpu.os, out.cpu.arch);
    if let Some(b) = &out.cpu.brand { println!("CPU       : {}", b); }
    println!("Cores     : {}", out.cpu.logical_cores);
    println!(
        "Features  : SSE4.2={} AVX2={} AVX512F={} NEON={} DOTPROD={}",
        out.cpu.features.sse4_2, out.cpu.features.avx2, out.cpu.features.avx512f,
        out.cpu.features.neon, out.cpu.features.dotprod
    );
    println!("Caps      : {}", out.caps);

    let s = &out.simd_sum;
    println!("SIMD sum test ({} f32 values):", s.len);
    println!("  scalar  : {:>7.3} ms", s.scalar_ms);
    match (s.simd_ms, s.speedup) {
        (Some(ms), Some(sp)) => println!("  simd    : {:>7.3} ms   (×{:.2})", ms, sp),
        _ => println!("  simd    : not available or disabled"),
    }
    println!("  verify  : {}", if s.verified_equal { "OK" } else { "MISMATCH" });
}

fn caps_string(f: &FeatureFlags, arch: &str) -> String {
    let mut parts = vec![arch.to_string()];
    if f.sse4_2 { parts.push("sse4.2".into()); }
    if f.avx2 { parts.push("avx2".into()); }
    if f.avx512f { parts.push("avx512f".into()); }
    if f.neon { parts.push("neon".into()); }
    if f.dotprod { parts.push("dotprod".into()); }
    format!("caps:{}", parts.join("+"))
}

fn gather_cpu() -> CpuInfo {
    let arch = std::env::consts::ARCH;
    let os = std::env::consts::OS;
    let logical_cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    let (brand, features) = {
        let cpuid = raw_cpuid::CpuId::new();
        let brand = cpuid.get_processor_brand_string().map(|b| b.as_str().trim().to_string());
        let f = cpuid.get_extended_feature_info();
        let sse42 = cpuid.get_feature_info().map(|fi| fi.has_sse42()).unwrap_or(false);
        let avx2 = f.map(|ef| ef.has_avx2()).unwrap_or(false);
        let avx512f = f.map(|ef| ef.has_avx512f()).unwrap_or(false);
        let ff = FeatureFlags { sse4_2: sse42, avx2, avx512f, ..Default::default() };
        (brand, ff)
    };

    #[cfg(target_arch = "aarch64")]
    let (brand, features) = {
        let mut ff = FeatureFlags::default();
        ff.neon = std::arch::is_aarch64_feature_detected!("neon");
        ff.dotprod = std::arch::is_aarch64_feature_detected!("dotprod");
        (None, ff)
    };

    #[cfg(all(not(any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64"))))]
    let (brand, features) = (None, FeatureFlags::default());

    CpuInfo { arch, os, brand, features, logical_cores }
}

// Quick correctness + perf smoke test: sum of f32 with scalar vs SIMD
fn simd_bench(n: usize, simd_mode: SimdMode) -> SimdResult {
    let mut v = Vec::with_capacity(n);
    // deterministic data
    let mut x = 1.0f32;
    for _ in 0..n { v.push(x); x = (x * 1.000_001 + 0.123_45).fract(); }

    // scalar
    let t0 = Instant::now();
    let mut sum_scalar = 0.0f32;
    for &y in &v { sum_scalar += y; }
    let scalar_ms = dur_ms(t0.elapsed());

    // SIMD path if available and not disabled
    let simd: Option<(f32, f64)> = if simd_mode == SimdMode::Off { None } else { simd_sum(&v) };

    let (sum_simd, simd_ms) = match simd { Some((s, ms)) => (Some(s), Some(ms)), None => (None, None) };
    let verified_equal = match sum_simd { Some(s) => (sum_scalar - s).abs() <= 1e-2 * sum_scalar.abs().max(1.0), None => true };
    let speedup = simd_ms.map(|ms| scalar_ms / ms);

    SimdResult { len: n, scalar_ms, simd_ms, speedup, verified_equal }
}

fn dur_ms(d: Duration) -> f64 { d.as_secs_f64() * 1_000.0 }

fn simd_sum(v: &[f32]) -> Option<(f32, f64)> {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    unsafe { return simd_sum_x86(v); }
    #[cfg(target_arch = "aarch64")]
    unsafe { return simd_sum_neon(v); }
    #[allow(unreachable_code)]
    None
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
unsafe fn simd_sum_x86(v: &[f32]) -> Option<(f32, f64)> {
    #[cfg(target_arch = "x86_64")]
    use std::arch::x86_64::*;
    #[cfg(target_arch = "x86")]
    use std::arch::x86::*;

    if !std::arch::is_x86_feature_detected!("sse4.2") { return None; }
    let use_avx2 = std::arch::is_x86_feature_detected!("avx2");
    let t0 = Instant::now();

    let s = if use_avx2 {
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        {
            let mut acc = _mm256_setzero_ps();
            let mut i = 0;
            let chunks = v.len() / 8 * 8;
            while i < chunks {
                let x = _mm256_loadu_ps(v.as_ptr().add(i));
                acc = _mm256_add_ps(acc, x);
                i += 8;
            }
            let mut tmp = [0f32; 8];
            _mm256_storeu_ps(tmp.as_mut_ptr(), acc);
            let mut s = tmp.iter().copied().sum::<f32>();
            for &y in &v[chunks..] { s += y; }
            s
        }
    } else {
        let mut acc = _mm_setzero_ps();
        let mut i = 0;
        let chunks = v.len() / 4 * 4;
        while i < chunks {
            let x = _mm_loadu_ps(v.as_ptr().add(i));
            acc = _mm_add_ps(acc, x);
            i += 4;
        }
        let mut tmp = [0f32; 4];
        _mm_storeu_ps(tmp.as_mut_ptr(), acc);
        let mut s = tmp.iter().copied().sum::<f32>();
        for &y in &v[chunks..] { s += y; }
        s
    };

    let ms = dur_ms(t0.elapsed());
    Some((s, ms))
}

#[cfg(target_arch = "aarch64")]
unsafe fn simd_sum_neon(v: &[f32]) -> Option<(f32, f64)> {
    use std::arch::aarch64::*;
    if !std::arch::is_aarch64_feature_detected!("neon") { return None; }
    let t0 = Instant::now();

    let mut acc = vdupq_n_f32(0.0);
    let mut i = 0usize;
    let chunks = v.len() / 4 * 4;
    while i < chunks {
        let x = vld1q_f32(v.as_ptr().add(i));
        acc = vaddq_f32(acc, x);
        i += 4;
    }
    let mut tmp = [0f32; 4];
    vst1q_f32(tmp.as_mut_ptr(), acc);
    let mut s = tmp.iter().copied().sum::<f32>();
    for &y in &v[chunks..] { s += y; }

    let ms = dur_ms(t0.elapsed());
    Some((s, ms))
}
