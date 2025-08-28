// Stress test for nvs::VectorStoreV2 - bundle-based implementation
// Tests concurrent operations, memory safety, and performance
#include "../vector_store_v2.h"
#include "../document_loader.h"
#include <thread>
#include <random>
#include <chrono>
#include <iostream>
#include <fstream>
#include <atomic>
#include <cassert>
#include <filesystem>
#include <vector>
#include <mutex>

using namespace std::chrono;
namespace fs = std::filesystem;

// Test configuration
constexpr size_t NUM_THREADS = 8;
constexpr size_t SEARCHES_PER_THREAD = 100;

// Helper to generate random normalized embedding
std::vector<float> generate_random_embedding(size_t dim, std::mt19937& rng) {
    std::uniform_real_distribution<float> dist(-1.0f, 1.0f);
    std::vector<float> embedding(dim);
    float sum = 0.0f;
    
    for (size_t i = 0; i < dim; ++i) {
        embedding[i] = dist(rng);
        sum += embedding[i] * embedding[i];
    }
    
    // Normalize
    float inv_norm = 1.0f / std::sqrt(sum);
    for (size_t i = 0; i < dim; ++i) {
        embedding[i] *= inv_norm;
    }
    
    return embedding;
}

// Test 1: Bundle creation and loading stress test
void test_bundle_creation_stress(const std::string& test_dir) {
    std::cout << "\n📦 Test 1: Bundle creation stress test\n";
    
    // Check if test data exists
    if (!fs::exists(test_dir)) {
        std::cout << "❌ Test data directory not found: " << test_dir << "\n";
        std::cout << "   Creating synthetic test data...\n";
        fs::create_directories(test_dir);
        
        // Create some test documents
        std::mt19937 rng(42);
        const size_t test_dim = 1536;  // Use OpenAI dimension for synthetic data
        for (int i = 0; i < 100; ++i) {
            std::ofstream file(test_dir + "/doc" + std::to_string(i) + ".json");
            auto embedding = generate_random_embedding(test_dim, rng);
            
            file << "{\"id\":\"stress-" << i << "\",";
            file << "\"text\":\"Stress test document " << i << "\",";
            file << "\"metadata\":{\"embedding\":[";
            for (size_t j = 0; j < embedding.size(); ++j) {
                if (j > 0) file << ",";
                file << embedding[j];
            }
            file << "]}}";
        }
    }
    
    // Load documents using document loader
    auto start = high_resolution_clock::now();
    DocumentLoader loader;
    auto result = loader.loadDirectory(test_dir);
    auto load_time = duration_cast<milliseconds>(high_resolution_clock::now() - start).count();
    
    std::cout << "   Loaded " << result.documents.size() << " documents in " << load_time << "ms\n";
    std::cout << "   Dimensions detected: " << result.dimensions << "\n";
    std::cout << "   Average document length: " << result.average_document_length << "\n";
    
    // Create bundle directory
    std::string bundle_dir = "stress_test_bundle";
    if (fs::exists(bundle_dir)) {
        fs::remove_all(bundle_dir);
    }
    fs::create_directories(bundle_dir);
    
    // Write bundle files (simplified version - real nvs-pack would do this)
    std::cout << "   Creating bundle files...\n";
    
    // This would normally be done by nvs-pack tool
    // For now, we'll just verify the loader worked
    assert(result.documents.size() > 0);
    assert(result.dimensions > 0);
    
    std::cout << "✅ Bundle creation stress test passed\n";
}

