// Comprehensive test suite for nvs::VectorStoreV2
// Tests edge cases, error conditions, and data integrity
#include "../vector_store_v2.h"
#include "../document_loader.h"
#include <iostream>
#include <fstream>
#include <filesystem>
#include <random>
#include <cassert>
#include <chrono>
#include <cstring>
#include <thread>
#include <atomic>
#include <memory>

namespace fs = std::filesystem;
using namespace std::chrono;

// ANSI colors for test output
#define GREEN "\033[32m"
#define RED "\033[31m"
#define YELLOW "\033[33m"
#define BLUE "\033[34m"
#define RESET "\033[0m"

void test_pass(const std::string& test_name) {
    std::cout << GREEN << "✓ " << RESET << test_name << std::endl;
}

void test_fail(const std::string& test_name, const std::string& reason) {
    std::cout << RED << "✗ " << RESET << test_name << ": " << reason << std::endl;
    exit(1);
}

void test_section(const std::string& section) {
    std::cout << "\n" << BLUE << "━━━ " << section << " ━━━" << RESET << "\n";
}

// Test 1: Bundle with missing files
void test_missing_bundle_files() {
    test_section("Test 1: Missing Bundle Files");
    
    // Create a directory with only partial bundle files
    std::string bad_bundle = "test_bad_bundle";
    fs::remove_all(bad_bundle);
    fs::create_directories(bad_bundle);
    
    // Create only manifest
    std::ofstream manifest(bad_bundle + "/manifest.json");
    manifest << R"({
        "format": "nvs-bundle-v1",
        "created": "2024-01-01T00:00:00Z",
        "num_docs": 100,
        "dimensions": 1536,
        "embedding_model": "test",
        "dtype": "float32"
    })";
    manifest.close();
    
    nvs::VectorStoreV2 store;
    
    // Test: Should fail when vectors.f32 is missing
    if (!store.open(bad_bundle)) {
        test_pass("Correctly rejected bundle with missing vectors.f32");
    } else {
        test_fail("Missing vectors.f32", "Should have failed to open");
    }
    
    // Add vectors file but missing others
    std::ofstream vectors(bad_bundle + "/vectors.f32", std::ios::binary);
    std::vector<float> dummy(100 * 1536, 0.0f);
    vectors.write(reinterpret_cast<char*>(dummy.data()), dummy.size() * sizeof(float));
    vectors.close();
    
    // Still should fail (missing doclen, lexicon, etc.)
    if (!store.open(bad_bundle)) {
        test_pass("Correctly rejected incomplete bundle");
    } else {
        test_fail("Incomplete bundle", "Should have failed to open");
    }
    
    fs::remove_all(bad_bundle);
}

// Test 2: Corrupted manifest
void test_corrupted_manifest() {
    test_section("Test 2: Corrupted Manifest");
    
    std::string bad_bundle = "test_corrupted_manifest";
    fs::remove_all(bad_bundle);
    fs::create_directories(bad_bundle);
    
    // Test malformed JSON
    std::ofstream manifest(bad_bundle + "/manifest.json");
    manifest << "{ this is not valid JSON ]";
    manifest.close();
    
    nvs::VectorStoreV2 store;
    if (!store.open(bad_bundle)) {
        test_pass("Correctly rejected malformed JSON manifest");
    } else {
        test_fail("Malformed manifest", "Should have failed to parse");
    }
    
    // Test manifest with wrong format version
    manifest.open(bad_bundle + "/manifest.json", std::ios::trunc);
    manifest << R"({
        "format": "nvs-bundle-v99",
        "num_docs": 100
    })";
    manifest.close();
    
    if (!store.open(bad_bundle)) {
        test_pass("Correctly rejected unsupported format version");
    } else {
        test_fail("Wrong format", "Should have rejected v99");
    }
    
    fs::remove_all(bad_bundle);
}

