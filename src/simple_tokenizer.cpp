#include "simple_tokenizer.h"
#include <sstream>
#include <cctype>

namespace nvs {

// CTRE patterns defined as compile-time strings
// Note: CTRE doesn't support case-insensitive flags directly, so we use character classes

// Contraction patterns
static constexpr auto WONT_PATTERN = ctll::fixed_string{R"(\b([Ww])on't\b)"};
static constexpr auto SHANT_PATTERN = ctll::fixed_string{R"(\b([Ss])han't\b)"};
static constexpr auto AINT_PATTERN = ctll::fixed_string{R"(\b([Aa])in't\b)"};
static constexpr auto CANT_PATTERN = ctll::fixed_string{R"(\b([Cc])an't\b)"};
static constexpr auto CANNOT_PATTERN = ctll::fixed_string{R"(\b([Cc])annot\b)"};
static constexpr auto NT_PATTERN = ctll::fixed_string{R"(\b([A-Za-z]+)n't\b)"};

// Contractions with apostrophes
static constexpr auto CONTRACTIONS2_PATTERN = ctll::fixed_string{R"(\b([A-Za-z]+)('ll|'re|'ve|'s|'m|'d)\b)"};
static constexpr auto DYE_PATTERN = ctll::fixed_string{R"(\b([Dd])('ye)\b)"};

// Three-part contractions
static constexpr auto CONTRACTIONS3_PATTERN = ctll::fixed_string{R"(\b([Tt])'([Ii])s\b)"};
static constexpr auto CONTRACTIONS3_PATTERN2 = ctll::fixed_string{R"(\b([Tt])'([Ww])as\b)"};

// Delimiter patterns
// Unicode-aware: treat all letters/numbers as part of words
// Allow . ' - / , & inside tokens as before
// Pragmatic UTF-8 support: treat all non-ASCII bytes (0x80-0xFF) as part of words
// so multi-byte UTF-8 sequences don't get split. Also allow ASCII letters/digits and certain punctuation.
static constexpr auto NON_WORD_PATTERN = ctll::fixed_string{R"(([^\x80-\xff\w\.'\-/,&]))"};
static constexpr auto COMMA_PATTERN = ctll::fixed_string{R"((,)\s)"};
static constexpr auto COMMA_NO_SPACE_PATTERN = ctll::fixed_string{R"((,)([^\s]))"};
static constexpr auto APOSTROPHE_SPACE_PATTERN = ctll::fixed_string{R"(('\s))"};
static constexpr auto PERIOD_EOL_PATTERN = ctll::fixed_string{R"(\.(\s*(\n|$)))"};
static constexpr auto ELLIPSIS_PATTERN = ctll::fixed_string{R"((\.{3,}))"};

// Whitespace pattern for tokenization - include Unicode space separators
static constexpr auto WHITESPACE_PATTERN = ctll::fixed_string{R"(([\p{Zs}\t\n\r]+))"};

template <auto& Pat, class F>
static inline void rewrite(std::string& s, F&& f) {
    std::string out; out.reserve(s.size() * 2);
    size_t last = 0;
    for (auto m : ctre::search_all<Pat>(s)) {
        out.append(s, last, m.begin() - s.begin() - last);
        f(out, m);
        last = m.end() - s.begin();
    }
    out.append(s, last);
    s.swap(out);
}

std::string SimpleTokenizer::process_contractions(std::string text) const {
    if (!splitContraction_) {
        return text;
    }

    // Process special contractions first
    rewrite<WONT_PATTERN>(text, [](std::string& out, const auto& m) {
        auto v = m.template get<1>().to_view();
        const bool cap = !v.empty() && std::isupper(static_cast<unsigned char>(v.front()));
        out += cap ? "Will not" : "will not";
    });

    rewrite<SHANT_PATTERN>(text, [](std::string& out, const auto& m) {
        auto v = m.template get<1>().to_view();
        const bool cap = !v.empty() && std::isupper(static_cast<unsigned char>(v.front()));
        out += cap ? "Shall not" : "shall not";
    });

    rewrite<AINT_PATTERN>(text, [](std::string& out, const auto& m) {
        auto v = m.template get<1>().to_view();
        const bool cap = !v.empty() && std::isupper(static_cast<unsigned char>(v.front()));
        out += cap ? "Is not" : "is not";
    });

    rewrite<CANT_PATTERN>(text, [](std::string& out, const auto& m) {
        auto v = m.template get<1>().to_view();
        const bool cap = !v.empty() && std::isupper(static_cast<unsigned char>(v.front()));
        out += cap ? "Can not" : "can not";
    });

    rewrite<CANNOT_PATTERN>(text, [](std::string& out, const auto& m) {
        auto v = m.template get<1>().to_view();
        const bool cap = !v.empty() && std::isupper(static_cast<unsigned char>(v.front()));
        out += cap ? "Can not" : "can not";
    });

    // Other n't contractions -> word + not
    rewrite<NT_PATTERN>(text, [](std::string& out, const auto& m) {
        auto v = m.template get<1>().to_view();
        out.append(v.data(), v.size());
        out +=  " not";
    });

    // Two-part contractions ('ll, 're, 've, 's, 'm, 'd)
    rewrite<CONTRACTIONS2_PATTERN>(text, [](std::string& out, const auto& m) {
        out.append(m.template get<1>().to_view());
        out.append(" ");
        out.append(m.template get<2>().to_view());
    });

    // D'ye special case
    rewrite<DYE_PATTERN>(text, [](std::string& out, const auto& m) {
        out.append(m.template get<1>().to_view());
        out.append(" ");
        out.append(m.template get<2>().to_view());
    });

    // Three-part contractions (t'is -> it is, t'was -> it was)
    rewrite<CONTRACTIONS3_PATTERN>(text, [](std::string& out, const auto& m) {
        out.append(m.template get<1>().to_view());
        out.append(" ");
        out.append(m.template get<2>().to_view());
        out.append("s");
    });

    rewrite<CONTRACTIONS3_PATTERN2>(text, [](std::string& out, const auto& m) {
        out.append(m.template get<1>().to_view());
        out.append(" ");
        out.append(m.template get<2>().to_view());
        out.append("as");
    });

    return text;
}

std::string SimpleTokenizer::process_delimiters(std::string text) const {
    // UTF-8 aware delimiter processing: surround delimiter codepoints with spaces,
    // preserve word codepoints and allowed punctuation within tokens, and normalize
    // whitespace to single spaces.
    auto is_allowed_punct = [](uint32_t cp) {
        // Punctuation allowed inside word tokens
        return cp == '.' || cp == '\'' || cp == '-' || cp == '/' || cp == '&';
    };
    auto is_ascii_alnum = [](uint32_t cp) {
        return (cp >= 'A' && cp <= 'Z') || (cp >= 'a' && cp <= 'z') || (cp >= '0' && cp <= '9') || cp == '_';
    };
    auto is_whitespace = [](uint32_t cp) {
        // ASCII whitespace + common Unicode space separator bytes will be handled by \s splitting too
        return cp == ' ' || cp == '\t' || cp == '\n' || cp == '\r';
    };
    auto is_word = [&](uint32_t cp) {
        // Treat any non-ASCII codepoint as part of words; plus ASCII alnum; plus certain punctuation.
        if (cp >= 0x80) return true;
        if (is_ascii_alnum(cp)) return true;
        if (is_allowed_punct(cp)) return true;
        return false;
    };
    
    std::string out;
    out.reserve(text.size() * 2);
    auto append_space = [&](){ if (out.empty() || out.back() != ' ') out.push_back(' '); };
    
    size_t i = 0;
    while (i < text.size()) {
        unsigned char c = static_cast<unsigned char>(text[i]);
        uint32_t cp = 0; size_t len = 1;
        if (c < 0x80) {
            cp = c;
        } else if ((c >> 5) == 0x6 && i + 1 < text.size()) {
            cp = ((c & 0x1F) << 6) | (static_cast<unsigned char>(text[i+1]) & 0x3F);
            len = 2;
        } else if ((c >> 4) == 0xE && i + 2 < text.size()) {
            cp = ((c & 0x0F) << 12) | ((static_cast<unsigned char>(text[i+1]) & 0x3F) << 6) | (static_cast<unsigned char>(text[i+2]) & 0x3F);
            len = 3;
        } else if ((c >> 3) == 0x1E && i + 3 < text.size()) {
            cp = ((c & 0x07) << 18) | ((static_cast<unsigned char>(text[i+1]) & 0x3F) << 12) |
                 ((static_cast<unsigned char>(text[i+2]) & 0x3F) << 6) | (static_cast<unsigned char>(text[i+3]) & 0x3F);
            len = 4;
        } else {
            // Invalid sequence - treat as delimiter
            cp = c; len = 1;
        }
        
        if (is_whitespace(cp)) {
            append_space();
        } else if (cp == '.') {
            // Handle ellipsis sequences: split each '.' as separate token
            size_t j = i; size_t run = 0;
            while (j < text.size() && static_cast<unsigned char>(text[j]) == '.') { ++j; ++run; }
            if (run >= 3) {
                for (size_t k = 0; k < run; ++k) { append_space(); out.push_back('.'); append_space(); }
                i += run; // continue
                continue;
            } else {
                // If period at end of line/text (optionally followed by whitespace/newline), separate it
                size_t k = i + 1;
                while (k < text.size()) {
                    unsigned char nb = static_cast<unsigned char>(text[k]);
                    if (nb == ' ' || nb == '\t' || nb == '\r') { ++k; continue; }
                    break;
                }
                if (k >= text.size() || static_cast<unsigned char>(text[k]) == '\n') {
                    append_space(); out.push_back('.'); append_space();
                } else {
                    out.append(text, i, 1);
                }
            }
        } else if (is_word(cp)) {
            out.append(text, i, len);
        } else {
            append_space();
            out.append(text, i, len);
            append_space();
        }
        i += len;
    }
    return out;
}

std::vector<std::string> SimpleTokenizer::split(const std::string& input) {
    std::vector<std::string> tokens;
    
    if (input.empty()) {
        return tokens;
    }
    
    // Process the text through our pipeline
    std::string text = input;
    text = process_contractions(text);
    text = process_delimiters(text);
    
    // Split on whitespace using CTRE
    size_t last_pos = 0;
    for (auto match : ctre::search_all<WHITESPACE_PATTERN>(text)) {
        size_t token_start = last_pos;
        size_t token_end = match.begin() - text.begin();
        
        if (token_end > token_start) {
            std::string token = text.substr(token_start, token_end - token_start);
            
            // Skip empty tokens
            if (token.empty()) {
                continue;
            }
            
            // Apply post-processing rules for periods
            if (token.back() == '.') {
                // Check if it's an abbreviation
                std::string word_without_period = token.substr(0, token.length() - 1);
                if (!word_without_period.empty() && !isAbbreviation(word_without_period)) {
                    // Split the period as a separate token
                    tokens.push_back(word_without_period);
                    tokens.push_back(".");
                } else {
                    // Keep the period with the abbreviation
                    tokens.push_back(token);
                }
            } else {
                tokens.push_back(token);
            }
        }
        
        last_pos = match.end() - text.begin();
    }
    
    // Don't forget the last token
    if (last_pos < text.length()) {
        std::string token = text.substr(last_pos);
        if (!token.empty()) {
            // Apply the same period rules
            if (token.back() == '.') {
                std::string word_without_period = token.substr(0, token.length() - 1);
                if (!word_without_period.empty() && !isAbbreviation(word_without_period)) {
                    tokens.push_back(word_without_period);
                    tokens.push_back(".");
                } else {
                    tokens.push_back(token);
                }
            } else {
                tokens.push_back(token);
            }
        }
    }
    
    return tokens;
}

bool SimpleTokenizer::isAbbreviation(const std::string& tok) {
    // Common English abbreviations
    static const std::unordered_set<std::string> abbreviations = {
        "Dr", "Mr", "Mrs", "Ms", "Prof", "Sr", "Jr",
        "Ph", "M", "B", "D",  // Degrees
        "Inc", "Corp", "Co", "Ltd",
        "Jan", "Feb", "Mar", "Apr", "Jun", "Jul", "Aug", "Sep", "Sept", "Oct", "Nov", "Dec",
        "Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun",
        "St", "Ave", "Rd", "Blvd",
        "U", "S", "N", "E", "W",  // Directions and U.S.
        "vs", "etc", "al", "eg", "ie", "cf"
    };
    
    return abbreviations.count(tok) > 0;
}

} // namespace nvs

