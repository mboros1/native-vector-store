#include <fast_pdf_parser/tiktoken_tokenizer.h>
#include <iostream>
#include <string>

int main() {
    fast_pdf_parser::TiktokenTokenizer tokenizer;
    
    // Test some strings
    std::vector<std::string> test_strings = {
        "Hello, world!",
        "The quick brown fox jumps over the lazy dog.",
        "This is a test of the tiktoken tokenizer.",
        "C++ code: int main() { return 0; }",
        "1234567890",
        "Special chars: @#$%^&*()",
        // A longer test approximating 512 tokens
        "The semantic descriptions in this International Standard define a parameterized nondeterministic abstract "
        "machine. This International Standard places no requirement on the structure of conforming implementations. "
        "In particular, they need not copy or emulate the structure of the abstract machine. Rather, conforming "
        "implementations are required to emulate (only) the observable behavior of the abstract machine as explained "
        "below. Certain aspects and operations of the abstract machine are described in this International Standard as "
        "implementation-defined (for example, sizeof(int)). These constitute the parameters of the abstract machine. "
        "Each implementation shall include documentation describing its characteristics and behavior in these respects."
    };
    
    std::cout << "Tiktoken Token Counter Test\n";
    std::cout << "===========================\n\n";
    
    for (const auto& text : test_strings) {
        size_t char_count = text.length();
        size_t token_count = tokenizer.count_tokens(text);
        double ratio = static_cast<double>(char_count) / token_count;
        
        std::cout << "Text: \"" << (text.length() > 50 ? text.substr(0, 47) + "..." : text) << "\"\n";
        std::cout << "  Characters: " << char_count << "\n";
        std::cout << "  Tokens: " << token_count << "\n";
        std::cout << "  Chars/Token: " << ratio << "\n";
        
        // Show first 10 token IDs
        auto tokens = tokenizer.encode(text);
        std::cout << "  Token IDs: ";
        for (size_t i = 0; i < tokens.size() && i < 10; ++i) {
            std::cout << tokens[i];
            if (i < tokens.size() - 1) std::cout << ", ";
        }
        if (tokens.size() > 10) std::cout << "...";
        std::cout << "\n\n";
    }
    
    // Test encoding/decoding
    std::cout << "Encode/Decode Test:\n";
    std::string test = "Hello, tiktoken!";
    auto tokens = tokenizer.encode(test);
    std::string decoded = tokenizer.decode(tokens);
    std::cout << "Original: \"" << test << "\"\n";
    std::cout << "Encoded: ";
    for (int t : tokens) std::cout << t << " ";
    std::cout << "\nDecoded: \"" << decoded << "\"\n";
    std::cout << "Match: " << (test == decoded ? "YES" : "NO") << "\n";
    
    return 0;
}