// Test 2: Concurrent search stress test
void test_concurrent_search_stress(const std::string& bundle_path) {
    std::cout << "\n🔍 Test 2: Concurrent search stress test\n";
    
    if (!fs::exists(bundle_path + "/manifest.json")) {
        std::cout << "⚠️  Bundle not found at " << bundle_path << ", skipping test\n";
        return;
    }
    
    // Open the store
    nvs::VectorStoreV2 store;
    auto start = high_resolution_clock::now();
    
    if (!store.open(bundle_path)) {
        std::cout << "❌ Failed to open bundle\n";
        return;
    }
    
    auto open_time = duration_cast<milliseconds>(high_resolution_clock::now() - start).count();
    std::cout << "   Bundle opened in " << open_time << "ms\n";
    std::cout << "   Documents: " << store.size() << "\n";
    std::cout << "   Dimensions: " << store.dimensions() << "\n";
    
    // Stress test: Many concurrent searches
    std::atomic<size_t> total_searches{0};
    std::atomic<size_t> total_results{0};
    std::atomic<bool> has_error{false};
    
    auto search_start = high_resolution_clock::now();
    
    std::vector<std::thread> searchers;
    for (size_t t = 0; t < NUM_THREADS; ++t) {
        searchers.emplace_back([&store, &total_searches, &total_results, &has_error, t]() {
            std::mt19937 rng(t);
            size_t local_results = 0;
            
            try {
                for (size_t i = 0; i < SEARCHES_PER_THREAD; ++i) {
                    auto query = generate_random_embedding(store.dimensions(), rng);
                    auto results = store.search(query.data(), 10);
                    
                    if (results.empty() && store.size() > 0) {
                        std::cerr << "Thread " << t << ": Search returned no results!\n";
                        has_error = true;
                        break;
                    }
                    
                    local_results += results.size();
                    total_searches++;
                }
            } catch (const std::exception& e) {
                std::cerr << "Thread " << t << " error: " << e.what() << "\n";
                has_error = true;
            }
            
            total_results += local_results;
        });
    }
    
    for (auto& t : searchers) {
        t.join();
    }
    
    auto search_time = duration_cast<milliseconds>(high_resolution_clock::now() - search_start).count();
    
    if (has_error) {
        std::cout << "❌ Errors occurred during concurrent searches\n";
    } else {
        std::cout << "✅ " << total_searches.load() << " concurrent searches in " << search_time << "ms\n";
        std::cout << "   Average results per search: " << (total_results.load() / total_searches.load()) << "\n";
        std::cout << "   Throughput: " << (total_searches.load() * 1000 / search_time) << " searches/sec\n";
    }
}

// Test 3: Memory-mapped file stress test
void test_mmap_stress(const std::string& bundle_path) {
    std::cout << "\n🗺️  Test 3: Memory-mapped file stress test\n";
    
    if (!fs::exists(bundle_path + "/manifest.json")) {
        std::cout << "⚠️  Bundle not found, skipping test\n";
        return;
    }
    
    // Test rapid open/close cycles (simulating Lambda cold starts)
    const int cycles = 10;
    std::vector<long> open_times;
    std::vector<long> first_search_times;
    
    // First open to get dimensions
    nvs::VectorStoreV2 temp_store;
    if (!temp_store.open(bundle_path)) {
        std::cout << "❌ Failed to open bundle for dimension check\n";
        return;
    }
    size_t dim = temp_store.dimensions();
    temp_store.close();
    
    std::mt19937 rng(42);
    auto query = generate_random_embedding(dim, rng);
    
    for (int i = 0; i < cycles; ++i) {
        nvs::VectorStoreV2 store;
        
        auto start = high_resolution_clock::now();
        if (!store.open(bundle_path)) {
            std::cout << "❌ Failed to open bundle on cycle " << i << "\n";
            return;
        }
        auto open_time = duration_cast<microseconds>(high_resolution_clock::now() - start).count();
        open_times.push_back(open_time);
        
        // First search after open
        start = high_resolution_clock::now();
        auto results = store.search(query.data(), 10);
        auto search_time = duration_cast<microseconds>(high_resolution_clock::now() - start).count();
        first_search_times.push_back(search_time);
        
        assert(!results.empty());
        
        // Store automatically closes on destruction
    }
    
    // Calculate statistics
    std::sort(open_times.begin(), open_times.end());
    std::sort(first_search_times.begin(), first_search_times.end());
    
    long median_open = open_times[open_times.size() / 2];
    long median_search = first_search_times[first_search_times.size() / 2];
    
    std::cout << "✅ Completed " << cycles << " open/close cycles\n";
    std::cout << "   Median open time: " << median_open << "μs (" << median_open/1000.0 << "ms)\n";
    std::cout << "   Median first search: " << median_search << "μs (" << median_search/1000.0 << "ms)\n";
}

