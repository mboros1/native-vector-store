#include "vector_store.h"
#include "simple_tokenizer.h"
#include <iostream>
#include <vector>
#include <string>
#include <cassert>
#include <iomanip>
#include <random>
#include <chrono>
#include <set>

// Helper function to create test documents with embeddings
simdjson::error_code create_test_document(const std::string& id, const std::string& text, 
                                        const std::vector<float>& embedding, 
                                        std::string& json_out) {
    json_out = R"({"id":")" + id + R"(","text":")" + text + R"(","metadata":{"embedding":[)";
    
    for (size_t i = 0; i < embedding.size(); ++i) {
        if (i > 0) json_out += ",";
        json_out += std::to_string(embedding[i]);
    }
    json_out += R"(]}})";
    return simdjson::SUCCESS;
}

// Generate random embedding with some semantic structure
std::vector<float> generate_embedding(size_t dim, int category, float noise_level = 0.1f) {
    std::vector<float> embedding(dim);
    std::mt19937 gen(42 + category);
    std::normal_distribution<float> noise(0.0f, noise_level);
    
    // Create category-specific patterns
    for (size_t i = 0; i < dim; ++i) {
        float base_value = 0.0f;
        
        // Different categories have different patterns
        switch (category) {
            case 0: // Machine learning category
                base_value = std::sin(i * 0.5f) * 0.5f;
                break;
            case 1: // Natural language category
                base_value = std::cos(i * 0.3f) * 0.4f;
                break;
            case 2: // Computer vision category
                base_value = std::sin(i * 0.2f) * std::cos(i * 0.1f);
                break;
            case 3: // Data science category
                base_value = std::tanh(i * 0.1f - 2.0f);
                break;
            default: // General/other
                base_value = 0.1f;
        }
        
        embedding[i] = base_value + noise(gen);
    }
    
    // Normalize
    float sum = 0.0f;
    for (float val : embedding) sum += val * val;
    float inv_norm = 1.0f / std::sqrt(sum + 1e-10f);
    for (float& val : embedding) val *= inv_norm;
    
    return embedding;
}

void test_hybrid_search_basic() {
    std::cout << "\n=== Testing Basic Hybrid Search ===" << std::endl;
    
    const size_t dim = 128;  // Realistic embedding dimension
    VectorStore store(dim);
    
    // Create test corpus with semantic categories
    struct TestDoc {
        std::string id;
        std::string text;
        int category;
    };
    
    std::vector<TestDoc> docs = {
        // Machine learning cluster (category 0)
        {"ml1", "Machine learning algorithms use gradient descent for optimization", 0},
        {"ml2", "Deep learning neural networks require backpropagation training", 0},
        {"ml3", "Supervised learning uses labeled data for model training", 0},
        {"ml4", "Reinforcement learning agents maximize reward through exploration", 0},
        
        // NLP cluster (category 1)
        {"nlp1", "Natural language processing enables computers to understand text", 1},
        {"nlp2", "Transformer models revolutionized language understanding tasks", 1},
        {"nlp3", "Text embeddings capture semantic meaning in vector space", 1},
        {"nlp4", "Named entity recognition identifies people places and organizations", 1},
        
        // Computer vision cluster (category 2)
        {"cv1", "Computer vision algorithms detect objects in images", 2},
        {"cv2", "Convolutional neural networks excel at image classification", 2},
        {"cv3", "Image segmentation divides pictures into meaningful regions", 2},
        {"cv4", "Face recognition systems use biometric features for identification", 2},
        
        // Data science cluster (category 3)
        {"ds1", "Data science combines statistics and programming for insights", 3},
        {"ds2", "Feature engineering improves machine learning model performance", 3},
        {"ds3", "Data visualization helps communicate complex patterns effectively", 3},
        {"ds4", "Statistical analysis reveals trends and correlations in datasets", 3},
        
        // Mixed/ambiguous documents
        {"mix1", "Neural networks process images and text using deep learning", 0},
        {"mix2", "Computer algorithms analyze visual data for pattern recognition", 2},
        {"mix3", "Machine learning transforms data science and analytics workflows", 3},
        {"mix4", "Language models and vision systems share transformer architectures", 1},
    };
    
    // Add documents to store
    std::cout << "Adding " << docs.size() << " documents to store..." << std::endl;
    
    simdjson::ondemand::parser parser;
    auto alloc_error = parser.allocate(1024 * 1024);
    if (alloc_error) {
        std::cerr << "Parser allocation failed: " << simdjson::error_message(alloc_error) << std::endl;
        return;
    }
    
    for (const auto& doc : docs) {
        auto embedding = generate_embedding(dim, doc.category);
        std::string json_str;
        create_test_document(doc.id, doc.text, embedding, json_str);
        
        simdjson::padded_string padded(json_str);
        simdjson::ondemand::document json_doc;
        auto error = parser.iterate(padded).get(json_doc);
        if (!error) {
            store.add_document(json_doc);
        }
    }
    
    store.finalize();
    std::cout << "Store finalized with " << store.size() << " documents" << std::endl;
    std::cout << "Average document length: " << std::fixed << std::setprecision(2) 
              << store.avg_doc_length() << " tokens\n" << std::endl;
}

