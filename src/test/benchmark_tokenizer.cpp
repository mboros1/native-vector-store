#include "../simple_tokenizer.h"
#include <iostream>
#include <chrono>
#include <vector>
#include <string>

int main() {
    nvs::SimpleTokenizer tokenizer(true);  // Enable contraction splitting
    
    // Create test data
    std::vector<std::string> test_texts = {
        "The quick brown fox jumps over the lazy dog.",
        "Dr. Smith and Prof. Jones won't be attending today's meeting.",
        "I can't believe it's already 2025! Time flies...",
        "The U.S. government announced new policies, effective immediately.",
        "Machine learning, artificial intelligence, and data science are hot topics.",
        "It's amazing how CTRE's compile-time regex improves performance!",
        "Testing one,two,three: does the tokenizer handle this well?",
        "Numbers like 123.45 and dates like 2025-08-28 should be tokenized correctly."
    };
    
    // Warm up
    for (int i = 0; i < 100; ++i) {
        for (const auto& text : test_texts) {
            auto tokens = tokenizer.split(text);
        }
    }
    
    // Benchmark
    const int iterations = 10000;
    auto start = std::chrono::high_resolution_clock::now();
    
    size_t total_tokens = 0;
    for (int i = 0; i < iterations; ++i) {
        for (const auto& text : test_texts) {
            auto tokens = tokenizer.split(text);
            total_tokens += tokens.size();
        }
    }
    
    auto end = std::chrono::high_resolution_clock::now();
    auto duration = std::chrono::duration_cast<std::chrono::microseconds>(end - start);
    
    std::cout << "CTRE-based SimpleTokenizer Performance:\n";
    std::cout << "=========================================\n";
    std::cout << "Iterations: " << iterations << "\n";
    std::cout << "Texts per iteration: " << test_texts.size() << "\n";
    std::cout << "Total texts processed: " << (iterations * test_texts.size()) << "\n";
    std::cout << "Total tokens generated: " << total_tokens << "\n";
    std::cout << "Total time: " << duration.count() << " microseconds\n";
    std::cout << "Time per text: " << (duration.count() / (double)(iterations * test_texts.size())) << " microseconds\n";
    std::cout << "Texts per second: " << ((iterations * test_texts.size()) * 1000000.0 / duration.count()) << "\n";
    std::cout << "Tokens per second: " << (total_tokens * 1000000.0 / duration.count()) << "\n";
    
    return 0;
}