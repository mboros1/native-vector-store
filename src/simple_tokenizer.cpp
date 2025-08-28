#include "simple_tokenizer.h"
#include <sstream>

namespace nvs {

std::vector<std::string> SimpleTokenizer::split(const std::string& input) {
    std::string text = input;

    if (splitContraction_) {
        text = std::regex_replace(text, WONT_CONTRACTION, "$1ill not");
        text = std::regex_replace(text, SHANT_CONTRACTION, "$1ll not");
        text = std::regex_replace(text, AINT_CONTRACTION, "$1m not");

        for (auto& re : NOT_CONTRACTIONS)
            text = std::regex_replace(text, re, "$1 not");
        for (auto& re : CONTRACTIONS2)
            text = std::regex_replace(text, re, "$1 $2");
        for (auto& re : CONTRACTIONS3)
            text = std::regex_replace(text, re, "$1 $2 $3");
    }

    // apply delimiter rules
    text = std::regex_replace(text, DELIMITERS[0], " $1 ");
    text = std::regex_replace(text, DELIMITERS[1], " $1");
    text = std::regex_replace(text, DELIMITERS[2], " $1");
    text = std::regex_replace(text, DELIMITERS[3], " . ");
    text = std::regex_replace(text, DELIMITERS[4], " $1 ");

    // split on whitespace
    std::vector<std::string> tokens;
    std::sregex_token_iterator it(text.begin(), text.end(), WHITESPACE, -1), end;
    for (; it != end; ++it) {
        if (!it->str().empty())
            tokens.push_back(it->str());
    }

    // handle trailing "." with abbreviation
    if (tokens.size() > 1 && tokens.back() == "." &&
        isAbbreviation(tokens[tokens.size()-2]))
    {
        tokens[tokens.size()-2] += ".";
        tokens.pop_back();
    }

    return tokens;
}

bool SimpleTokenizer::isAbbreviation(const std::string& tok) {
    static const std::unordered_set<std::string> abbr = {
        "etc", "Mr", "Mrs", "Dr", "U.S.A"
    };
    return abbr.count(tok) > 0;
}

// Static regex definitions - using case-insensitive flag instead of (?i)
const std::regex SimpleTokenizer::WONT_CONTRACTION(
    "\\b(w)(on't)\\b", std::regex_constants::icase);
const std::regex SimpleTokenizer::SHANT_CONTRACTION(
    "\\b(sha)(n't)\\b", std::regex_constants::icase);
const std::regex SimpleTokenizer::AINT_CONTRACTION(
    "\\b(a)(in't)\\b", std::regex_constants::icase);

const std::vector<std::regex> SimpleTokenizer::NOT_CONTRACTIONS = {
    std::regex("\\b(can)('t|not)\\b", std::regex_constants::icase),
    std::regex("(.)(n't)\\b", std::regex_constants::icase)
};

const std::vector<std::regex> SimpleTokenizer::CONTRACTIONS2 = {
    std::regex("(.)('ll|'re|'ve|'s|'m|'d)\\b", std::regex_constants::icase),
    std::regex("\\b(D)('ye)\\b", std::regex_constants::icase),
    std::regex("\\b(Gim)(me)\\b", std::regex_constants::icase),
    std::regex("\\b(Gon)(na)\\b", std::regex_constants::icase),
    std::regex("\\b(Got)(ta)\\b", std::regex_constants::icase),
    std::regex("\\b(Lem)(me)\\b", std::regex_constants::icase),
    std::regex("\\b(Mor)('n)\\b", std::regex_constants::icase),
    std::regex("\\b(T)(is)\\b", std::regex_constants::icase),
    std::regex("\\b(T)(was)\\b", std::regex_constants::icase),
    std::regex("\\b(Wan)(na)\\b", std::regex_constants::icase)
};

const std::vector<std::regex> SimpleTokenizer::CONTRACTIONS3 = {
    std::regex("\\b(Whad)(dd)(ya)\\b", std::regex_constants::icase),
    std::regex("\\b(Wha)(t)(cha)\\b", std::regex_constants::icase)
};

const std::vector<std::regex> SimpleTokenizer::DELIMITERS = {
    std::regex("([^\\w\\.\\'\\-\\/,&])"),
    std::regex("(,\\s)"),
    std::regex("('\\s)"),
    std::regex("\\. *(\\n|$)"),
    std::regex("(\\.{3,})")
};

const std::regex SimpleTokenizer::WHITESPACE("\\s+");

} // namespace nvs