void test_hybrid_search_queries() {
    std::cout << "\n=== Testing Hybrid Search Queries ===" << std::endl;
    
    const size_t dim = 128;
    VectorStore store(dim);
    
    // Build corpus (same as above but condensed)
    std::vector<std::pair<std::string, std::string>> corpus = {
        {"ml1", "Machine learning algorithms use gradient descent optimization"},
        {"ml2", "Deep neural networks require backpropagation for training"},
        {"nlp1", "Natural language processing transforms text into vectors"},
        {"nlp2", "Transformer models excel at language understanding tasks"},
        {"cv1", "Computer vision algorithms detect and classify objects"},
        {"cv2", "Convolutional networks process images through multiple layers"},
        {"ds1", "Data science combines statistics with machine learning"},
        {"ds2", "Statistical analysis reveals patterns in large datasets"},
    };
    
    simdjson::ondemand::parser parser;
    auto alloc_error = parser.allocate(1024 * 1024);
    if (alloc_error) {
        std::cerr << "Parser allocation failed: " << simdjson::error_message(alloc_error) << std::endl;
        return;
    }
    
    // Add documents with category-based embeddings
    for (size_t i = 0; i < corpus.size(); ++i) {
        int category = i / 2;  // Group pairs into categories
        auto embedding = generate_embedding(dim, category);
        std::string json_str;
        create_test_document(corpus[i].first, corpus[i].second, embedding, json_str);
        
        simdjson::padded_string padded(json_str);
        simdjson::ondemand::document json_doc;
        if (!parser.iterate(padded).get(json_doc)) {
            store.add_document(json_doc);
        }
    }
    
    store.finalize();
    
    // Test different query scenarios
    struct QueryTest {
        std::string description;
        std::vector<std::string> query_terms;
        int semantic_category;  // For generating query vector
        std::vector<std::string> expected_top_docs;  // Expected relevant docs
    };
    
    std::vector<QueryTest> test_queries = {
        {
            "Exact keyword match (neural networks)",
            {"neural", "networks"},
            0,  // ML category
            {"ml2", "cv2"}  // Both mention neural networks
        },
        {
            "Semantic similarity (AI learning)",
            {"artificial", "intelligence", "learning"},
            0,  // ML category
            {"ml1", "ml2", "ds1"}  // Semantically related to ML
        },
        {
            "Mixed query (vision + algorithms)",
            {"vision", "algorithms"},
            2,  // CV category
            {"cv1", "cv2"}  // Computer vision documents
        },
        {
            "Broad query (data analysis)",
            {"data", "analysis"},
            3,  // DS category
            {"ds1", "ds2"}  // Data science documents
        }
    };
    
    std::cout << "Testing query scenarios:\n" << std::endl;
    
    for (const auto& test : test_queries) {
        std::cout << "Query: " << test.description << std::endl;
        std::cout << "Terms: ";
        for (const auto& term : test.query_terms) {
            std::cout << "\"" << term << "\" ";
        }
        std::cout << std::endl;
        
        // Generate query vector based on semantic category
        auto query_vector = generate_embedding(dim, test.semantic_category, 0.05f);
        
        // Test different search modes
        std::cout << "\n1. Vector-only search:" << std::endl;
        auto vector_results = store.search(query_vector.data(), 5);
        for (size_t i = 0; i < std::min(size_t(3), vector_results.size()); ++i) {
            const auto& entry = store.get_entry(vector_results[i].second);
            std::cout << "   " << entry.doc.id << " (score: " 
                      << std::fixed << std::setprecision(4) << vector_results[i].first << ")" << std::endl;
        }
        
        std::cout << "\n2. BM25-only search:" << std::endl;
        auto bm25_results = store.search_bm25(test.query_terms);
        for (size_t i = 0; i < std::min(size_t(3), bm25_results.size()); ++i) {
            const auto& entry = store.get_entry(bm25_results[i].first);
            std::cout << "   " << entry.doc.id << " (score: " 
                      << std::fixed << std::setprecision(4) << bm25_results[i].second << ")" << std::endl;
        }
        
        std::cout << "\n3. Hybrid search (0.5/0.5):" << std::endl;
        auto hybrid_results = store.search_hybrid(query_vector.data(), test.query_terms, 0.5, 0.5, 5);
        for (size_t i = 0; i < std::min(size_t(3), hybrid_results.size()); ++i) {
            const auto& entry = store.get_entry(hybrid_results[i].first);
            std::cout << "   " << entry.doc.id << " (score: " 
                      << std::fixed << std::setprecision(4) << hybrid_results[i].second << ")" << std::endl;
        }
        
        // Check if expected documents appear in top results
        std::set<std::string> top_hybrid_docs;
        for (size_t i = 0; i < std::min(size_t(3), hybrid_results.size()); ++i) {
            const auto& entry = store.get_entry(hybrid_results[i].first);
            top_hybrid_docs.insert(std::string(entry.doc.id));
        }
        
        std::cout << "\nExpected docs found: ";
        for (const auto& expected : test.expected_top_docs) {
            if (top_hybrid_docs.count(expected)) {
                std::cout << expected << " ✓ ";
            } else {
                std::cout << expected << " ✗ ";
            }
        }
        std::cout << "\n" << std::endl;
        std::cout << "---" << std::endl;
    }
}