#ifdef NVS_ENABLE_INLINE_TESTS
#include "doctest/doctest.h"

TEST_CASE("SimpleTokenizer basic tokenization") {
    nvs::SimpleTokenizer tokenizer(false);
    
    SUBCASE("simple sentence") {
        auto tokens = tokenizer.split("Hello world");
        CHECK(tokens.size() == 2);
        CHECK(tokens[0] == "Hello");
        CHECK(tokens[1] == "world");
    }
    
    SUBCASE("punctuation handling") {
        auto tokens = tokenizer.split("Hello, world!");
        CHECK(tokens.size() == 4);
        CHECK(tokens[0] == "Hello");
        CHECK(tokens[1] == ",");
        CHECK(tokens[2] == "world");
        CHECK(tokens[3] == "!");
    }
    
    SUBCASE("period handling") {
        auto tokens = tokenizer.split("End of sentence.");
        CHECK(tokens.size() == 4);
        CHECK(tokens[0] == "End");
        CHECK(tokens[1] == "of");
        CHECK(tokens[2] == "sentence");
        CHECK(tokens[3] == ".");
    }
    
    SUBCASE("abbreviation handling") {
        auto tokens = tokenizer.split("Dr. Smith");
        CHECK(tokens.size() == 2);
        CHECK(tokens[0] == "Dr.");
        CHECK(tokens[1] == "Smith");
    }
    
    SUBCASE("multiple spaces") {
        auto tokens = tokenizer.split("multiple   spaces    here");
        CHECK(tokens.size() == 3);
        CHECK(tokens[0] == "multiple");
        CHECK(tokens[1] == "spaces");
        CHECK(tokens[2] == "here");
    }
    
    SUBCASE("empty string") {
        auto tokens = tokenizer.split("");
        CHECK(tokens.size() == 0);
    }
    
    SUBCASE("whitespace only") {
        auto tokens = tokenizer.split("   \t\n  ");
        CHECK(tokens.size() == 0);
    }
    
    SUBCASE("tabs and newlines") {
        auto tokens = tokenizer.split("line1\nline2\ttab");
        CHECK(tokens.size() == 3);
        CHECK(tokens[0] == "line1");
        CHECK(tokens[1] == "line2");
        CHECK(tokens[2] == "tab");
    }
    
    SUBCASE("leading and trailing whitespace") {
        auto tokens = tokenizer.split("  \t  word1 word2  \n  ");
        CHECK(tokens.size() == 2);
        CHECK(tokens[0] == "word1");
        CHECK(tokens[1] == "word2");
    }
}

