#include "vector_store.h"
#include <thread>
#include <random>
#include <chrono>
#include <iostream>
#include <atomic>
#include <cassert>
#include <sstream>
#include <iomanip>

using namespace std::chrono;

// Test configuration
constexpr size_t DIM = 1536;
constexpr size_t DOCS_PER_THREAD = 125;  // 1K total with 8 threads
constexpr size_t NUM_THREADS = 8;
constexpr size_t SEARCH_ITERATIONS = 100;

// Helper to generate random embedding
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

// Helper to create JSON document
std::string create_json_document(const std::string& id, const std::string& text, 
                                const std::vector<float>& embedding) {
    std::stringstream json;
    json << "{\"id\":\"" << id << "\",\"text\":\"" << text << "\",\"metadata\":{\"embedding\":[";
    
    for (size_t i = 0; i < embedding.size(); ++i) {
        if (i > 0) json << ",";
        json << std::fixed << std::setprecision(6) << embedding[i];
    }
    
    json << "]}}";
    return json.str();
}

// Test 1: 1K parallel inserts with 8 threads
void test_concurrent_inserts() {
    std::cout << "\n📝 Test 1: 1K parallel inserts with " << NUM_THREADS << " threads\n";
    
    VectorStore store(DIM);
    std::atomic<size_t> total_success{0};
    std::atomic<size_t> total_failures{0};
    auto start = high_resolution_clock::now();
    
    std::vector<std::thread> threads;
    
    for (size_t t = 0; t < NUM_THREADS; ++t) {
        threads.emplace_back([&store, &total_success, &total_failures, t]() {
            std::mt19937 rng(t);
            simdjson::ondemand::parser parser;
            size_t success = 0;
            size_t failures = 0;
            
            for (size_t i = 0; i < DOCS_PER_THREAD; ++i) {
                auto embedding = generate_random_embedding(DIM, rng);
                std::string id = "thread-" + std::to_string(t) + "-doc-" + std::to_string(i);
                std::string text = "Document " + std::to_string(i) + " from thread " + std::to_string(t);
                
                std::string json_str = create_json_document(id, text, embedding);
                simdjson::padded_string padded(json_str);
                
                simdjson::ondemand::document doc;
                auto error = parser.iterate(padded).get(doc);
                if (!error) {
                    error = store.add_document(doc);
                    if (!error) {
                        success++;
                    } else {
                        failures++;
                    }
                } else {
                    failures++;
                }
            }
            
            total_success += success;
            total_failures += failures;
        });
    }
    
    for (auto& t : threads) {
        t.join();
    }
    
    auto end = high_resolution_clock::now();
    auto elapsed = duration_cast<milliseconds>(end - start).count();
    
    std::cout << "✅ Inserted " << total_success.load() << " documents in " << elapsed << "ms\n";
    std::cout << "   Failed insertions: " << total_failures.load() << "\n";
    std::cout << "   Store size: " << store.size() << "\n";
    std::cout << "   Rate: " << (total_success.load() * 1000 / elapsed) << " docs/sec\n";
    
    // Verify store size matches successful insertions
    assert(store.size() == total_success.load());
}

// Test 2: 64MB+1 allocation (expect fail)
void test_oversize_allocation() {
    std::cout << "\n📏 Test 2: 64MB+1 allocation (expect fail)\n";
    
    VectorStore store(10);
    
    // Create a document with metadata that exceeds chunk size
    std::stringstream huge_json;
    huge_json << "{\"id\":\"huge\",\"text\":\"test\",\"metadata\":{\"embedding\":[";
    for (int i = 0; i < 10; ++i) {
        if (i > 0) huge_json << ",";
        huge_json << "0.1";
    }
    huge_json << "],\"huge\":\"";
    // Add 64MB + 1 byte of data
    for (size_t i = 0; i < 67108865; ++i) {
        huge_json << "x";
    }
    huge_json << "\"}}";
    
    std::string json_str = huge_json.str();
    simdjson::padded_string padded(json_str);
    simdjson::ondemand::parser parser;
    simdjson::ondemand::document doc;
    
    auto error = parser.iterate(padded).get(doc);
    if (!error) {
        // This should fail in the allocator
        error = store.add_document(doc);
        if (error == simdjson::MEMALLOC) {
            std::cout << "✅ Correctly rejected oversize allocation\n";
        } else {
            std::cout << "❌ Should have failed with MEMALLOC error, got: " << simdjson::error_message(error) << "\n";
            std::exit(1);
        }
    } else {
        std::cout << "❌ Failed to parse test JSON: " << simdjson::error_message(error) << "\n";
        std::exit(1);
    }
}

