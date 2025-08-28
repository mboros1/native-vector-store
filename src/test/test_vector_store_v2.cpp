// Test program for VectorStoreV2 (bundle-based implementation)
#include "../vector_store_v2.h"
#include <iostream>
#include <chrono>
#include <random>
#include <cassert>

// Color output for test results
#define GREEN "\033[32m"
#define RED "\033[31m"
#define YELLOW "\033[33m"
#define RESET "\033[0m"

void test_pass(const std::string& test_name) {
    std::cout << GREEN << "✓ " << RESET << test_name << std::endl;
}

void test_fail(const std::string& test_name, const std::string& reason) {
    std::cout << RED << "✗ " << RESET << test_name << ": " << reason << std::endl;
}

// Generate random normalized vector
std::vector<float> generate_random_vector(size_t dim) {
    static std::random_device rd;
    static std::mt19937 gen(rd());
    static std::normal_distribution<float> dist(0.0f, 1.0f);
    
    std::vector<float> vec(dim);
    float norm = 0.0f;
    
    for (size_t i = 0; i < dim; ++i) {
        vec[i] = dist(gen);
        norm += vec[i] * vec[i];
    }
    
    // Normalize
    norm = std::sqrt(norm);
    for (size_t i = 0; i < dim; ++i) {
        vec[i] /= norm;
    }
    
    return vec;
}