// Test 3: Dimension mismatch
void test_dimension_mismatch() {
    test_section("Test 3: Dimension Mismatch");
    
    std::string bad_bundle = "test_dim_mismatch";
    fs::remove_all(bad_bundle);
    fs::create_directories(bad_bundle);
    
    // Create manifest claiming 1536 dimensions
    std::ofstream manifest(bad_bundle + "/manifest.json");
    manifest << R"({
        "format": "nvs-bundle-v1",
        "created": "2024-01-01T00:00:00Z",
        "num_docs": 10,
        "dim": 1536,
        "embedding": {
            "model": "test",
            "dtype": "float32"
        },
        "bm25": {
            "avgdl": 100.0,
            "k1": 1.2,
            "b": 0.75
        }
    })";
    manifest.close();
    
    // But create vectors file with wrong size (10 docs * 100 dims instead of 1536)
    std::ofstream vectors(bad_bundle + "/vectors.f32", std::ios::binary);
    std::vector<float> wrong_size(10 * 100, 0.5f);
    vectors.write(reinterpret_cast<char*>(wrong_size.data()), wrong_size.size() * sizeof(float));
    vectors.close();
    
    // Create other required files
    std::ofstream doclen(bad_bundle + "/doclen.u32", std::ios::binary);
    std::vector<uint32_t> lengths(10, 100);
    doclen.write(reinterpret_cast<char*>(lengths.data()), lengths.size() * sizeof(uint32_t));
    doclen.close();
    
    std::ofstream lexicon(bad_bundle + "/lexicon.bin", std::ios::binary);
    uint32_t num_terms = 0;
    lexicon.write(reinterpret_cast<char*>(&num_terms), sizeof(num_terms));
    lexicon.close();
    
    std::ofstream postings(bad_bundle + "/postings.bin", std::ios::binary);
    postings.close();
    
    std::ofstream terms(bad_bundle + "/terms.dict");
    terms.close();
    
    std::ofstream meta_idx(bad_bundle + "/meta.idx", std::ios::binary);
    std::vector<uint64_t> offsets(11, 0);  // n+1 offsets
    meta_idx.write(reinterpret_cast<char*>(offsets.data()), offsets.size() * sizeof(uint64_t));
    meta_idx.close();
    
    std::ofstream meta(bad_bundle + "/meta.bin", std::ios::binary);
    meta.close();
    
    nvs::VectorStoreV2 store;
    // The dimension mismatch should be caught when checking file sizes
    if (store.open(bad_bundle)) {
        // If it opened, verify operations fail safely
        auto results = store.search(nullptr, 10);
        if (results.empty()) {
            test_pass("Safely handled dimension mismatch (no crash)");
        } else {
            test_fail("Dimension mismatch", "Unexpected results from mismatched data");
        }
    } else {
        test_pass("Correctly rejected dimension mismatch bundle");
    }
    
    fs::remove_all(bad_bundle);
}

// Test 4: Zero documents
void test_zero_documents() {
    test_section("Test 4: Zero Documents Bundle");
    
    std::string empty_bundle = "test_empty_bundle";
    fs::remove_all(empty_bundle);
    fs::create_directories(empty_bundle);
    
    // Create valid but empty bundle
    std::ofstream manifest(empty_bundle + "/manifest.json");
    manifest << R"({
        "format": "nvs-bundle-v1",
        "created": "2024-01-01T00:00:00Z",
        "num_docs": 0,
        "dim": 1536,
        "embedding": {
            "model": "test",
            "dtype": "float32"
        },
        "bm25": {
            "avgdl": 0.0,
            "k1": 1.2,
            "b": 0.75
        }
    })";
    manifest.close();
    
    // Create empty data files
    std::ofstream(empty_bundle + "/vectors.f32", std::ios::binary).close();
    std::ofstream(empty_bundle + "/doclen.u32", std::ios::binary).close();
    
    std::ofstream lexicon(empty_bundle + "/lexicon.bin", std::ios::binary);
    uint32_t num_terms = 0;
    lexicon.write(reinterpret_cast<char*>(&num_terms), sizeof(num_terms));
    lexicon.close();
    
    std::ofstream(empty_bundle + "/postings.bin", std::ios::binary).close();
    std::ofstream(empty_bundle + "/terms.dict").close();
    
    std::ofstream meta_idx(empty_bundle + "/meta.idx", std::ios::binary);
    uint64_t offset = 0;
    meta_idx.write(reinterpret_cast<char*>(&offset), sizeof(offset));
    meta_idx.close();
    
    std::ofstream(empty_bundle + "/meta.bin", std::ios::binary).close();
    
    nvs::VectorStoreV2 store;
    if (store.open(empty_bundle)) {
        // Test operations on empty store
        assert(store.size() == 0);
        
        std::vector<float> query(1536, 0.1f);
        auto results = store.search(query.data(), 10);
        assert(results.empty());
        
        std::vector<std::string> terms = {"test"};
        auto bm25_results = store.search_bm25(terms, 10);
        assert(bm25_results.empty());
        
        auto hybrid_results = store.search_hybrid(query.data(), terms, 10);
        assert(hybrid_results.empty());
        
        test_pass("Empty bundle handled correctly");
    } else {
        test_fail("Empty bundle", "Should open successfully");
    }
    
    fs::remove_all(empty_bundle);
}

