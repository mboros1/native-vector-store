#include <algorithm>
#include <atomic>
#include <chrono>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <random>
#include <thread>
#include <vector>

#ifdef NVS_USE_NANOBENCH
#define ANKERL_NANOBENCH_IMPLEMENT
#include "../deps/nanobench/nanobench.h"
#endif

#ifdef _OPENMP
#include <omp.h>
#endif

struct TopK {
    size_t k;
    std::vector<std::pair<float, uint32_t>> heap; // min-heap
    static bool cmp(const std::pair<float,uint32_t>& a, const std::pair<float,uint32_t>& b){ return a.first > b.first; }
    explicit TopK(size_t kk): k(kk){ heap.reserve(k+1);}    
    void push(float s, uint32_t id){
        if (heap.size() < k){ heap.emplace_back(s,id); std::push_heap(heap.begin(), heap.end(), cmp); }
        else if (k>0 && s > heap.front().first){ std::pop_heap(heap.begin(), heap.end(), cmp); heap.back()={s,id}; std::push_heap(heap.begin(), heap.end(), cmp);} }
    void merge(const TopK& o){ for (auto& p: o.heap) push(p.first, p.second); }
};

static inline float dot(const float* a, const float* b, size_t dim){
    float s=0.f;
    #pragma omp simd reduction(+:s)
    for (size_t i=0;i<dim;++i) s += a[i]*b[i];
    return s;
}

int main(){
    const size_t dim = std::getenv("NVS_DIM") ? std::strtoul(std::getenv("NVS_DIM"), nullptr, 10) : 1536;
    const size_t n   = std::getenv("NVS_N")   ? std::strtoul(std::getenv("NVS_N"),   nullptr, 10) : 50000;
    const size_t k   = std::getenv("NVS_K")   ? std::strtoul(std::getenv("NVS_K"),   nullptr, 10) : 10;

    // 64-byte aligned row stride
    const size_t row_bytes = dim * sizeof(float);
    const size_t aligned = ((row_bytes + 63) / 64) * 64;
    const size_t row_stride_f32 = aligned / sizeof(float);

    std::vector<float> store(n * row_stride_f32, 0.0f);
    std::mt19937 rng(42);
    std::uniform_real_distribution<float> dist(-1.0f, 1.0f);

    // Generate normalized vectors
    for (size_t i=0;i<n;++i){
        float norm=0.f;
        for (size_t j=0;j<dim;++j){ float v = dist(rng); store[i*row_stride_f32 + j] = v; norm += v*v; }
        norm = std::sqrt(norm); float inv = norm > 0 ? 1.0f/norm : 1.0f;
        for (size_t j=0;j<dim;++j){ store[i*row_stride_f32 + j] *= inv; }
    }

    // One random query
    std::vector<float> q(dim);
    {
        float norm=0.f; for (size_t j=0;j<dim;++j){ float v = dist(rng); q[j]=v; norm+=v*v; }
        norm = std::sqrt(norm); float inv = norm > 0 ? 1.0f/norm : 1.0f; for (auto& v: q) v*=inv;
    }

    auto run_once = [&]{
        #ifdef _OPENMP
        int T = omp_get_max_threads();
        #else
        int T = 1;
        #endif
        std::vector<TopK> heaps; heaps.reserve(T);
        for (int i=0;i<T;++i) heaps.emplace_back(k);
        #pragma omp parallel
        {
            int tid = 0;
            #ifdef _OPENMP
            tid = omp_get_thread_num();
            #endif
            TopK& local = heaps[tid];
            #pragma omp for schedule(static)
            for (int i=0;i<(int)n;++i){
                const float* row = &store[(size_t)i*row_stride_f32];
                float s = dot(q.data(), row, dim);
                local.push(s, (uint32_t)i);
            }
        }
        // reduce
        TopK finalk(k); for (auto& h: heaps) finalk.merge(h);
        std::sort_heap(finalk.heap.begin(), finalk.heap.end(), TopK::cmp);
        return finalk.heap;
    };

#ifdef NVS_USE_NANOBENCH
    ankerl::nanobench::Bench().epochs(20).run("cpp_vector_search", [&]{ (void)run_once(); });
#else
    auto t0 = std::chrono::high_resolution_clock::now();
    auto heap = run_once();
    auto t1 = std::chrono::high_resolution_clock::now();
    double ms = std::chrono::duration<double, std::milli>(t1 - t0).count();
    std::printf("cpp_vector_search n=%zu dim=%zu k=%zu: %.3f ms (%.1f Mdot/s)\n", n, dim, k, ms, (n/ms));
    (void)heap;
#endif
    return 0;
}