void test_hybrid_search_weights() {
    std::cout << "\n=== Testing Hybrid Search Weight Sensitivity ===" << std::endl;
    
    const size_t dim = 64;
    VectorStore store(dim);
    
    // Create documents with varying keyword/semantic overlap
    std::vector<std::tuple<std::string, std::string, int>> docs = {
        {"exact1", "machine learning algorithms optimize neural network weights", 0},
        {"exact2", "neural network training requires gradient optimization", 0},
        {"similar1", "deep learning models use backpropagation", 0},
        {"similar2", "artificial intelligence systems learn from data", 0},
        {"different1", "database query processing improves performance", 2},
        {"different2", "web server handles HTTP requests efficiently", 2},
    };
    
    simdjson::ondemand::parser parser;
    auto alloc_error = parser.allocate(1024 * 1024);
    if (alloc_error) {
        std::cerr << "Parser allocation failed: " << simdjson::error_message(alloc_error) << std::endl;
        return;
    }
    
    for (const auto& [id, text, category] : docs) {
        auto embedding = generate_embedding(dim, category);
        std::string json_str;
        create_test_document(id, text, embedding, json_str);
        
        simdjson::padded_string padded(json_str);
        simdjson::ondemand::document json_doc;
        if (!parser.iterate(padded).get(json_doc)) {
            store.add_document(json_doc);
        }
    }
    
    store.finalize();
    
    // Query that has both exact matches and semantic similarity
    std::vector<std::string> query_terms = {"neural", "network", "optimization"};
    auto query_vector = generate_embedding(dim, 0, 0.05f);  // Category 0 with low noise
    
    std::cout << "Query terms: ";
    for (const auto& term : query_terms) std::cout << "\"" << term << "\" ";
    std::cout << "\n" << std::endl;
    
    // Test different weight combinations
    std::vector<std::pair<double, double>> weight_configs = {
        {1.0, 0.0},  // Pure vector
        {0.8, 0.2},  // Vector-heavy
        {0.6, 0.4},  // Slightly vector-heavy
        {0.5, 0.5},  // Balanced
        {0.4, 0.6},  // Slightly BM25-heavy
        {0.2, 0.8},  // BM25-heavy
        {0.0, 1.0},  // Pure BM25
    };
    
    std::cout << "Weight sensitivity analysis:" << std::endl;
    std::cout << std::setw(15) << "Vector Weight" << std::setw(15) << "BM25 Weight" 
              << std::setw(10) << "Top Doc" << std::setw(12) << "Score" << std::endl;
    std::cout << std::string(52, '-') << std::endl;
    
    for (const auto& [vector_weight, bm25_weight] : weight_configs) {
        auto results = store.search_hybrid(query_vector.data(), query_terms, 
                                          vector_weight, bm25_weight, 3);
        
        if (!results.empty()) {
            const auto& entry = store.get_entry(results[0].first);
            std::cout << std::setw(15) << std::fixed << std::setprecision(1) << vector_weight
                      << std::setw(15) << bm25_weight
                      << std::setw(10) << entry.doc.id
                      << std::setw(12) << std::setprecision(4) << results[0].second 
                      << std::endl;
        }
    }
}