// Test 3: Alignment requests
void test_alignment_requests() {
    std::cout << "\n🎯 Test 3: Various alignment requests\n";
    
    class TestArenaAllocator : public ArenaAllocator {
    public:
        void test_alignments() {
            // Test valid alignments
            size_t valid_aligns[] = {1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096};
            
            for (size_t align : valid_aligns) {
                void* ptr = allocate(128, align);
                if (!ptr) {
                    std::cout << "❌ Failed to allocate with alignment " << align << "\n";
                    std::exit(1);
                }
                assert(((uintptr_t)ptr % align) == 0);
            }
            std::cout << "✅ All valid alignments handled correctly\n";
            
            // Test invalid alignment (>4096)
            void* ptr = allocate(128, 8192);
            if (ptr) {
                std::cout << "❌ Should have rejected alignment > 4096\n";
                std::exit(1);
            } else {
                std::cout << "✅ Correctly rejected large alignment\n";
            }
        }
    };
    
    TestArenaAllocator allocator;
    allocator.test_alignments();
}

// Test 4: Phase separation - load, finalize, then search
void test_phase_separation() {
    std::cout << "\n🔄 Test 4: Phase separation - load, finalize, then search\n";
    
    VectorStore store(DIM);
    auto start = high_resolution_clock::now();
    
    // Phase 1: Load documents (single-threaded for simplicity)
    std::mt19937 rng(42);
    simdjson::ondemand::parser parser;
    size_t docs_loaded = 0;
    
    for (size_t i = 0; i < 1000; ++i) {
        auto embedding = generate_random_embedding(DIM, rng);
        std::string json_str = create_json_document(
            "doc-" + std::to_string(i),
            "Document " + std::to_string(i),
            embedding
        );
        
        simdjson::padded_string padded(json_str);
        simdjson::ondemand::document doc;
        if (!parser.iterate(padded).get(doc)) {
            auto error = store.add_document(doc);
            if (!error) {
                docs_loaded++;
            }
        }
    }
    
    auto load_time = duration_cast<milliseconds>(high_resolution_clock::now() - start).count();
    std::cout << "   Loaded " << docs_loaded << " documents in " << load_time << "ms\n";
    
    // Verify searches fail before finalization
    auto query = generate_random_embedding(DIM, rng);
    auto results = store.search(query.data(), 10);
    assert(results.empty());
    std::cout << "   ✅ Searches correctly blocked before finalization\n";
    
    // Phase 2: Finalize the store
    auto finalize_start = high_resolution_clock::now();
    store.finalize();
    auto finalize_time = duration_cast<milliseconds>(high_resolution_clock::now() - finalize_start).count();
    std::cout << "   Finalized (normalized) in " << finalize_time << "ms\n";
    
    // Verify no more documents can be added
    {
        auto embedding = generate_random_embedding(DIM, rng);
        std::string json_str = create_json_document("blocked", "Should fail", embedding);
        simdjson::padded_string padded(json_str);
        simdjson::ondemand::document doc;
        parser.iterate(padded).get(doc);
        auto error = store.add_document(doc);
        assert(error == simdjson::INCORRECT_TYPE);
        std::cout << "   ✅ Document additions correctly blocked after finalization\n";
    }
    
    // Phase 3: Concurrent searches (multiple threads)
    std::atomic<size_t> total_searches{0};
    auto search_start = high_resolution_clock::now();
    
    std::vector<std::thread> searchers;
    for (size_t t = 0; t < 4; ++t) {
        searchers.emplace_back([&store, &total_searches, t]() {
            std::mt19937 rng(t);
            for (size_t i = 0; i < 25; ++i) {
                auto query = generate_random_embedding(DIM, rng);
                auto results = store.search(query.data(), 10);
                assert(!results.empty() && results.size() <= 10);
                total_searches++;
            }
        });
    }
    
    for (auto& t : searchers) {
        t.join();
    }
    
    auto search_time = duration_cast<milliseconds>(high_resolution_clock::now() - search_start).count();
    std::cout << "   Performed " << total_searches.load() << " concurrent searches in " << search_time << "ms\n";
    
    auto total_time = duration_cast<milliseconds>(high_resolution_clock::now() - start).count();
    std::cout << "✅ Phase separation test completed in " << total_time << "ms\n";
}

