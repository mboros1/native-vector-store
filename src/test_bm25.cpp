#include "vector_store.h"
#include "simple_tokenizer.h"
#include "english_punctuations.h"
#include <iostream>
#include <vector>
#include <string>
#include <cassert>
#include <iomanip>

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

void test_simple_tokenizer() {
    std::cout << "\n=== Testing SimpleTokenizer ===" << std::endl;
    
    // Test cases with various text types
    std::vector<std::pair<std::string, std::string>> test_cases = {
        {"Simple sentence", "Hello world, how are you?"},
        {"Contractions", "I can't believe it's working! Won't you try it?"},
        {"Technical text", "Machine learning algorithms use neural networks for deep learning."},
        {"Punctuation heavy", "Hello... world! (Are you there?) Yes, I am here."},
        {"Mixed case", "The CEO of AI-Corp said, 'This isn't just a prototype.'"},
        {"Numbers and symbols", "Version 2.0 includes 50% faster processing & better UX."},
        {"Abbreviations", "Dr. Smith works at U.S.A. Inc. etc."},
        {"Empty and spaces", "   \n\t   "},
        {"Special chars", "email@domain.com and https://website.com/path"},
    };
    
    // Test without contraction splitting
    std::cout << "\n--- Without contraction splitting ---" << std::endl;
    SimpleTokenizer tokenizer_basic(false);
    
    for (const auto& test : test_cases) {
        auto tokens = tokenizer_basic.split(test.second);
        std::cout << "Input: \"" << test.second << "\"" << std::endl;
        std::cout << "Tokens (" << tokens.size() << "): ";
        for (size_t i = 0; i < tokens.size(); ++i) {
            if (i > 0) std::cout << ", ";
            std::cout << "\"" << tokens[i] << "\"";
        }
        std::cout << "\n" << std::endl;
    }
    
    // Test with contraction splitting
    std::cout << "\n--- With contraction splitting ---" << std::endl;
    SimpleTokenizer tokenizer_contractions(true);
    
    std::vector<std::string> contraction_tests = {
        "I can't believe it won't work.",
        "She ain't coming and we shan't wait.",
        "They're gonna wanna see what's happening.",
        "I'd've done it if I could've.",
    };
    
    for (const auto& text : contraction_tests) {
        auto tokens = tokenizer_contractions.split(text);
        std::cout << "Input: \"" << text << "\"" << std::endl;
        std::cout << "Tokens: ";
        for (size_t i = 0; i < tokens.size(); ++i) {
            if (i > 0) std::cout << ", ";
            std::cout << "\"" << tokens[i] << "\"";
        }
        std::cout << "\n" << std::endl;
    }
}

void test_english_punctuations() {
    std::cout << "\n=== Testing EnglishPunctuations ===" << std::endl;
    
    auto& puncts = EnglishPunctuations::getInstance();
    
    std::cout << "Total punctuation marks: " << puncts.size() << std::endl;
    
    // Test common punctuation
    std::vector<std::string> test_marks = {
        ",", ".", "!", "?", ";", ":", "(", ")", "[", "]", "{", "}", 
        "\"", "'", "`", "/", "-", "--", "---", "...", "<", ">"
    };
    
    std::cout << "Testing punctuation detection:" << std::endl;
    for (const auto& mark : test_marks) {
        bool is_punct = puncts.contains(mark);
        std::cout << "\"" << mark << "\": " << (is_punct ? "✓" : "✗") << std::endl;
    }
    
    // Test non-punctuation
    std::vector<std::string> non_punct = {"hello", "123", "abc", "A", "@", "#", "$", "%"};
    std::cout << "\nTesting non-punctuation:" << std::endl;
    for (const auto& mark : non_punct) {
        bool is_punct = puncts.contains(mark);
        std::cout << "\"" << mark << "\": " << (is_punct ? "✗" : "✓") << std::endl;
    }
    
    // Test iteration
    std::cout << "\nAll punctuation marks: ";
    size_t count = 0;
    for (const auto& mark : puncts) {
        if (count++ > 0) std::cout << ", ";
        std::cout << "\"" << mark << "\"";
    }
    std::cout << std::endl;
}