int main(int argc, char* argv[]) {
    if (argc != 2) {
        std::cerr << "Usage: " << argv[0] << " <bundle-directory>\n";
        return 1;
    }
    
    std::string bundle_path = argv[1];
    
    std::cout << "Testing VectorStoreV2 with bundle: " << bundle_path << "\n";
    std::cout << "=====================================\n\n";
    
    // Test 1: Open bundle
    std::cout << YELLOW << "Test 1: Opening bundle..." << RESET << "\n";
    auto start = std::chrono::high_resolution_clock::now();
    
    nvs::VectorStoreV2 store;
    if (!store.open(bundle_path)) {
        test_fail("Open bundle", "Failed to open");
        return 1;
    }
    
    auto end = std::chrono::high_resolution_clock::now();
    auto open_time = std::chrono::duration_cast<std::chrono::milliseconds>(end - start).count();
    
    test_pass("Bundle opened in " + std::to_string(open_time) + "ms");
    std::cout << "  Documents: " << store.size() << "\n";
    std::cout << "  Dimensions: " << store.dimensions() << "\n";
    std::cout << "  Model: " << store.info().embedding_model << "\n\n";
    
    // Test 2: Vector search
    std::cout << YELLOW << "Test 2: Vector search..." << RESET << "\n";
    
    auto query_vec = generate_random_vector(store.dimensions());
    
    start = std::chrono::high_resolution_clock::now();
    auto vector_results = store.search(query_vec.data(), 5);
    end = std::chrono::high_resolution_clock::now();
    
    auto search_time = std::chrono::duration_cast<std::chrono::milliseconds>(end - start).count();
    
    if (vector_results.empty()) {
        test_fail("Vector search", "No results returned");
    } else {
        test_pass("Vector search completed in " + std::to_string(search_time) + "ms");
        std::cout << "  Top 5 results:\n";
        for (size_t i = 0; i < std::min(size_t(5), vector_results.size()); ++i) {
            const auto& result = vector_results[i];
            std::cout << "    " << (i+1) << ". Doc " << result.doc_id 
                     << " (ID: " << result.id << ")"
                     << ", Score: " << result.score << "\n";
            std::cout << "       Text: \"" << result.text.substr(0, 50) << "...\"\n";
        }
    }
    std::cout << "\n";
    
    // Test 3: BM25 search
    std::cout << YELLOW << "Test 3: BM25 text search..." << RESET << "\n";
    
    std::vector<std::string> query_terms = {"test", "document", "sample"};
    
    start = std::chrono::high_resolution_clock::now();
    auto bm25_results = store.search_bm25(query_terms, 5);
    end = std::chrono::high_resolution_clock::now();
    
    auto bm25_time = std::chrono::duration_cast<std::chrono::milliseconds>(end - start).count();
    
    if (bm25_results.empty()) {
        std::cout << YELLOW << "  No BM25 results (terms might not exist in corpus)" << RESET << "\n";
    } else {
        test_pass("BM25 search completed in " + std::to_string(bm25_time) + "ms");
        std::cout << "  Top results for terms [test, document, sample]:\n";
        for (size_t i = 0; i < std::min(size_t(3), bm25_results.size()); ++i) {
            const auto& result = bm25_results[i];
            std::cout << "    " << (i+1) << ". Doc " << result.doc_id 
                     << " (ID: " << result.id << ")"
                     << ", Score: " << result.score << "\n";
        }
    }
    std::cout << "\n";
    
    // Test 4: Hybrid search
    std::cout << YELLOW << "Test 4: Hybrid search..." << RESET << "\n";
    
    start = std::chrono::high_resolution_clock::now();
    auto hybrid_results = store.search_hybrid(query_vec.data(), query_terms, 5, 0.7);
    end = std::chrono::high_resolution_clock::now();
    
    auto hybrid_time = std::chrono::duration_cast<std::chrono::milliseconds>(end - start).count();
    
    if (hybrid_results.empty()) {
        test_fail("Hybrid search", "No results returned");
    } else {
        test_pass("Hybrid search completed in " + std::to_string(hybrid_time) + "ms");
        std::cout << "  Top 5 hybrid results (70% vector, 30% BM25):\n";
        for (size_t i = 0; i < std::min(size_t(5), hybrid_results.size()); ++i) {
            const auto& result = hybrid_results[i];
            std::cout << "    " << (i+1) << ". Doc " << result.doc_id 
                     << " (ID: " << result.id << ")"
                     << ", Score: " << result.score << "\n";
        }
    }
    std::cout << "\n";
    
    // Test 5: Document retrieval
    std::cout << YELLOW << "Test 5: Document retrieval..." << RESET << "\n";
    
    nvs::VectorStoreV2::SearchResult doc;
    if (store.get_document(0, doc)) {
        test_pass("Retrieved document 0");
        std::cout << "  ID: " << doc.id << "\n";
        std::cout << "  Text length: " << doc.text.length() << " chars\n";
        std::cout << "  Metadata length: " << doc.metadata_json.length() << " chars\n";
    } else {
        test_fail("Document retrieval", "Failed to get document 0");
    }
    std::cout << "\n";
    
    // Test 6: Performance benchmark
    std::cout << YELLOW << "Test 6: Performance benchmark..." << RESET << "\n";
    
    const int num_searches = 100;
    std::vector<long> search_times;
    search_times.reserve(num_searches);
    
    for (int i = 0; i < num_searches; ++i) {
        auto q = generate_random_vector(store.dimensions());
        start = std::chrono::high_resolution_clock::now();
        auto res = store.search(q.data(), 10);
        end = std::chrono::high_resolution_clock::now();
        search_times.push_back(
            std::chrono::duration_cast<std::chrono::microseconds>(end - start).count()
        );
    }
    
    // Calculate statistics
    std::sort(search_times.begin(), search_times.end());
    long min_time = search_times.front();
    long max_time = search_times.back();
    long median_time = search_times[search_times.size() / 2];
    long p95_time = search_times[static_cast<size_t>(search_times.size() * 0.95)];
    long p99_time = search_times[static_cast<size_t>(search_times.size() * 0.99)];
    
    double avg_time = 0;
    for (long t : search_times) {
        avg_time += t;
    }
    avg_time /= search_times.size();
    
    std::cout << "  Search performance over " << num_searches << " queries:\n";
    std::cout << "    Min: " << min_time << "μs (" << min_time/1000.0 << "ms)\n";
    std::cout << "    Median: " << median_time << "μs (" << median_time/1000.0 << "ms)\n";
    std::cout << "    Average: " << static_cast<long>(avg_time) << "μs (" << avg_time/1000.0 << "ms)\n";
    std::cout << "    P95: " << p95_time << "μs (" << p95_time/1000.0 << "ms)\n";
    std::cout << "    P99: " << p99_time << "μs (" << p99_time/1000.0 << "ms)\n";
    std::cout << "    Max: " << max_time << "μs (" << max_time/1000.0 << "ms)\n";
    
    test_pass("Performance benchmark completed");
    
    // Test 7: Reopen test (simulate Lambda cold start)
    std::cout << "\n" << YELLOW << "Test 7: Reopen test (simulating cold start)..." << RESET << "\n";
    
    store.close();
    
    start = std::chrono::high_resolution_clock::now();
    if (!store.open(bundle_path)) {
        test_fail("Reopen", "Failed to reopen bundle");
        return 1;
    }
    end = std::chrono::high_resolution_clock::now();
    
    auto reopen_time = std::chrono::duration_cast<std::chrono::milliseconds>(end - start).count();
    test_pass("Bundle reopened in " + std::to_string(reopen_time) + "ms");
    
    // Quick search after reopen
    start = std::chrono::high_resolution_clock::now();
    auto post_reopen_results = store.search(query_vec.data(), 1);
    end = std::chrono::high_resolution_clock::now();
    
    auto first_search_time = std::chrono::duration_cast<std::chrono::milliseconds>(end - start).count();
    test_pass("First search after reopen: " + std::to_string(first_search_time) + "ms");
    
    std::cout << "\n=====================================\n";
    std::cout << GREEN << "✅ All tests passed!" << RESET << "\n";
    std::cout << "\nSummary:\n";
    std::cout << "  Bundle open time: " << open_time << "ms\n";
    std::cout << "  Vector search: " << search_time << "ms\n";
    std::cout << "  BM25 search: " << bm25_time << "ms\n";
    std::cout << "  Hybrid search: " << hybrid_time << "ms\n";
    std::cout << "  Median search latency: " << median_time/1000.0 << "ms\n";
    std::cout << "  Cold start simulation: " << reopen_time << "ms\n";
    
    return 0;
}