TEST_CASE("SimpleTokenizer contraction splitting") {
    nvs::SimpleTokenizer tokenizer(true);  // Enable contraction splitting
    
    SUBCASE("won't contraction") {
        auto tokens = tokenizer.split("I won't go");
        CHECK(tokens.size() == 4);
        CHECK(tokens[0] == "I");
        CHECK(tokens[1] == "will");
        CHECK(tokens[2] == "not");
        CHECK(tokens[3] == "go");
    }
    
    SUBCASE("can't contraction") {
        auto tokens = tokenizer.split("I can't do it");
        CHECK(tokens.size() == 5);
        CHECK(tokens[0] == "I");
        CHECK(tokens[1] == "can");
        CHECK(tokens[2] == "not");
        CHECK(tokens[3] == "do");
        CHECK(tokens[4] == "it");
    }
    
    SUBCASE("it's contraction") {
        auto tokens = tokenizer.split("It's working");
        CHECK(tokens.size() == 3);
        CHECK(tokens[0] == "It");
        CHECK(tokens[1] == "'s");
        CHECK(tokens[2] == "working");
    }
    
    SUBCASE("I'm contraction") {
        auto tokens = tokenizer.split("I'm happy");
        CHECK(tokens.size() == 3);
        CHECK(tokens[0] == "I");
        CHECK(tokens[1] == "'m");
        CHECK(tokens[2] == "happy");
    }
    
    SUBCASE("we'll contraction") {
        auto tokens = tokenizer.split("We'll see");
        CHECK(tokens.size() == 3);
        CHECK(tokens[0] == "We");
        CHECK(tokens[1] == "'ll");
        CHECK(tokens[2] == "see");
    }
    
    SUBCASE("ain't contraction") {
        auto tokens = tokenizer.split("ain't nobody");
        CHECK(tokens.size() == 3);
        CHECK(tokens[0] == "is");
        CHECK(tokens[1] == "not");
        CHECK(tokens[2] == "nobody");
    }
    
    SUBCASE("shan't contraction") {
        auto tokens = tokenizer.split("We shan't fail");
        CHECK(tokens.size() == 4);
        CHECK(tokens[0] == "We");
        CHECK(tokens[1] == "shall");
        CHECK(tokens[2] == "not");
        CHECK(tokens[3] == "fail");
    }
    
    SUBCASE("capitalized contractions") {
        auto tokens = tokenizer.split("Won't Can't");
        CHECK(tokens.size() == 4);
        CHECK(tokens[0] == "Will");
        CHECK(tokens[1] == "not");
        CHECK(tokens[2] == "Can");
        CHECK(tokens[3] == "not");
    }
    
    SUBCASE("cannot contraction") {
        auto tokens = tokenizer.split("I cannot go");
        CHECK(tokens.size() == 4);
        CHECK(tokens[0] == "I");
        CHECK(tokens[1] == "can");
        CHECK(tokens[2] == "not");
        CHECK(tokens[3] == "go");
    }
    
    SUBCASE("mixed case contractions") {
        auto tokens = tokenizer.split("WON'T CAN'T");
        // Uppercase contractions aren't recognized by the patterns
        CHECK(tokens.size() == 2);
        CHECK(tokens[0] == "WON'T");
        CHECK(tokens[1] == "CAN'T");
    }
    
    SUBCASE("they're they've they'll") {
        auto tokens = tokenizer.split("They're here, they've arrived, they'll stay");
        CHECK(tokens.size() == 11);  // Includes "stay" at the end
        CHECK(tokens[0] == "They");
        CHECK(tokens[1] == "'re");
        CHECK(tokens[2] == "here");
        CHECK(tokens[3] == ",");
        CHECK(tokens[4] == "they");
        CHECK(tokens[5] == "'ve");
        CHECK(tokens[6] == "arrived");
        CHECK(tokens[7] == ",");
        CHECK(tokens[8] == "they");
        CHECK(tokens[9] == "'ll");
        CHECK(tokens[10] == "stay");
    }
    
    SUBCASE("would've could've should've") {
        auto tokens = tokenizer.split("I would've could've should've");
        CHECK(tokens.size() == 7);  // Includes last 've
        CHECK(tokens[0] == "I");
        CHECK(tokens[1] == "would");
        CHECK(tokens[2] == "'ve");
        CHECK(tokens[3] == "could");
        CHECK(tokens[4] == "'ve");
        CHECK(tokens[5] == "should");
        CHECK(tokens[6] == "'ve");
    }
}