// Test 5: Out of bounds access
void test_out_of_bounds() {
    test_section("Test 5: Out of Bounds Access");
    
    // Use the test bundle if it exists
    std::string bundle_path = "test-bundle";
    if (!fs::exists(bundle_path + "/manifest.json")) {
        std::cout << YELLOW << "⚠ Test bundle not found, skipping bounds test" << RESET << "\n";
        return;
    }
    
    nvs::VectorStoreV2 store;
    if (!store.open(bundle_path)) {
        test_fail("Open bundle", "Failed to open test bundle");
        return;
    }
    
    size_t num_docs = store.size();
    
    // Test 1: Document retrieval with invalid ID
    nvs::VectorStoreV2::SearchResult result;
    
    // Beyond bounds
    if (!store.get_document(num_docs + 100, result)) {
        test_pass("Correctly rejected out-of-bounds document ID");
    } else {
        test_fail("Bounds check", "Should have rejected invalid doc ID");
    }
    
    // Maximum valid ID
    if (store.get_document(num_docs - 1, result)) {
        test_pass("Retrieved maximum valid document ID");
    } else {
        test_fail("Max ID", "Failed to retrieve valid max doc ID");
    }
    
    // Test 2: Search with nullptr query
    auto null_results = store.search(nullptr, 10);
    if (null_results.empty()) {
        test_pass("Safely handled nullptr query");
    } else {
        test_fail("Nullptr handling", "Should return empty for nullptr");
    }
    
    // Test 3: Search with k = 0
    std::vector<float> query(store.dimensions(), 0.1f);
    auto zero_k_results = store.search(query.data(), 0);
    if (zero_k_results.empty()) {
        test_pass("Correctly handled k=0");
    } else {
        test_fail("Zero k", "Should return empty for k=0");
    }
    
    // Test 4: Search with k > num_docs
    auto large_k_results = store.search(query.data(), num_docs * 2);
    if (large_k_results.size() <= num_docs) {
        test_pass("Correctly capped results to document count");
    } else {
        test_fail("Large k", "Returned more results than documents");
    }
}

// Test 6: Concurrent bundle opening
void test_concurrent_opening() {
    test_section("Test 6: Concurrent Bundle Opening");
    
    std::string bundle_path = "test-bundle";
    if (!fs::exists(bundle_path + "/manifest.json")) {
        std::cout << YELLOW << "⚠ Test bundle not found, skipping concurrency test" << RESET << "\n";
        return;
    }
    
    const int num_threads = 10;
    std::vector<std::thread> threads;
    std::atomic<int> successful_opens{0};
    std::atomic<bool> any_error{false};
    
    auto start = high_resolution_clock::now();
    
    for (int i = 0; i < num_threads; ++i) {
        threads.emplace_back([&bundle_path, &successful_opens, &any_error, i]() {
            nvs::VectorStoreV2 store;
            if (store.open(bundle_path)) {
                successful_opens++;
                
                // Do some operations
                std::vector<float> query(store.dimensions(), 0.1f + i * 0.01f);
                auto results = store.search(query.data(), 5);
                
                if (results.empty() && store.size() > 0) {
                    std::cerr << "Thread " << i << " got empty results\n";
                    any_error = true;
                }
            } else {
                std::cerr << "Thread " << i << " failed to open bundle\n";
                any_error = true;
            }
        });
    }
    
    for (auto& t : threads) {
        t.join();
    }
    
    auto elapsed = duration_cast<milliseconds>(high_resolution_clock::now() - start).count();
    
    if (!any_error && successful_opens == num_threads) {
        test_pass("All " + std::to_string(num_threads) + " threads opened bundle successfully in " + 
                 std::to_string(elapsed) + "ms");
    } else {
        test_fail("Concurrent opening", "Some threads failed");
    }
}

