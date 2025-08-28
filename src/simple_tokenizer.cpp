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

// Unit tests - only compiled when tests are enabled
#ifdef NVS_ENABLE_INLINE_TESTS
#include "../deps/doctest.h"
#include <algorithm>

TEST_CASE("SimpleTokenizer basic tokenization") {
    using namespace nvs;
    
    SUBCASE("Simple sentence tokenization") {
        SimpleTokenizer tokenizer;
        auto tokens = tokenizer.split("Hello world");
        
        CHECK(tokens.size() == 2);
        CHECK(tokens[0] == "Hello");
        CHECK(tokens[1] == "world");
    }
    
    SUBCASE("Tokenize with punctuation") {
        SimpleTokenizer tokenizer;
        auto tokens = tokenizer.split("Hello, world!");
        
        CHECK(tokens.size() == 4);
        CHECK(tokens[0] == "Hello");
        CHECK(tokens[1] == ",");
        CHECK(tokens[2] == "world");
        CHECK(tokens[3] == "!");
    }
    
    SUBCASE("Tokenize with periods") {
        SimpleTokenizer tokenizer;
        auto tokens = tokenizer.split("This is a sentence.");
        
        CHECK(std::find(tokens.begin(), tokens.end(), ".") != tokens.end());
        CHECK(std::find(tokens.begin(), tokens.end(), "sentence") != tokens.end());
    }
    
    SUBCASE("Handle empty string") {
        SimpleTokenizer tokenizer;
        auto tokens = tokenizer.split("");
        
        CHECK(tokens.empty());
    }
    
    SUBCASE("Handle whitespace-only string") {
        SimpleTokenizer tokenizer;
        auto tokens = tokenizer.split("   \t\n  ");
        
        CHECK(tokens.empty());
    }
    
    SUBCASE("Handle multiple spaces") {
        SimpleTokenizer tokenizer;
        auto tokens = tokenizer.split("word1    word2     word3");
        
        CHECK(tokens.size() == 3);
        CHECK(tokens[0] == "word1");
        CHECK(tokens[1] == "word2");
        CHECK(tokens[2] == "word3");
    }
}

TEST_CASE("SimpleTokenizer contractions") {
    using namespace nvs;
    
    SUBCASE("No contraction splitting by default") {
        SimpleTokenizer tokenizer(false);
        auto tokens = tokenizer.split("can't won't shouldn't");
        
        CHECK(tokens.size() == 3);
        CHECK(tokens[0] == "can't");
        CHECK(tokens[1] == "won't");
        CHECK(tokens[2] == "shouldn't");
    }
    
    SUBCASE("Split contractions when enabled") {
        SimpleTokenizer tokenizer(true);
        auto tokens = tokenizer.split("can't");
        
        // Should split into "can" and "not"
        CHECK(std::find(tokens.begin(), tokens.end(), "can") != tokens.end());
        CHECK(std::find(tokens.begin(), tokens.end(), "not") != tokens.end());
    }
    
    SUBCASE("Split won't contraction") {
        SimpleTokenizer tokenizer(true);
        auto tokens = tokenizer.split("won't");
        
        CHECK(std::find(tokens.begin(), tokens.end(), "will") != tokens.end());
        CHECK(std::find(tokens.begin(), tokens.end(), "not") != tokens.end());
    }
    
    SUBCASE("Split shan't contraction") {
        SimpleTokenizer tokenizer(true);
        auto tokens = tokenizer.split("shan't");
        
        CHECK(std::find(tokens.begin(), tokens.end(), "shall") != tokens.end());
        CHECK(std::find(tokens.begin(), tokens.end(), "not") != tokens.end());
    }
    
    SUBCASE("Split ain't contraction") {
        SimpleTokenizer tokenizer(true);
        auto tokens = tokenizer.split("ain't");
        
        CHECK(std::find(tokens.begin(), tokens.end(), "am") != tokens.end());
        CHECK(std::find(tokens.begin(), tokens.end(), "not") != tokens.end());
    }
    
    SUBCASE("Handle possessives") {
        SimpleTokenizer tokenizer(true);
        auto tokens = tokenizer.split("John's book");
        
        // Should contain "John", "'s", and "book"
        CHECK(std::find(tokens.begin(), tokens.end(), "John") != tokens.end());
        CHECK(std::find(tokens.begin(), tokens.end(), "book") != tokens.end());
    }
}

