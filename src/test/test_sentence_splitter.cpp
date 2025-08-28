#include <iostream>
#include <vector>
#include <string>
#include "../simple_sentence_splitter.h"

void test_basic_splitting() {
    nvs::SimpleSentenceSplitter splitter;
    
    std::cout << "Test 1: Basic sentence splitting\n";
    std::string text = "This is the first sentence. This is the second! Is this the third? Yes it is.";
    auto sentences = splitter.split(text);
    
    std::cout << "Input: \"" << text << "\"\n";
    std::cout << "Found " << sentences.size() << " sentences:\n";
    for (size_t i = 0; i < sentences.size(); ++i) {
        std::cout << "  [" << i+1 << "] \"" << sentences[i] << "\"\n";
    }
    std::cout << "\n";
}

void test_abbreviations() {
    nvs::SimpleSentenceSplitter splitter;
    
    std::cout << "Test 2: Handling abbreviations\n";
    std::string text = "Dr. Smith works at the U.S. Dept. of Defense. He arrived at 3 p.m. yesterday.";
    auto sentences = splitter.split(text);
    
    std::cout << "Input: \"" << text << "\"\n";
    std::cout << "Found " << sentences.size() << " sentences:\n";
    for (size_t i = 0; i < sentences.size(); ++i) {
        std::cout << "  [" << i+1 << "] \"" << sentences[i] << "\"\n";
    }
    std::cout << "\n";
}

void test_missing_spaces() {
    nvs::SimpleSentenceSplitter splitter;
    
    std::cout << "Test 3: Missing spaces after punctuation\n";
    std::string text = "I believe.I think.Therefore I am! Really?Yes, really.";
    auto sentences = splitter.split(text);
    
    std::cout << "Input: \"" << text << "\"\n";
    std::cout << "Found " << sentences.size() << " sentences:\n";
    for (size_t i = 0; i < sentences.size(); ++i) {
        std::cout << "  [" << i+1 << "] \"" << sentences[i] << "\"\n";
    }
    std::cout << "\n";
}

void test_quotes_and_brackets() {
    nvs::SimpleSentenceSplitter splitter;
    
    std::cout << "Test 4: Quotes and brackets\n";
    std::string text = "He said \"Hello there!\" Then he left. (This was unexpected.) \"Why?\" she asked.";
    auto sentences = splitter.split(text);
    
    std::cout << "Input: \"" << text << "\"\n";
    std::cout << "Found " << sentences.size() << " sentences:\n";
    for (size_t i = 0; i < sentences.size(); ++i) {
        std::cout << "  [" << i+1 << "] \"" << sentences[i] << "\"\n";
    }
    std::cout << "\n";
}

void test_complex_text() {
    nvs::SimpleSentenceSplitter splitter;
    
    std::cout << "Test 5: Complex real-world text\n";
    std::string text = "The company, founded in 1985 by Mr. John Smith Jr., specializes in A.I. "
                       "and machine learning. Its revenue was $2.5 billion in 2023. "
                       "The C.E.O. announced: \"We're expanding to the U.K. and E.U. markets!\" "
                       "This is exciting news for investors.";
    auto sentences = splitter.split(text);
    
    std::cout << "Input: \"" << text << "\"\n";
    std::cout << "Found " << sentences.size() << " sentences:\n";
    for (size_t i = 0; i < sentences.size(); ++i) {
        std::cout << "  [" << i+1 << "] \"" << sentences[i] << "\"\n";
    }
    std::cout << "\n";
}

int main() {
    std::cout << "========================================\n";
    std::cout << "Simple Sentence Splitter Test Suite\n";
    std::cout << "========================================\n\n";
    
    test_basic_splitting();
    test_abbreviations();
    test_missing_spaces();
    test_quotes_and_brackets();
    test_complex_text();
    
    std::cout << "All tests completed!\n";
    return 0;
}