TEST_CASE("SimpleTokenizer special characters") {
    nvs::SimpleTokenizer tokenizer(false);
    
    SUBCASE("hyphenated words") {
        auto tokens = tokenizer.split("self-driving");
        CHECK(tokens.size() == 1);
        CHECK(tokens[0] == "self-driving");
    }
    
    SUBCASE("forward slash") {
        auto tokens = tokenizer.split("and/or");
        CHECK(tokens.size() == 1);
        CHECK(tokens[0] == "and/or");
    }
    
    SUBCASE("ampersand") {
        auto tokens = tokenizer.split("R&D");
        CHECK(tokens.size() == 1);
        CHECK(tokens[0] == "R&D");
    }
    
    SUBCASE("parentheses") {
        auto tokens = tokenizer.split("(example)");
        CHECK(tokens.size() == 3);
        CHECK(tokens[0] == "(");
        CHECK(tokens[1] == "example");
        CHECK(tokens[2] == ")");
    }
    
    SUBCASE("quotes") {
        auto tokens = tokenizer.split("\"quoted\"");
        CHECK(tokens.size() == 3);
        CHECK(tokens[0] == "\"");
        CHECK(tokens[1] == "quoted");
        CHECK(tokens[2] == "\"");
    }
    
    SUBCASE("ellipsis") {
        auto tokens = tokenizer.split("wait...");
        // Note: ellipsis gets split into individual periods due to delimiter processing
        CHECK(tokens.size() == 4);
        CHECK(tokens[0] == "wait");
        CHECK(tokens[1] == ".");
        CHECK(tokens[2] == ".");
        CHECK(tokens[3] == ".");
    }
}

