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
constexpr size_t DOCS_PER_THREAD = 125000;
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

// Test 1: 1M parallel inserts with 8 threads
void test_concurrent_inserts() {
    std::cout << "\n📝 Test 1: 1M parallel inserts with " << NUM_THREADS << " threads\n";
    
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
    
    try {
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
            store.add_document(doc);
        }
        
        std::cout << "❌ Should have thrown error for oversize allocation\n";
        std::exit(1);
    } catch (const std::bad_alloc& e) {
        std::cout << "✅ Correctly rejected oversize allocation: " << e.what() << "\n";
    } catch (const std::exception& e) {
        std::cout << "✅ Correctly rejected oversize allocation: " << e.what() << "\n";
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
                try {
                    void* ptr = allocate(128, align);
                    assert(ptr != nullptr);
                    assert(((uintptr_t)ptr % align) == 0);
                } catch (const std::exception& e) {
                    std::cout << "❌ Failed with alignment " << align << ": " << e.what() << "\n";
                    std::exit(1);
                }
            }
            std::cout << "✅ All valid alignments handled correctly\n";
            
            // Test invalid alignment (>4096)
            try {
                allocate(128, 8192);
                std::cout << "❌ Should have rejected alignment > 4096\n";
                std::exit(1);
            } catch (const std::invalid_argument& e) {
                std::cout << "✅ Correctly rejected large alignment: " << e.what() << "\n";
            }
        }
    };
    
    TestArenaAllocator allocator;
    allocator.test_alignments();
}

// Test 4: Concurrent normalize_all + search while loading
void test_concurrent_operations() {
    std::cout << "\n🔄 Test 4: Concurrent normalize_all + search while loading\n";
    
    VectorStore store(DIM);
    std::atomic<bool> stop_loading{false};
    std::atomic<bool> stop_normalizing{false};
    std::atomic<bool> stop_searching{false};
    std::atomic<size_t> docs_loaded{0};
    std::atomic<size_t> normalizations{0};
    std::atomic<size_t> searches{0};
    
    // Pre-load some documents
    std::mt19937 rng(42);
    simdjson::ondemand::parser parser;
    
    for (size_t i = 0; i < 10000; ++i) {
        auto embedding = generate_random_embedding(DIM, rng);
        std::string json_str = create_json_document(
            "init-" + std::to_string(i),
            "Initial document " + std::to_string(i),
            embedding
        );
        
        simdjson::padded_string padded(json_str);
        simdjson::ondemand::document doc;
        if (!parser.iterate(padded).get(doc)) {
            store.add_document(doc);
        }
    }
    
    auto start = high_resolution_clock::now();
    
    // Thread 1: Continuous loading
    std::thread loader([&]() {
        std::mt19937 rng(1);
        simdjson::ondemand::parser parser;
        size_t count = 0;
        
        while (!stop_loading && count < 50000) {
            for (size_t i = 0; i < 100 && count < 50000; ++i) {
                auto embedding = generate_random_embedding(DIM, rng);
                std::string json_str = create_json_document(
                    "load-" + std::to_string(count),
                    "Loading document " + std::to_string(count),
                    embedding
                );
                
                simdjson::padded_string padded(json_str);
                simdjson::ondemand::document doc;
                if (!parser.iterate(padded).get(doc)) {
                    auto error = store.add_document(doc);
                    if (!error) {
                        count++;
                    }
                }
            }
            std::this_thread::sleep_for(milliseconds(10));
        }
        docs_loaded = count;
        stop_loading = true;
    });
    
    // Thread 2: Continuous normalization
    std::thread normalizer([&]() {
        size_t iterations = 0;
        while (!stop_normalizing && iterations < 100) {
            store.normalize_all();
            iterations++;
            std::this_thread::sleep_for(milliseconds(50));
        }
        normalizations = iterations;
        stop_normalizing = true;
    });
    
    // Thread 3: Continuous searching
    std::thread searcher([&]() {
        std::mt19937 rng(3);
        size_t iterations = 0;
        
        while (!stop_searching && iterations < SEARCH_ITERATIONS) {
            auto query = generate_random_embedding(DIM, rng);
            auto results = store.search(query.data(), 10);
            assert(results.size() <= 10);
            iterations++;
            std::this_thread::sleep_for(milliseconds(30));
        }
        searches = iterations;
        stop_searching = true;
    });
    
    // Wait for all operations to complete
    loader.join();
    normalizer.join();
    searcher.join();
    
    auto end = high_resolution_clock::now();
    auto elapsed = duration_cast<milliseconds>(end - start).count();
    
    std::cout << "✅ Concurrent operations completed in " << elapsed << "ms\n";
    std::cout << "   Documents loaded: " << docs_loaded.load() << "\n";
    std::cout << "   Normalizations: " << normalizations.load() << "\n";
    std::cout << "   Searches: " << searches.load() << "\n";
    std::cout << "   Final store size: " << store.size() << "\n";
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
                    try {
                        const auto& entry = store.get_entry(i);
                        // Verify entry is complete
                        if (entry.embedding && !entry.doc.id.empty()) {
                            reads++;
                        } else if (entry.embedding || !entry.doc.id.empty()) {
                            // Partial entry detected!
                            reader_saw_incomplete = true;
                        }
                    } catch (...) {
                        // Entry might not exist yet
                    }
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
        
        for (size_t i = 0; i < 10000; ++i) {
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
    std::cout << "   Build with: -fsanitize=address,thread -g\n";
    
    try {
        test_concurrent_inserts();
        test_oversize_allocation();
        test_alignment_requests();
        test_concurrent_operations();
        test_memory_fence();
        
        std::cout << "\n✅ All stress tests passed!\n";
        return 0;
    } catch (const std::exception& e) {
        std::cout << "\n❌ Test failed: " << e.what() << "\n";
        return 1;
    }
}