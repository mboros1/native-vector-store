#ifndef SIMPLE_SENTENCE_SPLITTER_H
#define SIMPLE_SENTENCE_SPLITTER_H

#include <string>
#include <vector>
#include <cctype>
#include <algorithm>
#include <string_view>
#include "ctre.hpp"
#include "english_abbreviations.h"
#include "english_dictionary.h"

namespace nvs {

/**
 * Simple sentence splitter for English text, based on Smile NLP's implementation.
 * 
 * This splitter handles:
 * - Standard sentence endings (. ! ?)
 * - Abbreviations (Mr., Dr., etc.)
 * - Numbers with periods
 * - Quoted text and brackets
 * - URLs and decimal numbers
 * 
 * The implementation follows the logic from Smile NLP's SimpleSentenceSplitter
 * but adapted for C++ using CTRE for compile-time regex performance.
 */
class SimpleSentenceSplitter {
public:
    std::vector<std::string> split(const std::string& text) const {
        std::vector<std::string> sentences;
        
        if (text.empty()) {
            return sentences;
        }
        
        // Clean up the text - replace carriage returns with spaces
        std::string cleaned = text;
        for (auto& c : cleaned) {
            if (c == '\n' || c == '\r') {
                c = ' ';
            }
        }
        
        // Use \031 (end of medium) as a special character for missing space after punctuation
        const char MISSING_SPACE_MARKER = '\031';
        
        // Clean any existing markers
        for (auto& c : cleaned) {
            if (c == MISSING_SPACE_MARKER) c = ' ';
        }
        
        // Insert missing spaces after punctuation using CTRE
        // Pattern: (any char)(. or ! or ?)(non-space, non-punctuation char)
        cleaned = insert_missing_spaces(cleaned, MISSING_SPACE_MARKER);
        
        // Add newline at end for processing
        cleaned += "\n";
        
        // Process the text character by character with lookahead
        std::string current_sentence;
        size_t i = 0;
        int word_count = 0;
        
        while (i < cleaned.length()) {
            char c = cleaned[i];
            
            // Count words as we go
            if (i > 0 && !std::isspace(cleaned[i-1]) && std::isspace(c)) {
                word_count++;
            }
            
            current_sentence += c;
            
            // Check for potential sentence endings
            if (c == '.' || c == '!' || c == '?' || c == ':') {
                // Look ahead for context
                size_t next = i + 1;
                
                // Skip quotes and brackets after punctuation
                while (next < cleaned.length() && 
                       (cleaned[next] == '"' || cleaned[next] == '\'' || 
                        cleaned[next] == ')' || cleaned[next] == ']' || 
                        cleaned[next] == '}')) {
                    current_sentence += cleaned[next];
                    next++;
                }
                
                // Skip whitespace to find next word
                size_t ws_start = next;
                while (next < cleaned.length() && 
                       (std::isspace(cleaned[next]) || cleaned[next] == MISSING_SPACE_MARKER)) {
                    if (cleaned[next] != MISSING_SPACE_MARKER) {
                        current_sentence += cleaned[next];
                    }
                    next++;
                }
                
                // Determine if this is a sentence break
                bool is_sentence_break = false;
                
                if (c == '.') {
                    // Get the last word before the period
                    std::string last_word = extract_last_word(current_sentence);
                    
                    // Get the next word (if any)
                    std::string next_word;
                    if (next < cleaned.length()) {
                        size_t word_end = next;
                        while (word_end < cleaned.length() && !std::isspace(cleaned[word_end])) {
                            word_end++;
                        }
                        next_word = cleaned.substr(next, word_end - next);
                    }
                    
                    // Check various abbreviation patterns
                    if (is_abbreviation(last_word)) {
                        // Known abbreviation - only break if next word is common and we have enough context
                        if (is_common_word(next_word) && word_count > 6) {
                            is_sentence_break = true;
                        }
                    } else if (is_special_abbreviation_pattern(last_word)) {
                        // Special patterns like Ph.D., U.S.A., etc.
                        if (is_common_word(next_word) && word_count > 6) {
                            is_sentence_break = true;
                        }
                    } else if (next < cleaned.length() && std::isupper(cleaned[next])) {
                        // Next character is uppercase - likely sentence break
                        is_sentence_break = true;
                    } else if (next >= cleaned.length() - 1) {
                        // End of text
                        is_sentence_break = true;
                    }
                } else if (c == '!' || c == '?') {
                    // These almost always end sentences
                    is_sentence_break = true;
                } else if (c == ':' && word_count > 6) {
                    // Colon only ends sentence after sufficient context
                    is_sentence_break = true;
                }
                
                if (is_sentence_break) {
                    // Clean and add the sentence
                    std::string final_sentence = cleanup_sentence(current_sentence);
                    if (!final_sentence.empty()) {
                        sentences.push_back(final_sentence);
                    }
                    current_sentence.clear();
                    word_count = 0;
                    i = next - 1; // Position at last processed character
                }
            }
            
            i++;
        }
        
        // Add any remaining sentence
        if (!current_sentence.empty()) {
            std::string final_sentence = cleanup_sentence(current_sentence);
            if (!final_sentence.empty() && final_sentence != "\n") {
                sentences.push_back(final_sentence);
            }
        }
        
        return sentences;
    }
    
private:
    // Insert missing spaces after punctuation
    std::string insert_missing_spaces(const std::string& text, char marker) const {
        std::string result;
        result.reserve(text.size() + 100); // Reserve extra space
        
        // Use CTRE for pattern matching
        static constexpr auto pattern = ctll::fixed_string{R"(([.!?])([^\s."'`\)\}\]]))"};
        
        size_t last_pos = 0;
        for (auto match : ctre::search_all<pattern>(text)) {
            // Add text before match
            result.append(text, last_pos, match.begin() - text.begin() - last_pos);
            
            // Add the punctuation
            result += match.get<1>().str();
            
            // Add marker for missing space
            result += marker;
            
            // Add the character after punctuation
            result += match.get<2>().str();
            
            last_pos = match.end() - text.begin();
        }
        
        // Add remaining text
        result.append(text, last_pos);
        
        return result;
    }
    
    // Extract the last word from a sentence
    std::string extract_last_word(const std::string& sentence) const {
        if (sentence.empty()) return "";
        
        // Find the last word boundary
        size_t end = sentence.find_last_not_of(".!?:;, \t\n\r");
        if (end == std::string::npos) return "";
        
        size_t start = sentence.find_last_of(" \t\n\r", end);
        if (start == std::string::npos) {
            start = 0;
        } else {
            start++;
        }
        
        std::string word = sentence.substr(start, end - start + 1);
        
        // Remove trailing punctuation for checking
        while (!word.empty() && std::ispunct(word.back()) && word.back() != '.') {
            word.pop_back();
        }
        
        return word;
    }
    
    // Check if a word is an abbreviation using our compiled list
    bool is_abbreviation(const std::string& word) const {
        if (word.empty()) return false;
        
        std::string check_word = word;
        // Remove trailing period if present
        if (!check_word.empty() && check_word.back() == '.') {
            check_word.pop_back();
        }
        
        // Convert to lowercase for checking
        std::string lower = to_lower(check_word);
        
        return EnglishAbbreviations::instance().count(lower) > 0;
    }
    
    // Check for special abbreviation patterns
    bool is_special_abbreviation_pattern(const std::string& word) const {
        if (word.empty()) return false;
        
        // Check for letter.period pattern (U.S.A., Ph.D., etc.)
        static constexpr auto letter_period = ctll::fixed_string{R"(^([A-Za-z]\.)+$)"};
        if (ctre::match<letter_period>(word)) {
            return true;
        }
        
        // Check for all consonants with at least one lowercase
        bool has_vowel = false;
        bool has_lowercase = false;
        for (char c : word) {
            if (c == '.') continue;
            char lower_c = std::tolower(c);
            if (lower_c == 'a' || lower_c == 'e' || lower_c == 'i' || 
                lower_c == 'o' || lower_c == 'u' || lower_c == 'y') {
                has_vowel = true;
                break;
            }
            if (std::islower(c)) {
                has_lowercase = true;
            }
        }
        
        if (!has_vowel && has_lowercase) {
            return true;
        }
        
        // Single letter (except 'I')
        if (word.length() == 1 && std::isalpha(word[0]) && std::toupper(word[0]) != 'I') {
            return true;
        }
        
        return false;
    }
    
    // Check if word is in common dictionary
    bool is_common_word(const std::string& word) const {
        if (word.empty()) return false;
        
        std::string lower = to_lower(word);
        // Remove any punctuation
        while (!lower.empty() && std::ispunct(lower.back())) {
            lower.pop_back();
        }
        
        return !lower.empty() && EnglishDictionary::instance().count(lower) > 0;
    }
    
    // Convert string to lowercase
    std::string to_lower(const std::string& str) const {
        std::string result = str;
        std::transform(result.begin(), result.end(), result.begin(), 
                      [](char c) { return std::tolower(c); });
        return result;
    }
    
    // Clean up a sentence - remove special markers and trim
    std::string cleanup_sentence(const std::string& sentence) const {
        std::string result = sentence;
        
        // Remove all MISSING_SPACE_MARKER characters
        result.erase(std::remove(result.begin(), result.end(), '\031'), result.end());
        
        // Trim whitespace from both ends
        size_t first = result.find_first_not_of(" \t\n\r");
        if (first == std::string::npos) return "";
        
        size_t last = result.find_last_not_of(" \t\n\r");
        return result.substr(first, (last - first + 1));
    }
};

} // namespace nvs

#endif // SIMPLE_SENTENCE_SPLITTER_H