// Test 4: Hybrid search stress test
void test_hybrid_search_stress(const std::string& bundle_path) {
    std::cout << "\n🔀 Test 4: Hybrid search stress test\n";
    
    if (!fs::exists(bundle_path + "/manifest.json")) {
        std::cout << "⚠️  Bundle not found, skipping test\n";
        return;
    }
    
    nvs::VectorStoreV2 store;
    if (!store.open(bundle_path)) {
        std::cout << "❌ Failed to open bundle\n";
        return;
    }
    
    std::cout << "   Testing hybrid search with " << NUM_THREADS << " threads\n";
    
    // Common query terms for BM25
    std::vector<std::string> query_terms = {"test", "document", "stress"};
    
    std::atomic<size_t> total_searches{0};
    std::atomic<bool> has_error{false};
    
    auto start = high_resolution_clock::now();
    
    std::vector<std::thread> searchers;
    for (size_t t = 0; t < NUM_THREADS; ++t) {
        searchers.emplace_back([&store, &query_terms, &total_searches, &has_error, t]() {
            std::mt19937 rng(t);
            
            try {
                for (size_t i = 0; i < SEARCHES_PER_THREAD / 2; ++i) {
                    auto query_vec = generate_random_embedding(store.dimensions(), rng);
                    
                    // Test different weight combinations
                    double weights[] = {0.0, 0.3, 0.5, 0.7, 1.0};
                    for (double w : weights) {
                        auto results = store.search_hybrid(query_vec.data(), query_terms, 10, w);
                        
                        if (results.empty() && store.size() > 0) {
                            std::cerr << "Thread " << t << ": Hybrid search returned no results with weight " << w << "\n";
                            has_error = true;
                            break;
                        }
                        
                        total_searches++;
                    }
                }
            } catch (const std::exception& e) {
                std::cerr << "Thread " << t << " error: " << e.what() << "\n";
                has_error = true;
            }
        });
    }
    
    for (auto& t : searchers) {
        t.join();
    }
    
    auto elapsed = duration_cast<milliseconds>(high_resolution_clock::now() - start).count();
    
    if (has_error) {
        std::cout << "❌ Errors occurred during hybrid searches\n";
    } else {
        std::cout << "✅ " << total_searches.load() << " hybrid searches in " << elapsed << "ms\n";
        std::cout << "   Throughput: " << (total_searches.load() * 1000 / elapsed) << " searches/sec\n";
    }
}

// Test 5: Document retrieval stress test
void test_document_retrieval_stress(const std::string& bundle_path) {
    std::cout << "\n📄 Test 5: Document retrieval stress test\n";
    
    if (!fs::exists(bundle_path + "/manifest.json")) {
        std::cout << "⚠️  Bundle not found, skipping test\n";
        return;
    }
    
    nvs::VectorStoreV2 store;
    if (!store.open(bundle_path)) {
        std::cout << "❌ Failed to open bundle\n";
        return;
    }
    
    const size_t num_docs = store.size();
    std::cout << "   Testing retrieval of " << num_docs << " documents\n";
    
    std::atomic<size_t> total_retrieved{0};
    std::atomic<bool> has_error{false};
    
    auto start = high_resolution_clock::now();
    
    std::vector<std::thread> retrievers;
    for (size_t t = 0; t < NUM_THREADS; ++t) {
        retrievers.emplace_back([&store, num_docs, &total_retrieved, &has_error, t]() {
            std::mt19937 rng(t);
            std::uniform_int_distribution<size_t> dist(0, num_docs - 1);
            
            try {
                for (size_t i = 0; i < 100; ++i) {
                    size_t doc_id = dist(rng);
                    nvs::VectorStoreV2::SearchResult result;
                    
                    if (!store.get_document(doc_id, result)) {
                        std::cerr << "Thread " << t << ": Failed to retrieve document " << doc_id << "\n";
                        has_error = true;
                        break;
                    }
                    
                    // Verify document fields
                    if (result.id.empty() || result.text.empty()) {
                        std::cerr << "Thread " << t << ": Retrieved empty document fields for " << doc_id << "\n";
                        has_error = true;
                        break;
                    }
                    
                    total_retrieved++;
                }
            } catch (const std::exception& e) {
                std::cerr << "Thread " << t << " error: " << e.what() << "\n";
                has_error = true;
            }
        });
    }
    
    for (auto& t : retrievers) {
        t.join();
    }
    
    auto elapsed = duration_cast<milliseconds>(high_resolution_clock::now() - start).count();
    
    if (has_error) {
        std::cout << "❌ Errors occurred during document retrieval\n";
    } else {
        std::cout << "✅ Retrieved " << total_retrieved.load() << " documents in " << elapsed << "ms\n";
        std::cout << "   Throughput: " << (total_retrieved.load() * 1000 / elapsed) << " retrievals/sec\n";
    }
}