TEST_CASE("SimpleTokenizer special characters") {
    using namespace nvs;
    
    SUBCASE("Handle parentheses") {
        SimpleTokenizer tokenizer;
        auto tokens = tokenizer.split("(hello) world");
        
        CHECK(tokens.size() == 4);
        CHECK(tokens[0] == "(");
        CHECK(tokens[1] == "hello");
        CHECK(tokens[2] == ")");
        CHECK(tokens[3] == "world");
    }
    
    SUBCASE("Handle brackets") {
        SimpleTokenizer tokenizer;
        auto tokens = tokenizer.split("[test] {example}");
        
        CHECK(std::find(tokens.begin(), tokens.end(), "[") != tokens.end());
        CHECK(std::find(tokens.begin(), tokens.end(), "]") != tokens.end());
        CHECK(std::find(tokens.begin(), tokens.end(), "{") != tokens.end());
        CHECK(std::find(tokens.begin(), tokens.end(), "}") != tokens.end());
        CHECK(std::find(tokens.begin(), tokens.end(), "test") != tokens.end());
        CHECK(std::find(tokens.begin(), tokens.end(), "example") != tokens.end());
    }
    
    SUBCASE("Handle quotes") {
        SimpleTokenizer tokenizer;
        auto tokens = tokenizer.split("\"hello\" 'world'");
        
        CHECK(std::find(tokens.begin(), tokens.end(), "\"") != tokens.end());
        // Single quotes may be handled differently
        CHECK(std::find(tokens.begin(), tokens.end(), "hello") != tokens.end());
        // Single quotes stay with the word
        CHECK(std::find(tokens.begin(), tokens.end(), "'world'") != tokens.end());
    }
    
    SUBCASE("Handle hyphens in words") {
        SimpleTokenizer tokenizer;
        auto tokens = tokenizer.split("state-of-the-art");
        
        // Hyphens should be preserved in compound words
        CHECK(tokens.size() == 1);
        CHECK(tokens[0] == "state-of-the-art");
    }
    
    SUBCASE("Handle forward slashes") {
        SimpleTokenizer tokenizer;
        auto tokens = tokenizer.split("and/or yes/no");
        
        // Slashes should be preserved
        CHECK(std::find(tokens.begin(), tokens.end(), "and/or") != tokens.end());
        CHECK(std::find(tokens.begin(), tokens.end(), "yes/no") != tokens.end());
    }
    
    SUBCASE("Handle ellipsis") {
        SimpleTokenizer tokenizer;
        auto tokens = tokenizer.split("wait... what...");
        
        // Ellipsis should be tokenized
        CHECK(tokens.size() >= 4);  // "wait", "...", "what", "..."
    }
}

TEST_CASE("SimpleTokenizer numbers and decimals") {
    using namespace nvs;
    
    SUBCASE("Handle integers") {
        SimpleTokenizer tokenizer;
        auto tokens = tokenizer.split("I have 42 apples");
        
        CHECK(tokens.size() == 4);
        CHECK(std::find(tokens.begin(), tokens.end(), "42") != tokens.end());
    }
    
    SUBCASE("Handle decimal numbers") {
        SimpleTokenizer tokenizer;
        auto tokens = tokenizer.split("The price is 3.14");
        
        CHECK(std::find(tokens.begin(), tokens.end(), "3.14") != tokens.end());
    }
    
    SUBCASE("Handle negative numbers") {
        SimpleTokenizer tokenizer;
        auto tokens = tokenizer.split("Temperature is -10 degrees");
        
        // Check if negative numbers are tokenized  (may be together or separate)
        bool has_negative = std::find(tokens.begin(), tokens.end(), "-10") != tokens.end();
        bool has_dash = std::find(tokens.begin(), tokens.end(), "-") != tokens.end();
        bool has_ten = std::find(tokens.begin(), tokens.end(), "10") != tokens.end();
        bool found_number = has_negative || (has_dash && has_ten);
        CHECK(found_number);
    }
    
    SUBCASE("Handle percentages") {
        SimpleTokenizer tokenizer;
        auto tokens = tokenizer.split("Growth of 25%");
        
        CHECK(std::find(tokens.begin(), tokens.end(), "25") != tokens.end());
        CHECK(std::find(tokens.begin(), tokens.end(), "%") != tokens.end());
    }
    
    SUBCASE("Handle currency") {
        SimpleTokenizer tokenizer;
        auto tokens = tokenizer.split("Cost: $19.99");
        
        CHECK(std::find(tokens.begin(), tokens.end(), "$") != tokens.end());
        CHECK(std::find(tokens.begin(), tokens.end(), "19.99") != tokens.end());
    }
}

TEST_CASE("SimpleTokenizer case handling") {
    using namespace nvs;
    
    SUBCASE("Preserve case") {
        SimpleTokenizer tokenizer;
        auto tokens = tokenizer.split("HELLO World MiXeD");
        
        CHECK(tokens.size() == 3);
        CHECK(tokens[0] == "HELLO");
        CHECK(tokens[1] == "World");
        CHECK(tokens[2] == "MiXeD");
    }
    
    SUBCASE("Handle acronyms") {
        SimpleTokenizer tokenizer;
        auto tokens = tokenizer.split("USA FBI NASA");
        
        CHECK(tokens[0] == "USA");
        CHECK(tokens[1] == "FBI");
        CHECK(tokens[2] == "NASA");
    }
    
    SUBCASE("Handle camelCase") {
        SimpleTokenizer tokenizer;
        auto tokens = tokenizer.split("camelCaseWord");
        
        // Should be treated as single token, preserving case
        CHECK(tokens.size() == 1);
        CHECK(tokens[0] == "camelCaseWord");
    }
}

