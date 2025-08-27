#ifndef SIMPLE_SENTENCE_SPLITTER_H
#define SIMPLE_SENTENCE_SPLITTER_H

#include <string>
#include <vector>
#include <string_view>

class SimpleSentenceSplitter {
public:
    std::vector<std::string> split(const std::string& text) const {
        std::vector<std::string> sentences;
        std::string current_sentence;
        
        size_t i = 0;
        while (i < text.length()) {
            char c = text[i];
            current_sentence += c;
            
            // Check for sentence endings
            if (c == '.' || c == '!' || c == '?') {
                // Look ahead to see if this is really the end of a sentence
                size_t next = i + 1;
                
                // Skip any closing quotes or brackets
                while (next < text.length() && 
                       (text[next] == '"' || text[next] == '\'' || 
                        text[next] == ')' || text[next] == ']')) {
                    current_sentence += text[next];
                    next++;
                }
                
                // Skip whitespace
                size_t ws_start = next;
                while (next < text.length() && std::isspace(text[next])) {
                    next++;
                }
                
                // Check if the next character indicates a new sentence
                bool is_sentence_end = false;
                if (next >= text.length()) {
                    is_sentence_end = true;  // End of text
                } else if (std::isupper(text[next])) {
                    is_sentence_end = true;  // Next sentence starts with capital
                } else if (next - ws_start > 1) {
                    is_sentence_end = true;  // Multiple spaces/newlines
                }
                
                // Check for common abbreviations that shouldn't end sentences
                if (is_sentence_end && c == '.') {
                    // Simple check for common patterns like "Mr." "Dr." "Inc." etc
                    if (current_sentence.length() >= 3) {
                        size_t word_start = current_sentence.rfind(' ', current_sentence.length() - 2);
                        if (word_start == std::string::npos) word_start = 0;
                        else word_start++;
                        
                        std::string last_word = current_sentence.substr(word_start);
                        // Remove trailing period for comparison
                        if (!last_word.empty() && last_word.back() == '.') {
                            last_word.pop_back();
                        }
                        
                        // Check if it's a common abbreviation
                        if (is_common_abbreviation(last_word)) {
                            is_sentence_end = false;
                        }
                    }
                }
                
                if (is_sentence_end) {
                    // Trim whitespace from the sentence
                    size_t first = current_sentence.find_first_not_of(" \t\n\r");
                    size_t last = current_sentence.find_last_not_of(" \t\n\r");
                    if (first != std::string::npos) {
                        sentences.push_back(current_sentence.substr(first, last - first + 1));
                    }
                    current_sentence.clear();
                    i = next - 1;  // Position before the next non-whitespace character
                }
            }
            i++;
        }
        
        // Don't forget the last sentence if it doesn't end with punctuation
        if (!current_sentence.empty()) {
            size_t first = current_sentence.find_first_not_of(" \t\n\r");
            size_t last = current_sentence.find_last_not_of(" \t\n\r");
            if (first != std::string::npos) {
                sentences.push_back(current_sentence.substr(first, last - first + 1));
            }
        }
        
        return sentences;
    }
    
private:
    bool is_common_abbreviation(const std::string& word) const {
        // Common abbreviations that don't end sentences
        static const std::vector<std::string> abbreviations = {
            "Mr", "Mrs", "Ms", "Dr", "Prof", "Sr", "Jr",
            "Inc", "Corp", "Ltd", "Co", "vs", "etc", "al",
            "Jan", "Feb", "Mar", "Apr", "Jun", "Jul", "Aug", "Sep", "Sept", "Oct", "Nov", "Dec",
            "Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun",
            "St", "Ave", "Rd", "Blvd", "Dept", "Univ", "Prof",
            "Ph", "M", "B", "D",  // Ph.D., M.D., B.S., etc.
            "U", "S", "E", "N", "W",  // U.S., E.U., N.Y., etc.
            "i", "e", "g",  // i.e., e.g.
        };
        
        for (const auto& abbr : abbreviations) {
            if (word == abbr) return true;
        }
        
        // Check for single capital letters (like in "U.S.A.")
        if (word.length() == 1 && std::isupper(word[0])) {
            return true;
        }
        
        // Check for numbers ending in period (like "1." in lists)
        if (!word.empty() && std::isdigit(word[0])) {
            return true;
        }
        
        return false;
    }
};

#endif // SIMPLE_SENTENCE_SPLITTER_H