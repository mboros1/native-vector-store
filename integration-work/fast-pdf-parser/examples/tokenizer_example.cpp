/**
 * Example demonstrating the tiktoken tokenizer
 * Compile with: clang++ -std=c++17 -Iinclude tokenizer_example.cpp -o tokenizer_example
 */

#include <fast_pdf_parser/tiktoken_tokenizer.h>
#include <iostream>

int main() {
    fast_pdf_parser::TiktokenTokenizer tokenizer;
    
    std::string text = "The quick brown fox jumps over the lazy dog. "
                      "This demonstrates tokenization with tiktoken!";
    
    // Count tokens
    size_t token_count = tokenizer.count_tokens(text);
    std::cout << "Text: \"" << text << "\"\n";
    std::cout << "Token count: " << token_count << "\n";
    
    // Show actual tokens
    auto tokens = tokenizer.encode(text);
    std::cout << "Tokens: ";
    for (int t : tokens) std::cout << t << " ";
    std::cout << "\n";
    
    // Decode back
    std::string decoded = tokenizer.decode(tokens);
    std::cout << "Decoded: \"" << decoded << "\"\n";
    std::cout << "Match: " << (text == decoded ? "YES" : "NO") << "\n";
    
    // Compare with estimate
    size_t estimated = fast_pdf_parser::TiktokenTokenizer::estimate_tokens(text);
    std::cout << "\nEstimated tokens: " << estimated 
              << " (off by " << static_cast<int>(estimated) - static_cast<int>(token_count) << ")\n";
    
    return 0;
}