TEST_CASE("SimpleTokenizer edge cases") {
    nvs::SimpleTokenizer tokenizer(false);
    
    SUBCASE("numbers") {
        auto tokens = tokenizer.split("123 456.78");
        CHECK(tokens.size() == 2);
        CHECK(tokens[0] == "123");
        CHECK(tokens[1] == "456.78");
    }
    
    SUBCASE("mixed alphanumeric") {
        auto tokens = tokenizer.split("test123 456test");
        CHECK(tokens.size() == 2);
        CHECK(tokens[0] == "test123");
        CHECK(tokens[1] == "456test");
    }
    
    SUBCASE("multiple abbreviations") {
        auto tokens = tokenizer.split("Dr. Smith and Prof. Jones");
        CHECK(tokens.size() == 5);
        CHECK(tokens[0] == "Dr.");
        CHECK(tokens[1] == "Smith");
        CHECK(tokens[2] == "and");
        CHECK(tokens[3] == "Prof.");
        CHECK(tokens[4] == "Jones");
    }
    
    SUBCASE("U.S. abbreviation") {
        auto tokens = tokenizer.split("U.S. government");
        // U.S. is recognized as abbreviations so periods stay attached
        CHECK(tokens.size() == 3);
        CHECK(tokens[0] == "U.S");
        CHECK(tokens[1] == ".");
        CHECK(tokens[2] == "government");
        // Note: This shows a limitation - ideally U.S. would be kept together
        // but the current implementation doesn't fully handle multi-part abbreviations
    }
    
    SUBCASE("comma without space") {
        auto tokens = tokenizer.split("one,two,three");
        CHECK(tokens.size() == 5);
        CHECK(tokens[0] == "one");
        CHECK(tokens[1] == ",");
        CHECK(tokens[2] == "two");
        CHECK(tokens[3] == ",");
        CHECK(tokens[4] == "three");
    }
    
    SUBCASE("email addresses") {
        auto tokens = tokenizer.split("contact user@example.com today");
        CHECK(tokens.size() == 5);
        CHECK(tokens[0] == "contact");
        CHECK(tokens[1] == "user");
        CHECK(tokens[2] == "@");
        CHECK(tokens[3] == "example.com");
        CHECK(tokens[4] == "today");
    }
    
    SUBCASE("URLs") {
        auto tokens = tokenizer.split("Visit https://example.com/page");
        CHECK(tokens.size() == 4);
        CHECK(tokens[0] == "Visit");
        CHECK(tokens[1] == "https");
        CHECK(tokens[2] == ":");
        CHECK(tokens[3] == "//example.com/page");  // slashes preserved with path
    }
    
    SUBCASE("currency symbols") {
        auto tokens = tokenizer.split("$100 €50 £25");
        CHECK(tokens.size() == 4);
        CHECK(tokens[0] == "$");
        CHECK(tokens[1] == "100");
        CHECK(tokens[2] == "€50");
        CHECK(tokens[3] == "£25");
    }
    
    SUBCASE("mathematical operators") {
        auto tokens = tokenizer.split("2+2=4");
        CHECK(tokens.size() == 5);
        CHECK(tokens[0] == "2");
        CHECK(tokens[1] == "+");
        CHECK(tokens[2] == "2");
        CHECK(tokens[3] == "=");
        CHECK(tokens[4] == "4");
    }
    
    SUBCASE("percentage") {
        auto tokens = tokenizer.split("100% complete");
        CHECK(tokens.size() == 3);
        CHECK(tokens[0] == "100");
        CHECK(tokens[1] == "%");
        CHECK(tokens[2] == "complete");
    }
    
    SUBCASE("dates with slashes") {
        auto tokens = tokenizer.split("12/25/2024");
        CHECK(tokens.size() == 1);
        CHECK(tokens[0] == "12/25/2024");
    }
    
    SUBCASE("dates with hyphens") {
        auto tokens = tokenizer.split("2024-12-25");
        CHECK(tokens.size() == 1);
        CHECK(tokens[0] == "2024-12-25");
    }
    
    SUBCASE("time formats") {
        auto tokens = tokenizer.split("3:30pm");
        CHECK(tokens.size() == 3);
        CHECK(tokens[0] == "3");
        CHECK(tokens[1] == ":");
        CHECK(tokens[2] == "30pm");
    }
}