void test_bm25_search() {
    std::cout << "\n=== Testing BM25 Search ===" << std::endl;
    
    // Create a VectorStore with 5-dimensional embeddings
    const size_t dim = 5;
    VectorStore store(dim);
    
    // Test documents with different content types
    struct TestDoc {
        std::string id;
        std::string text;
        std::vector<float> embedding;
    };
    
    std::vector<TestDoc> docs = {
        {"doc1", "Machine learning algorithms are powerful tools for data analysis.", 
         {0.1f, 0.2f, 0.3f, 0.4f, 0.5f}},
        {"doc2", "Deep learning uses neural networks to process complex data patterns.", 
         {0.2f, 0.3f, 0.4f, 0.5f, 0.6f}},
        {"doc3", "Data science combines statistics, machine learning, and domain expertise.", 
         {0.3f, 0.4f, 0.5f, 0.6f, 0.7f}},
        {"doc4", "Natural language processing enables computers to understand human language.", 
         {0.4f, 0.5f, 0.6f, 0.7f, 0.8f}},
        {"doc5", "Computer vision algorithms can analyze and interpret visual information.", 
         {0.5f, 0.6f, 0.7f, 0.8f, 0.9f}},
        {"doc6", "The quick brown fox jumps over the lazy dog repeatedly.", 
         {0.1f, 0.3f, 0.5f, 0.7f, 0.9f}},
        {"doc7", "Artificial intelligence revolutionizes how we process and analyze data.", 
         {0.2f, 0.4f, 0.6f, 0.8f, 1.0f}},
    };
    
    // Add documents to store
    std::cout << "Adding " << docs.size() << " documents to store..." << std::endl;
    
    simdjson::ondemand::parser parser;
    parser.allocate(1024 * 1024);  // 1MB capacity
    
    for (const auto& doc : docs) {
        std::string json_str;
        create_test_document(doc.id, doc.text, doc.embedding, json_str);
        
        simdjson::padded_string padded(json_str);
        simdjson::ondemand::document json_doc;
        auto error = parser.iterate(padded).get(json_doc);
        if (error) {
            std::cerr << "JSON parse error: " << simdjson::error_message(error) << std::endl;
            continue;
        }
        
        auto add_error = store.add_document(json_doc);
        if (add_error != VectorStoreError::SUCCESS) {
            std::cerr << "Add document error: " << vector_store_error_message(add_error) << std::endl;
            continue;
        }
        
        std::cout << "Added: " << doc.id << std::endl;
    }
    
    // Finalize store
    std::cout << "\nFinalizing store..." << std::endl;
    store.finalize();
    
    std::cout << "Store size: " << store.size() << std::endl;
    std::cout << "Average document length: " << std::fixed << std::setprecision(2) 
              << store.avg_doc_length() << " tokens" << std::endl;
    
    // Test BM25 searches with different query types
    std::vector<std::vector<std::string>> queries = {
        {"machine", "learning"},
        {"data", "analysis"},
        {"neural", "networks"},
        {"computer", "vision"},
        {"artificial", "intelligence"},
        {"quick", "brown", "fox"},
        {"process", "analyze"},
        {"algorithms"},
    };
    
    std::cout << "\n--- BM25 Search Results ---" << std::endl;
    
    for (const auto& query : queries) {
        std::cout << "\nQuery: ";
        for (size_t i = 0; i < query.size(); ++i) {
            if (i > 0) std::cout << " ";
            std::cout << "\"" << query[i] << "\"";
        }
        std::cout << std::endl;
        
        auto results = store.search_bm25(query);
        std::cout << "Results (" << results.size() << "):" << std::endl;
        
        for (size_t i = 0; i < std::min(size_t(5), results.size()); ++i) {
            const auto& entry = store.get_entry(results[i].first);
            std::cout << "  " << (i+1) << ". " << entry.doc.id 
                      << " (score: " << std::fixed << std::setprecision(4) 
                      << results[i].second << ")" << std::endl;
            std::cout << "     \"" << entry.doc.text << "\"" << std::endl;
        }
    }
    
    // Test hybrid search
    std::cout << "\n--- Hybrid Search Results ---" << std::endl;
    
    // Test query vector (similar to doc3)
    float query_vector[dim] = {0.35f, 0.45f, 0.55f, 0.65f, 0.75f};
    std::vector<std::string> query_terms = {"machine", "learning", "data"};
    
    std::cout << "Query vector: [";
    for (size_t i = 0; i < dim; ++i) {
        if (i > 0) std::cout << ", ";
        std::cout << std::fixed << std::setprecision(2) << query_vector[i];
    }
    std::cout << "]" << std::endl;
    
    std::cout << "Query terms: ";
    for (size_t i = 0; i < query_terms.size(); ++i) {
        if (i > 0) std::cout << " ";
        std::cout << "\"" << query_terms[i] << "\"";
    }
    std::cout << std::endl;
    
    // Test different weighting schemes
    std::vector<std::pair<double, double>> weight_schemes = {
        {0.7, 0.3},  // Vector-heavy
        {0.5, 0.5},  // Balanced
        {0.3, 0.7},  // BM25-heavy
    };
    
    for (const auto& weights : weight_schemes) {
        std::cout << "\nHybrid search (vector: " << weights.first 
                  << ", BM25: " << weights.second << "):" << std::endl;
        
        auto hybrid_results = store.search_hybrid(query_vector, query_terms, 
                                                weights.first, weights.second, 5);
        
        for (size_t i = 0; i < hybrid_results.size(); ++i) {
            const auto& entry = store.get_entry(hybrid_results[i].first);
            std::cout << "  " << (i+1) << ". " << entry.doc.id 
                      << " (score: " << std::fixed << std::setprecision(4) 
                      << hybrid_results[i].second << ")" << std::endl;
        }
    }
    
    // Test BM25 parameter tuning
    std::cout << "\n--- BM25 Parameter Tuning ---" << std::endl;
    
    std::vector<std::tuple<double, double, double>> bm25_params = {
        {1.2, 0.75, 1.0},  // Default
        {2.0, 0.9, 0.5},   // High k1, high b
        {0.8, 0.5, 1.5},   // Low k1, low b
    };
    
    std::vector<std::string> test_query = {"machine", "learning"};
    
    for (const auto& params : bm25_params) {
        store.set_bm25_parameters(std::get<0>(params), std::get<1>(params), std::get<2>(params));
        
        std::cout << "BM25 params (k1=" << std::get<0>(params) 
                  << ", b=" << std::get<1>(params) 
                  << ", delta=" << std::get<2>(params) << "):" << std::endl;
        
        auto results = store.search_bm25(test_query);
        for (size_t i = 0; i < std::min(size_t(3), results.size()); ++i) {
            const auto& entry = store.get_entry(results[i].first);
            std::cout << "  " << entry.doc.id 
                      << " (score: " << std::fixed << std::setprecision(4) 
                      << results[i].second << ")" << std::endl;
        }
    }
}

