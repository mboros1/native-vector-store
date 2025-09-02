#pragma once
#include <string>
#include <vector>
#include <unordered_set>
#include <algorithm>
#include "ctre-unicode.hpp"

namespace nvs {

/**
 * Tokenizer for general English-like text with pragmatic UTF-8 support.
 *
 * Semantics:
 * - Treats any non-ASCII UTF-8 codepoint (>= 0x80) and ASCII alphanumerics as
 *   word constituents. This keeps words in non‑ASCII scripts (Latin‑1, Cyrillic, etc.) intact.
 * - Permits a small set of in-word ASCII punctuation: apostrophe ('), hyphen (-), slash (/), and ampersand (&).
 *   This preserves common tokens like It's, self-driving, and/or, and R&D.
 * - Surrounds other delimiter characters with spaces prior to splitting, so punctuation like commas, quotes,
 *   parentheses, question and exclamation marks become stand-alone tokens.
 * - Ellipsis (three or more consecutive periods) is tokenized as individual '.' tokens.
 * - A lone period at end-of-line/text is split as '.'; internal periods remain in-word (e.g., 456.78).
 * - Contraction processing (splitContraction=true) is ASCII‑focused and keeps behavior compatible with existing tests.
 *
 * Ownership:
 * - split() returns a fresh std::vector<std::string>; there is no borrowing of the input buffer.
 * - No global state is retained; the tokenizer is stateless apart from the splitContraction_ flag.
 */
class SimpleTokenizer {
public:
    explicit SimpleTokenizer(bool splitContraction = false)
     : splitContraction_(splitContraction)
    {}

    std::vector<std::string> split(const std::string& input);

private:
    bool splitContraction_;

    /** Process ASCII contractions according to legacy patterns. */
    std::string process_contractions(std::string text) const;
    
    /**
     * Pre-normalize delimiters for tokenization. Performs a UTF‑8 aware pass that inserts
     * spaces around delimiter codepoints, preserves allowed in-word punctuation,
     * splits ellipsis into individual '.' tokens, and normalizes ASCII whitespace.
     */
    std::string process_delimiters(std::string text) const;

    /** very minimal abbreviation set; expand as needed */
    static bool isAbbreviation(const std::string& tok);
};

} // namespace nvs
