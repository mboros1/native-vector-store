#pragma once
#include <string>
#include <vector>
#include <regex>
#include <unordered_set>

class SimpleTokenizer {
public:
    explicit SimpleTokenizer(bool splitContraction = false)
     : splitContraction_(splitContraction)
    {}

    std::vector<std::string> split(const std::string& input);

private:
    bool splitContraction_;

    // -- static precompiled regexes --
    static const std::regex WONT_CONTRACTION;
    static const std::regex SHANT_CONTRACTION;
    static const std::regex AINT_CONTRACTION;
    static const std::vector<std::regex> NOT_CONTRACTIONS;
    static const std::vector<std::regex> CONTRACTIONS2;
    static const std::vector<std::regex> CONTRACTIONS3;
    static const std::vector<std::regex> DELIMITERS;
    static const std::regex WHITESPACE;

    // very minimal abbreviation set; expand as needed
    static bool isAbbreviation(const std::string& tok);
};