// Test 5: Memory fence effectiveness
void test_memory_fence() {
    std::cout << "\n🔒 Test 5: Memory fence effectiveness\n";
    
    VectorStore store(32);
    std::atomic<bool> reader_saw_incomplete{false};
    std::atomic<bool> stop_readers{false};
    std::atomic<size_t> successful_reads{0};
    
    // Multiple reader threads
    std::vector<std::thread> readers;
    for (size_t t = 0; t < 4; ++t) {
        readers.emplace_back([&]() {
            size_t reads = 0;
            while (!stop_readers) {
                size_t n = store.size();
                for (size_t i = 0; i < n; ++i) {
                    const auto& entry = store.get_entry(i);
                    // Check if entry has embedding (our "ready" flag)
                    if (entry.embedding) {
                        // Entry should be fully constructed
                        if (entry.doc.id.empty() || entry.doc.text.empty()) {
                            // Partial entry detected!
                            reader_saw_incomplete = true;
                        } else {
                            reads++;
                        }
                    }
                    // If no embedding, entry is not ready yet - skip it
                }
                std::this_thread::yield();
            }
            successful_reads += reads;
        });
    }
    
    // Writer thread
    std::thread writer([&]() {
        std::mt19937 rng(42);
        simdjson::ondemand::parser parser;
        
        for (size_t i = 0; i < 1000; ++i) {
            auto embedding = generate_random_embedding(32, rng);
            std::string json_str = create_json_document(
                "fence-" + std::to_string(i),
                "Testing memory fence " + std::to_string(i),
                embedding
            );
            
            simdjson::padded_string padded(json_str);
            simdjson::ondemand::document doc;
            if (!parser.iterate(padded).get(doc)) {
                store.add_document(doc);
            }
        }
    });
    
    writer.join();
    std::this_thread::sleep_for(milliseconds(100));
    stop_readers = true;
    
    for (auto& t : readers) {
        t.join();
    }
    
    if (reader_saw_incomplete) {
        std::cout << "❌ Reader saw incomplete entry (memory fence issue)\n";
        std::exit(1);
    } else {
        std::cout << "✅ Memory fence working correctly\n";
        std::cout << "   Successful reads: " << successful_reads.load() << "\n";
    }
}

int main() {
    std::cout << "🔥 Starting concurrent stress tests...\n";
    
    // Detect which sanitizer is enabled
    #if defined(__has_feature)
        #if __has_feature(address_sanitizer)
            std::cout << "   Running with AddressSanitizer (ASAN)\n";
        #elif __has_feature(thread_sanitizer)
            std::cout << "   Running with ThreadSanitizer (TSAN)\n";
        #endif
    #elif defined(__SANITIZE_ADDRESS__)
        std::cout << "   Running with AddressSanitizer (ASAN)\n";
    #elif defined(__SANITIZE_THREAD__)
        std::cout << "   Running with ThreadSanitizer (TSAN)\n";
    #else
        std::cout << "   ⚠️  Running without sanitizers - use 'make stress' for ASAN by default\n";
        std::cout << "   Or use: make stress SANITIZER=thread for TSAN\n";
        std::cout << "           make stress SANITIZER=none to disable\n";
    #endif
    
    test_concurrent_inserts();
    test_oversize_allocation();
    test_alignment_requests();
    test_phase_separation();
    test_memory_fence();
    
    std::cout << "\n✅ All stress tests passed!\n";
    return 0;
}