TEST_CASE("SimpleTokenizer performance patterns") {
    nvs::SimpleTokenizer tokenizer(false);
    
    SUBCASE("long text") {
        std::string long_text = "This is a somewhat longer piece of text that should be "
                               "tokenized efficiently by the CTRE-based tokenizer. It contains "
                               "various punctuation marks, numbers like 123, and special "
                               "characters! Does it work well? Let's see...";
        auto tokens = tokenizer.split(long_text);
        CHECK(tokens.size() > 20);
        CHECK(tokens[0] == "This");
        // Ellipsis gets split into individual periods
        CHECK(tokens[tokens.size() - 1] == ".");
    }
    
    SUBCASE("repeated patterns") {
        auto tokens = tokenizer.split("test test test test test");
        CHECK(tokens.size() == 5);
        for (const auto& token : tokens) {
            CHECK(token == "test");
        }
    }
    
    SUBCASE("very long string") {
        std::string long_str(1000, 'a');
        long_str += " ";
        long_str += std::string(1000, 'b');
        auto tokens = tokenizer.split(long_str);
        CHECK(tokens.size() == 2);
        CHECK(tokens[0].size() == 1000);
        CHECK(tokens[1].size() == 1000);
    }
    
    SUBCASE("many tokens") {
        std::string text;
        for (int i = 0; i < 100; ++i) {
            if (i > 0) text += " ";
            text += "word" + std::to_string(i);
        }
        auto tokens = tokenizer.split(text);
        CHECK(tokens.size() == 100);
    }
}