TEST_CASE("SimpleTokenizer complex text") {
    using namespace nvs;
    
    SUBCASE("Handle URLs") {
        SimpleTokenizer tokenizer;
        auto tokens = tokenizer.split("Visit https://example.com");
        
        // URL parts should be tokenized
        CHECK(tokens.size() > 2);
        CHECK(std::find(tokens.begin(), tokens.end(), "Visit") != tokens.end());
        CHECK(std::find(tokens.begin(), tokens.end(), "https") != tokens.end());
    }
    
    SUBCASE("Handle email addresses") {
        SimpleTokenizer tokenizer;
        auto tokens = tokenizer.split("Contact: user@example.com");
        
        // Email should be tokenized into parts
        CHECK(std::find(tokens.begin(), tokens.end(), "Contact") != tokens.end());
        CHECK(std::find(tokens.begin(), tokens.end(), "user") != tokens.end());
        CHECK(std::find(tokens.begin(), tokens.end(), "@") != tokens.end());
    }
    
    SUBCASE("Handle mixed content") {
        SimpleTokenizer tokenizer;
        auto tokens = tokenizer.split("The meeting is at 3:30 p.m. (EST).");
        
        CHECK(tokens.size() > 5);
        CHECK(std::find(tokens.begin(), tokens.end(), "meeting") != tokens.end());
        CHECK(std::find(tokens.begin(), tokens.end(), "3") != tokens.end());
        CHECK(std::find(tokens.begin(), tokens.end(), ":") != tokens.end());
        CHECK(std::find(tokens.begin(), tokens.end(), "30") != tokens.end());
    }
    
    SUBCASE("Handle abbreviations") {
        SimpleTokenizer tokenizer;
        auto tokens = tokenizer.split("Dr. Smith works at St. Mary's Hospital.");
        
        // Should handle period after abbreviation
        bool has_dr_dot = std::find(tokens.begin(), tokens.end(), "Dr.") != tokens.end();
        bool has_dr = std::find(tokens.begin(), tokens.end(), "Dr") != tokens.end();
        bool found_dr = has_dr_dot || has_dr;
        CHECK(found_dr);
        
        bool has_st_dot = std::find(tokens.begin(), tokens.end(), "St.") != tokens.end();
        bool has_st = std::find(tokens.begin(), tokens.end(), "St") != tokens.end();
        bool found_st = has_st_dot || has_st;
        CHECK(found_st);
    }
}

TEST_CASE("SimpleTokenizer edge cases") {
    using namespace nvs;
    
    SUBCASE("Very long word") {
        SimpleTokenizer tokenizer;
        std::string long_word(1000, 'a');
        auto tokens = tokenizer.split(long_word);
        
        CHECK(tokens.size() == 1);
        CHECK(tokens[0].length() == 1000);
    }
    
    SUBCASE("Unicode characters") {
        SimpleTokenizer tokenizer;
        auto tokens = tokenizer.split("café naïve");
        
        // Unicode may be tokenized differently depending on encoding
        CHECK(tokens.size() >= 2);
    }
    
    SUBCASE("Multiple punctuation marks") {
        SimpleTokenizer tokenizer;
        auto tokens = tokenizer.split("What?!?!");
        
        CHECK(std::find(tokens.begin(), tokens.end(), "What") != tokens.end());
        CHECK(std::count(tokens.begin(), tokens.end(), "?") >= 2);
        CHECK(std::count(tokens.begin(), tokens.end(), "!") >= 2);
    }
    
    SUBCASE("Tab and newline characters") {
        SimpleTokenizer tokenizer;
        auto tokens = tokenizer.split("word1\tword2\nword3\r\nword4");
        
        CHECK(tokens.size() == 4);
        CHECK(tokens[0] == "word1");
        CHECK(tokens[1] == "word2");
        CHECK(tokens[2] == "word3");
        CHECK(tokens[3] == "word4");
    }
    
    SUBCASE("Leading and trailing spaces") {
        SimpleTokenizer tokenizer;
        auto tokens = tokenizer.split("   leading and trailing   ");
        
        CHECK(tokens.size() == 3);
        CHECK(tokens[0] == "leading");
        CHECK(tokens[1] == "and");
        CHECK(tokens[2] == "trailing");
    }
}

TEST_CASE("SimpleTokenizer performance characteristics") {
    using namespace nvs;
    
    SUBCASE("Handle large text") {
        SimpleTokenizer tokenizer;
        
        // Create a large text with known word count
        std::string large_text;
        int expected_words = 10000;
        for (int i = 0; i < expected_words; ++i) {
            large_text += "word" + std::to_string(i) + " ";
        }
        
        auto tokens = tokenizer.split(large_text);
        
        CHECK(tokens.size() == expected_words);
    }
    
    SUBCASE("Consistent tokenization") {
        SimpleTokenizer tokenizer;
        std::string text = "This is a test. This is only a test!";
        
        auto tokens1 = tokenizer.split(text);
        auto tokens2 = tokenizer.split(text);
        
        CHECK(tokens1.size() == tokens2.size());
        for (size_t i = 0; i < tokens1.size(); ++i) {
            CHECK(tokens1[i] == tokens2[i]);
        }
    }
}
#endif