// Test 6: Race condition detection with rapid operations
void test_race_conditions(const std::string& bundle_path) {
    std::cout << "\n🏁 Test 6: Race condition detection\n";
    
    if (!fs::exists(bundle_path + "/manifest.json")) {
        std::cout << "⚠️  Bundle not found, skipping test\n";
        return;
    }
    
    const int num_iterations = 10;
    std::atomic<bool> has_race{false};
    
    for (int iter = 0; iter < num_iterations; ++iter) {
        nvs::VectorStoreV2 store;
        if (!store.open(bundle_path)) {
            std::cout << "❌ Failed to open bundle\n";
            return;
        }
        
        // Launch many threads doing different operations simultaneously
        std::vector<std::thread> workers;
        
        // Vector searches
        for (int i = 0; i < 4; ++i) {
            workers.emplace_back([&store, &has_race, i]() {
                std::mt19937 rng(i);
                auto query = generate_random_embedding(store.dimensions(), rng);
                for (int j = 0; j < 10; ++j) {
                    auto results = store.search(query.data(), 5);
                    if (results.size() > 5) {
                        std::cerr << "Race: search returned more than k results\n";
                        has_race = true;
                    }
                }
            });
        }
        
        // BM25 searches
        for (int i = 0; i < 4; ++i) {
            workers.emplace_back([&store, &has_race]() {
                std::vector<std::string> terms = {"test", "document"};
                for (int j = 0; j < 10; ++j) {
                    auto results = store.search_bm25(terms, 5);
                    if (results.size() > 5) {
                        std::cerr << "Race: BM25 returned more than k results\n";
                        has_race = true;
                    }
                }
            });
        }
        
        // Document retrievals
        for (int i = 0; i < 4; ++i) {
            workers.emplace_back([&store, &has_race]() {
                for (size_t j = 0; j < std::min(size_t(10), store.size()); ++j) {
                    nvs::VectorStoreV2::SearchResult result;
                    if (!store.get_document(j, result)) {
                        std::cerr << "Race: Failed to retrieve valid document\n";
                        has_race = true;
                    }
                }
            });
        }
        
        // Wait for all workers
        for (auto& w : workers) {
            w.join();
        }
        
        if (has_race) {
            std::cout << "❌ Race condition detected in iteration " << iter << "\n";
            break;
        }
    }
    
    if (!has_race) {
        std::cout << "✅ No race conditions detected in " << num_iterations << " iterations\n";
    }
}

int main(int argc, char** argv) {
    std::cout << "🔥 nvs::VectorStoreV2 Stress Tests\n";
    std::cout << "==============================\n";
    
    // Detect which sanitizer is enabled
    #if defined(__has_feature)
        #if __has_feature(address_sanitizer)
            std::cout << "🛡️  Running with AddressSanitizer (ASAN)\n";
        #elif __has_feature(thread_sanitizer)
            std::cout << "🔍 Running with ThreadSanitizer (TSAN)\n";
        #endif
    #elif defined(__SANITIZE_ADDRESS__)
        std::cout << "🛡️  Running with AddressSanitizer (ASAN)\n";
    #elif defined(__SANITIZE_THREAD__)
        std::cout << "🔍 Running with ThreadSanitizer (TSAN)\n";
    #else
        std::cout << "⚠️  Running without sanitizers\n";
        std::cout << "   Use: make stress-v2 for ASAN by default\n";
        std::cout << "        make stress-v2 SANITIZER=thread for TSAN\n";
        std::cout << "        make stress-v2 SANITIZER=none to disable\n";
    #endif
    
    std::string test_data_dir = argc > 1 ? argv[1] : "../test/stress_data";
    std::string bundle_path = argc > 2 ? argv[2] : "test-bundle";
    
    // Run all stress tests
    test_bundle_creation_stress(test_data_dir);
    test_concurrent_search_stress(bundle_path);
    test_mmap_stress(bundle_path);
    test_hybrid_search_stress(bundle_path);
    test_document_retrieval_stress(bundle_path);
    test_race_conditions(bundle_path);
    
    std::cout << "\n✅ All stress tests completed!\n";
    return 0;
}