void test_hybrid_search_performance() {
    std::cout << "\n=== Testing Hybrid Search Performance ===" << std::endl;
    
    const size_t dim = 768;  // BERT-like dimension
    const size_t num_docs = 10000;
    const size_t num_queries = 100;
    
    VectorStore store(dim);
    
    // Generate a larger corpus
    std::cout << "Building corpus of " << num_docs << " documents..." << std::endl;
    
    simdjson::ondemand::parser parser;
    auto alloc_error = parser.allocate(1024 * 1024);
    if (alloc_error) {
        std::cerr << "Parser allocation failed: " << simdjson::error_message(alloc_error) << std::endl;
        return;
    }
    
    std::mt19937 gen(42);
    std::uniform_int_distribution<> cat_dist(0, 9);  // 10 categories
    
    std::vector<std::string> words = {
        "machine", "learning", "neural", "network", "deep", "data", "algorithm",
        "model", "training", "optimization", "gradient", "vector", "embedding",
        "classification", "regression", "clustering", "analysis", "processing",
        "computer", "vision", "language", "natural", "artificial", "intelligence"
    };
    
    std::uniform_int_distribution<> word_dist(0, words.size() - 1);
    std::uniform_int_distribution<> text_len_dist(5, 15);
    
    auto start = std::chrono::high_resolution_clock::now();
    
    for (size_t i = 0; i < num_docs; ++i) {
        // Generate random text
        std::string text;
        int text_len = text_len_dist(gen);
        for (int j = 0; j < text_len; ++j) {
            if (j > 0) text += " ";
            text += words[word_dist(gen)];
        }
        
        // Generate embedding based on category
        int category = cat_dist(gen);
        auto embedding = generate_embedding(dim, category, 0.2f);
        
        std::string json_str;
        create_test_document("doc" + std::to_string(i), text, embedding, json_str);
        
        simdjson::padded_string padded(json_str);
        simdjson::ondemand::document json_doc;
        if (!parser.iterate(padded).get(json_doc)) {
            store.add_document(json_doc);
        }
    }
    
    store.finalize();
    
    auto build_time = std::chrono::high_resolution_clock::now() - start;
    std::cout << "Corpus built in " 
              << std::chrono::duration_cast<std::chrono::milliseconds>(build_time).count() 
              << "ms" << std::endl;
    std::cout << "Average document length: " << store.avg_doc_length() << " tokens\n" << std::endl;
    
    // Benchmark different search types
    std::cout << "Benchmarking " << num_queries << " queries:" << std::endl;
    
    // Generate random queries
    std::vector<std::pair<std::vector<float>, std::vector<std::string>>> queries;
    for (size_t i = 0; i < num_queries; ++i) {
        // Random query vector
        auto query_vector = generate_embedding(dim, cat_dist(gen), 0.1f);
        
        // Random query terms (2-4 terms)
        std::uniform_int_distribution<> num_terms_dist(2, 4);
        int num_terms = num_terms_dist(gen);
        std::vector<std::string> query_terms;
        for (int j = 0; j < num_terms; ++j) {
            query_terms.push_back(words[word_dist(gen)]);
        }
        
        queries.push_back({query_vector, query_terms});
    }
    
    // Benchmark vector search
    start = std::chrono::high_resolution_clock::now();
    for (const auto& [query_vector, _] : queries) {
        auto results = store.search(query_vector.data(), 10);
    }
    auto vector_time = std::chrono::high_resolution_clock::now() - start;
    
    // Benchmark BM25 search
    start = std::chrono::high_resolution_clock::now();
    for (const auto& [_, query_terms] : queries) {
        auto results = store.search_bm25(query_terms);
    }
    auto bm25_time = std::chrono::high_resolution_clock::now() - start;
    
    // Benchmark hybrid search
    start = std::chrono::high_resolution_clock::now();
    for (const auto& [query_vector, query_terms] : queries) {
        auto results = store.search_hybrid(query_vector.data(), query_terms, 0.5, 0.5, 10);
    }
    auto hybrid_time = std::chrono::high_resolution_clock::now() - start;
    
    std::cout << "\nPerformance Results:" << std::endl;
    std::cout << "Vector search: " 
              << std::chrono::duration_cast<std::chrono::microseconds>(vector_time).count() / num_queries
              << " µs/query" << std::endl;
    std::cout << "BM25 search: " 
              << std::chrono::duration_cast<std::chrono::microseconds>(bm25_time).count() / num_queries
              << " µs/query" << std::endl;
    std::cout << "Hybrid search: " 
              << std::chrono::duration_cast<std::chrono::microseconds>(hybrid_time).count() / num_queries
              << " µs/query" << std::endl;
    
    double overhead = (std::chrono::duration_cast<std::chrono::microseconds>(hybrid_time).count() - 
                      std::chrono::duration_cast<std::chrono::microseconds>(vector_time).count()) * 100.0 / 
                      std::chrono::duration_cast<std::chrono::microseconds>(vector_time).count();
    
    std::cout << "Hybrid overhead vs vector-only: " 
              << std::fixed << std::setprecision(1) << overhead << "%" << std::endl;
}

int main() {
    std::cout << "🔍 Hybrid Search Test Suite" << std::endl;
    std::cout << "============================" << std::endl;
    
    try {
        test_hybrid_search_basic();
        test_hybrid_search_queries();
        test_hybrid_search_weights();
        test_hybrid_search_performance();
        
        std::cout << "\n✅ All hybrid search tests completed successfully!" << std::endl;
        
    } catch (const std::exception& e) {
        std::cerr << "❌ Test failed with exception: " << e.what() << std::endl;
        return 1;
    } catch (...) {
        std::cerr << "❌ Test failed with unknown exception" << std::endl;
        return 1;
    }
    
    return 0;
}