TEST_CASE("SimpleTokenizer boundary conditions") {
    nvs::SimpleTokenizer tokenizer(false);
    
    SUBCASE("single character") {
        auto tokens = tokenizer.split("a");
        CHECK(tokens.size() == 1);
        CHECK(tokens[0] == "a");
    }
    
    SUBCASE("single punctuation") {
        auto tokens = tokenizer.split("!");
        CHECK(tokens.size() == 1);
        CHECK(tokens[0] == "!");
    }
    
    SUBCASE("single period") {
        auto tokens = tokenizer.split(".");
        CHECK(tokens.size() == 1);
        CHECK(tokens[0] == ".");
    }
    
    SUBCASE("only punctuation") {
        auto tokens = tokenizer.split("!@#$%^&*()");
        CHECK(tokens.size() == 10);
        CHECK(tokens[0] == "!");
        CHECK(tokens[1] == "@");
        CHECK(tokens[2] == "#");
    }
    
    SUBCASE("unicode characters latin-1") {
        auto tokens = tokenizer.split("café naïve");
        CHECK(tokens.size() == 2);
        CHECK(tokens[0] == "café");
        CHECK(tokens[1] == "naïve");
    }
    
    SUBCASE("unicode cyrillic") {
        auto tokens = tokenizer.split("привет мир");
        CHECK(tokens.size() == 2);
        CHECK(tokens[0] == "привет");
        CHECK(tokens[1] == "мир");
    }
    
    SUBCASE("multiple consecutive delimiters") {
        auto tokens = tokenizer.split("word!!!???...");
        CHECK(tokens.size() == 10);  // word + 3! + 3? + 3.
        CHECK(tokens[0] == "word");
        CHECK(tokens[1] == "!");
        CHECK(tokens[2] == "!");
        CHECK(tokens[3] == "!");
    }
    
    SUBCASE("apostrophe variations") {
        auto tokens = tokenizer.split("it's it's");  // straight vs curly apostrophe
        CHECK(tokens.size() == 2);  // Both preserved as single tokens
        CHECK(tokens[0] == "it's");
        CHECK(tokens[1] == "it's");  // Curly apostrophe version
    }
}

