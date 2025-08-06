// SimpleSentenceSplitter.h
#pragma once

#include <string>
#include <vector>
#include <regex>
#include <algorithm>
#include <cctype>

// You’ll need C++ ports of these:
//   • EnglishAbbreviations::contains(const std::string&)
//   • EnglishDictionary::instance().count(const std::string&)
#include "english_abbreviations.h"
#include "english_dictionary.h"

class SimpleSentenceSplitter {
public:
    /// Singleton accessor
    static SimpleSentenceSplitter& getInstance() {
        static SimpleSentenceSplitter instance;
        return instance;
    }

    /// Split text into sentences.
    std::vector<std::string> split(const std::string& input) {
        std::vector<std::string> sentences;
        int len = 0;
        std::string text = input;

        // 1) Normalize carriage returns to spaces
        text = std::regex_replace(text, regexCarriageReturn(), " ");

        // 2) Clear any stray 0x19 markers
        for (char& c : text) if (c == '\x19') c = ' ';

        // 3) Insert 0x19 where a space was likely forgotten after .!? 
        text = std::regex_replace(text, regexForgottenSpace(), "$1$2\x19$3");

        // 4) Add a newline so regex can match the final sentence
        text.push_back('\n');

        auto begin = text.cbegin();
        std::smatch m;
        std::string current;

        // 5) Loop over sentence-boundary matches
        while (std::regex_search(begin, text.cend(), m, regexSentence())) {
            // Extract groups
            std::string sent = m[1].str();
            std::string punct = m[2].str();

            // Determine which “after” group matched, and compute its end offset
            std::string after;
            size_t offsetBase = begin - text.cbegin();
            size_t newEnd;
            if (m[3].matched) {
                after = m[3].str();
                newEnd = m.position(3) + m.length(3) + offsetBase;
            }
            else if (m[5].matched) {
                after = m[5].str();
                newEnd = m.position(5) + m.length(5) + offsetBase;
            }
            else {
                after.clear();
                newEnd = m.position(0) + m.length(0) + offsetBase;
            }

            // Count words in 'sent'
            len += countWords(sent);

            std::string nextWord = m[4].matched ? m[4].str() : "";

            // Decide if this is a true break
            bool isBreak = false;
            if (punct == ".") {
                if (!isAbbreviation(sent, nextWord, len)) isBreak = true;
            }
            else if (punct == "!" || punct == "?" || (punct == ":" && len > 6)) {
                isBreak = true;
            }

            // Append appropriately
            if (isBreak) {
                appendSentence(sentences, current, sent, punct, after);
                len = 0;
            } else {
                appendContinuation(current, sent, punct, after);
            }

            // Move search cursor forward
            begin = text.cbegin() + newEnd;
        }

        // Capture any trailing text
        size_t consumed = begin - text.cbegin();
        if (consumed < text.size()) {
            current += text.substr(consumed);
        }
        if (!current.empty()) {
            sentences.push_back(cleanOutput(current));
        }

        return sentences;
    }

private:
    SimpleSentenceSplitter() = default;
    SimpleSentenceSplitter(const SimpleSentenceSplitter&) = delete;
    SimpleSentenceSplitter& operator=(const SimpleSentenceSplitter&) = delete;

    // Regex factories (thread‐safe init)
    static const std::regex& regexCarriageReturn() {
        static const std::regex r{"[\\n\\r]+"};
        return r;
    }
    static const std::regex& regexForgottenSpace() {
        static const std::regex r{"(.)([\\.!?])([^0-9\\s\\.\"'`\\)\\}\\]])"};
        return r;
    }
    static const std::regex& regexSentence() {
        static const std::regex r{
            R"((['\"`]*[\(\{\[]?[A-Za-z0-9]+.*?)([\.!\?:])"
            R"(?:(?=([\(\[\{\"'`<>]*[ \x19]+)[\(\[\{\"'`\)\}\] ]*([A-Z0-9][a-z]*))"
            R"(|(?=([\(\)\"'`<\}\] \x19]+)\s)))"
        };
        return r;
    }
    static const std::regex& regexWhitespace() {
        static const std::regex r{"\\s+"};
        return r;
    }
    static const std::regex& regexLastWord() {
        static const std::regex r{"\\b([\\w0-9\\.']+)$"};
        return r;
    }

    // Helpers
    static size_t countWords(const std::string& s) {
        return std::distance(
            std::sregex_token_iterator(s.begin(), s.end(), regexWhitespace(), -1),
            std::sregex_token_iterator{}
        );
    }

    static std::string extractLastWord(const std::string& s) {
        std::smatch m2;
        if (std::regex_search(s, m2, regexLastWord())) return m2[1].str();
        return "";
    }

    static bool isAbbreviation(const std::string& sentence,
                               const std::string& nextWord,
                               int wordCount) 
    {
        std::string last = extractLastWord(sentence);
        // Check vowel presence, letter patterns, single-letter
        static const std::regex hasVowel{"[AEIOUaeiou]"};
        static const std::regex hasLower{"[a-z]"};
        static const std::regex hasY{"y"};
        static const std::regex letterDot{"([A-Za-z]\\.)+"};

        bool cond1 = !std::regex_search(last, hasVowel)
                     && std::regex_search(last, hasLower)
                     && !std::regex_search(last, hasY);
        bool cond2 = std::regex_match(last, letterDot);
        bool cond3 = (last.size()==1 && std::isalpha(last[0]) && last!="I");
        bool cond4 = EnglishAbbreviations::contains(toLower(last));

        if (cond1||cond2||cond3||cond4) {
            if (EnglishDictionary::instance().count(nextWord) && wordCount>6) {
                return false; // actually a sentence break
            }
            return true; // abbreviation = no break
        }
        return false;
    }

    static std::string toLower(const std::string& s) {
        std::string out; out.reserve(s.size());
        for (char c: s) out.push_back(std::tolower((unsigned char)c));
        return out;
    }

    static void appendSentence(std::vector<std::string>& v,
                               std::string& curr,
                               const std::string& sent,
                               const std::string& punct,
                               const std::string& after)
    {
        curr += sent + punct + after;
        v.push_back(cleanOutput(curr));
        curr.clear();
    }

    static void appendContinuation(std::string& curr,
                                   const std::string& sent,
                                   const std::string& punct,
                                   const std::string& after)
    {
        curr += sent + punct;
        if (after.find('\x19')==std::string::npos) curr.push_back(' ');
    }

    static std::string cleanOutput(const std::string& s) {
        // Remove markers and trim whitespace
        std::string tmp;
        tmp.reserve(s.size());
        for (char c: s) if (c!='\x19') tmp.push_back(c);
        // Trim
        auto ws = " \t\n\r";
        auto start = tmp.find_first_not_of(ws);
        if (start==std::string::npos) return "";
        auto end = tmp.find_last_not_of(ws);
        return tmp.substr(start, end-start+1);
    }
};