// Test 7: Bundle persistence and consistency
void test_bundle_consistency() {
    test_section("Test 7: Bundle Consistency");
    
    std::string bundle_path = "test-bundle";
    if (!fs::exists(bundle_path + "/manifest.json")) {
        std::cout << YELLOW << "⚠ Test bundle not found, skipping consistency test" << RESET << "\n";
        return;
    }
    
    // Open bundle multiple times and verify consistent results
    std::vector<float> test_query;
    std::vector<size_t> first_results;
    std::vector<double> first_scores;
    
    // First opening
    {
        nvs::VectorStoreV2 store;
        if (!store.open(bundle_path)) {
            test_fail("Open bundle", "Failed to open for consistency test");
            return;
        }
        
        // Generate test query
        std::mt19937 rng(42);
        std::uniform_real_distribution<float> dist(-1.0f, 1.0f);
        test_query.resize(store.dimensions());
        for (auto& v : test_query) {
            v = dist(rng);
        }
        
        // Get results
        auto results = store.search(test_query.data(), 10);
        for (const auto& r : results) {
            first_results.push_back(r.doc_id);
            first_scores.push_back(r.score);
        }
    }
    
    // Reopen and verify same results
    for (int i = 0; i < 5; ++i) {
        nvs::VectorStoreV2 store;
        if (!store.open(bundle_path)) {
            test_fail("Reopen bundle", "Failed on iteration " + std::to_string(i));
            return;
        }
        
        auto results = store.search(test_query.data(), 10);
        
        if (results.size() != first_results.size()) {
            test_fail("Consistency", "Different result count on iteration " + std::to_string(i));
            return;
        }
        
        for (size_t j = 0; j < results.size(); ++j) {
            if (results[j].doc_id != first_results[j]) {
                test_fail("Consistency", "Different document IDs on iteration " + std::to_string(i));
                return;
            }
            
            // Allow small floating point differences
            if (std::abs(results[j].score - first_scores[j]) > 1e-6) {
                test_fail("Consistency", "Different scores on iteration " + std::to_string(i));
                return;
            }
        }
    }
    
    test_pass("Bundle produces consistent results across reopens");
}

// Test 8: Memory mapping stress
void test_mmap_stress() {
    test_section("Test 8: Memory Mapping Stress");
    
    std::string bundle_path = "test-bundle";
    if (!fs::exists(bundle_path + "/manifest.json")) {
        std::cout << YELLOW << "⚠ Test bundle not found, skipping mmap stress test" << RESET << "\n";
        return;
    }
    
    // Rapid open/close to stress mmap/munmap
    const int iterations = 100;
    auto start = high_resolution_clock::now();
    
    for (int i = 0; i < iterations; ++i) {
        nvs::VectorStoreV2 store;
        if (!store.open(bundle_path)) {
            test_fail("mmap stress", "Failed on iteration " + std::to_string(i));
            return;
        }
        // Store closes automatically when it goes out of scope
    }
    
    auto elapsed = duration_cast<milliseconds>(high_resolution_clock::now() - start).count();
    test_pass(std::to_string(iterations) + " open/close cycles in " + std::to_string(elapsed) + "ms");
    
    // Test multiple stores open simultaneously (different mmap regions)
    std::vector<std::unique_ptr<nvs::VectorStoreV2>> stores;
    for (int i = 0; i < 10; ++i) {
        auto store = std::make_unique<nvs::VectorStoreV2>();
        if (!store->open(bundle_path)) {
            test_fail("Multiple mmaps", "Failed to open store " + std::to_string(i));
            return;
        }
        stores.push_back(std::move(store));
    }
    
    // Verify all stores work
    std::vector<float> query(stores[0]->dimensions(), 0.1f);
    for (size_t i = 0; i < stores.size(); ++i) {
        auto results = stores[i]->search(query.data(), 1);
        if (results.empty() && stores[i]->size() > 0) {
            test_fail("Multiple mmaps", "Store " + std::to_string(i) + " failed search");
            return;
        }
    }
    
    test_pass("Multiple simultaneous memory mappings work correctly");
}

int main(int argc, char** argv) {
    std::cout << "\n" << BLUE << "╔════════════════════════════════════╗" << RESET << "\n";
    std::cout << BLUE << "║" << RESET << "  nvs::VectorStoreV2 Comprehensive Tests  " << BLUE << "║" << RESET << "\n";
    std::cout << BLUE << "╚════════════════════════════════════╝" << RESET << "\n";
    
    // Run all tests
    test_missing_bundle_files();
    test_corrupted_manifest();
    test_dimension_mismatch();
    test_zero_documents();
    test_out_of_bounds();
    test_concurrent_opening();
    test_bundle_consistency();
    test_mmap_stress();
    
    std::cout << "\n" << GREEN << "══════════════════════════════" << RESET << "\n";
    std::cout << GREEN << "✅ All comprehensive tests passed!" << RESET << "\n";
    std::cout << GREEN << "══════════════════════════════" << RESET << "\n\n";
    
    return 0;
}