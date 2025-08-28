#pragma once
#include <string>
#include <vector>
#include <unordered_set>
#include <algorithm>
#include "ctre-unicode.hpp"

namespace nvs {

class SimpleTokenizer {
public:
    explicit SimpleTokenizer(bool splitContraction = false)
     : splitContraction_(splitContraction)
    {}

    std::vector<std::string> split(const std::string& input);

private:
    bool splitContraction_;

    // Process contractions
    std::string process_contractions(std::string text) const;
    
    // Process delimiters  
    std::string process_delimiters(std::string text) const;

    // very minimal abbreviation set; expand as needed
    static bool isAbbreviation(const std::string& tok);
};

} // namespace nvs