void test_document_analysis() {
    std::cout << "\n=== Testing Document Analysis ===" << std::endl;
    
    VectorStore store(3);  // Small embedding for testing
    
    // Test document with detailed analysis
    std::string test_text = "Machine learning can't solve every problem, but it's incredibly powerful! "
                           "Dr. Smith's research shows 95% accuracy in NLP tasks.";
    
    SimpleTokenizer tokenizer(true);  // Enable contraction splitting
    auto tokens = tokenizer.split(test_text);
    
    std::cout << "Original text: \"" << test_text << "\"" << std::endl;
    std::cout << "Tokens (" << tokens.size() << "): ";
    for (size_t i = 0; i < tokens.size(); ++i) {
        if (i > 0) std::cout << ", ";
        std::cout << "\"" << tokens[i] << "\"";
    }
    std::cout << "\n" << std::endl;
    
    // Add document to store to see BM25 processing
    std::string json_str;
    std::vector<float> embedding = {0.1f, 0.2f, 0.3f};
    create_test_document("test_doc", test_text, embedding, json_str);
    
    simdjson::ondemand::parser parser;
    parser.allocate(1024 * 1024);
    
    simdjson::padded_string padded(json_str);
    simdjson::ondemand::document json_doc;
    auto error = parser.iterate(padded).get(json_doc);
    if (!error) {
        auto add_error = store.add_document(json_doc);
        if (add_error == VectorStoreError::SUCCESS) {
            std::cout << "Document successfully added to store" << std::endl;
            
            // Get the entry to examine BM25 processing
            if (store.size() > 0) {
                const auto& entry = store.get_entry(0);
                std::cout << "Document length: " << entry.length << " tokens" << std::endl;
                std::cout << "Term frequencies:" << std::endl;
                
                for (const auto& tf_pair : entry.tf) {
                    std::cout << "  \"" << tf_pair.first << "\": " << tf_pair.second << std::endl;
                }
            }
        }
    }
}

int main() {
    std::cout << "🧪 BM25 API Test Suite" << std::endl;
    std::cout << "=====================" << std::endl;
    
    try {
        test_simple_tokenizer();
        test_english_punctuations();
        test_document_analysis();
        test_bm25_search();
        
        std::cout << "\n✅ All BM25 API tests completed successfully!" << std::endl;
        
    } catch (const std::exception& e) {
        std::cerr << "❌ Test failed with exception: " << e.what() << std::endl;
        return 1;
    } catch (...) {
        std::cerr << "❌ Test failed with unknown exception" << std::endl;
        return 1;
    }
    
    return 0;
}