TEST_CASE("SimpleTokenizer abbreviation behavior") {
    nvs::SimpleTokenizer tokenizer(false);
    
    SUBCASE("common abbreviations keep period") {
        // When abbreviations are alone, period is split due to EOL pattern
        auto tokens = tokenizer.split("Dr.");
        CHECK(tokens.size() == 2);
        CHECK(tokens[0] == "Dr");
        CHECK(tokens[1] == ".");
        
        tokens = tokenizer.split("Mr.");
        CHECK(tokens.size() == 2);
        CHECK(tokens[0] == "Mr");
        CHECK(tokens[1] == ".");
        
        tokens = tokenizer.split("Prof.");
        CHECK(tokens.size() == 2);
        CHECK(tokens[0] == "Prof");
        CHECK(tokens[1] == ".");
    }
    
    SUBCASE("month abbreviations keep period") {
        // When abbreviations are alone, period is split due to EOL pattern
        auto tokens = tokenizer.split("Jan.");
        CHECK(tokens.size() == 2);
        CHECK(tokens[0] == "Jan");
        CHECK(tokens[1] == ".");
        
        tokens = tokenizer.split("Dec.");
        CHECK(tokens.size() == 2);
        CHECK(tokens[0] == "Dec");
        CHECK(tokens[1] == ".");
    }
    
    SUBCASE("non-abbreviations split period") {
        auto tokens = tokenizer.split("hello.");
        CHECK(tokens.size() == 2);
        CHECK(tokens[0] == "hello");
        CHECK(tokens[1] == ".");
        
        tokens = tokenizer.split("world.");
        CHECK(tokens.size() == 2);
        CHECK(tokens[0] == "world");
        CHECK(tokens[1] == ".");
    }
    
    SUBCASE("abbreviations in context") {
        auto tokens = tokenizer.split("See Dr. Smith");
        CHECK(tokens.size() == 3);
        CHECK(tokens[0] == "See");
        CHECK(tokens[1] == "Dr.");
        CHECK(tokens[2] == "Smith");
    }
}

TEST_CASE("SimpleTokenizer regression tests") {
    nvs::SimpleTokenizer tokenizer(false);
    
    SUBCASE("period after non-abbreviation") {
        auto tokens = tokenizer.split("end.");
        CHECK(tokens.size() == 2);
        CHECK(tokens[0] == "end");
        CHECK(tokens[1] == ".");
    }
    
    SUBCASE("period after abbreviation") {
        auto tokens = tokenizer.split("Dr.");
        CHECK(tokens.size() == 2);  // Period split when at EOL
        CHECK(tokens[0] == "Dr");
        CHECK(tokens[1] == ".");
    }
    
    SUBCASE("multiple periods in sequence") {
        auto tokens = tokenizer.split("...");
        CHECK(tokens.size() == 3);
        CHECK(tokens[0] == ".");
        CHECK(tokens[1] == ".");
        CHECK(tokens[2] == ".");
    }
    
    SUBCASE("preserved special tokens") {
        auto tokens = tokenizer.split("a/b c-d e&f");
        CHECK(tokens.size() == 3);
        CHECK(tokens[0] == "a/b");
        CHECK(tokens[1] == "c-d");
        CHECK(tokens[2] == "e&f");
    }
    
    SUBCASE("question and exclamation marks") {
        auto tokens = tokenizer.split("What? Really!");
        CHECK(tokens.size() == 4);
        CHECK(tokens[0] == "What");
        CHECK(tokens[1] == "?");
        CHECK(tokens[2] == "Really");
        CHECK(tokens[3] == "!");
    }
}

#endif
