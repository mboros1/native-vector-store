# Code Review: simple_tokenizer.cpp

**Date**: 2025-08-28T22:10:04.686Z
**Model**: GPT-5
**Configuration**: quick (Quick review focusing on critical issues)
**File**: /Users/martinboros/git/native-vector-store/src/simple_tokenizer.cpp


---

Comprehensive C++ Code Review: simple_tokenizer.cpp

Summary
- Overall: Solid, readable tokenizer with simple regex-driven rules and inline tests. However, it has correctness issues in contraction replacements, heavy reliance on std::regex with performance costs, limited Unicode handling, and some maintainability and design limitations.
- Primary risks: incorrect contraction expansions ("won't", "shan't", "ain't"), regex complexity/latency, locale/Unicode behavior, incomplete abbreviation handling, and delimiter edge cases.

1) Correctness (Rating: 6/10)
Issues
- Lines ~9-23: Contraction replacements are incorrect.
  - WONT_CONTRACTION uses "$1ill not" for "\\b(w)(on't)\\b". That expands "won't" to "will not" only if "$1" (w) + "ill not" yields "will not", which is intended. But case-insensitive flag breaks proper casing (e.g., "Won't" => "Will not" should preserve capitalization more carefully).
  - SHANT_CONTRACTION replacement "$1ll not" for "\\b(sha)(n't)\\b" produces "shall not" only if "$1" is "sha" → "shall not": correct textually, but casing suffers; also inconsistent pattern naming and replacement form.
  - AINT_CONTRACTION replacement "$1m not" for "\\b(a)(in't)\\b" yields "am not": "a" + "m not". This is only correct if the joined result is "am not", but it produces "am not" with a space as intended. However, see below.
  - NOT_CONTRACTIONS: "\\b(can)('t|not)\\b" → "$1 not" will turn "cannot" into "can not" (arguably acceptable), but "can't" expands to "can not", fine. The second rule "(.)(n't)\\b" with "$1 not" will expand "won't" after the earlier transforms? Potential double application or inconsistent results when combined with earlier special-cases.
  - CONTRACTIONS2 "(.)('ll|'re|'ve|'s|'m|'d)\\b" → "$1 $2" splits tokens but leaves leading apostrophes on $2 (e.g., "he's" → "he 's" which your tests suggest is desired for possessives in "John's book" but your test did not require "'s" explicitly; it only checked "John" and "book").
- Lines ~35-43: Delimiter rules:
  - DELIMITERS[0] "([^\\w\\.\\'\\-\\/,&])": relies on \\w meaning [A-Za-z0-9_], which is ASCII-only under the default C locale. It will split on accented letters (Unicode), contradicting the test’s allowance for Unicode behavior but likely undesirable. Tests accept lenient result but functionality is limited.
  - DELIMITERS[1] "(,\\s)" replaced with " $1" preserves comma + space as a single token when later split — but then whitespace split will split " $1" into "," token or ", " combined with space? Because replacement is " $1" and then whitespace split, it leaves "," attached to no word: OK. But it misses commas not followed by whitespace.
  - DELIMITERS[3] "\\. *(\\n|$)" replaced with " . " forces period tokenization only at line end; intra-sentence periods are not processed identically. This can leave "U.S.A" unaffected (good), but a trailing "." not at EOL may be inconsistently handled.
- Lines ~51-62: sregex_token_iterator with WHITESPACE splits fine; but repeated regex replaces create complex intermediate strings which may cause unexpected sequences (double spaces, etc.).
- Lines ~64-71: Abbreviation handling only merges a trailing "." with the previous token for a single-case scenario. Fails for tokens like "U.S." (multiple dots) or abbreviations not in set. Also tokenization earlier might leave "." tokens from DELIMITERS[3] only at EOL; abbreviations mid-sentence not handled.
- isAbbreviation (lines ~76-81): Small fixed set; misses many abbreviations; case-sensitive; "U.S.A" without periods; inconsistent with tests expecting "Dr." possibly.
- Regex patterns with "(.)" capture only one character; this can split incorrectly for multi-letter prefixes or cause false positives at word boundaries (e.g., emoji/Unicode).
- Potential double-processing order sensitivity: The sequence of contraction replacements can interact and produce unexpected tokens if input matches multiple rules.
- No handling of multithreading but static regex objects are const; safe.

Improvements
- Correct contraction replacements with explicit whole-word patterns and precise expansions, preserving case.
  Example:
  - For won’t:
    Pattern: "(?i)\\bwon’t\\b|\\bwon't\\b"
    Replace: "will not" with case-preserving function (see below).
  - For shan’t: "\\bshan’t\\b|\\bshan't\\b" → "shall not"
  - For ain’t: "\\bain’t\\b|\\bain't\\b" → "am not" only in appropriate person/context? Simpler: "is not"/"are not" ambiguous; keep "ain't" → "is not" may be too opinionated; consider leaving as "ain't" unless you truly need normalization.
- Avoid fragile "(.)" captures; use word groups:
  - From: std::regex("(.)('ll|'re|'ve|'s|'m|'d)\\b", icase)
  - To: std::regex("\\b([A-Za-z]+)('ll|'re|'ve|'s|'m|'d)\\b", icase)
- Make replacements using a function to preserve capitalization:
  - Use std::regex_replace with a callback is not supported in std::regex; instead, manual scan or use Boost.Regex/RE2, or run a find loop with std::regex_search and build output preserving case of the first letter.
- Normalize delimiter handling:
  - Handle commas without requiring following whitespace: "(,)" and ensure both leading/trailing spacing " $1 ".
  - For periods, avoid splitting decimals:
    - Use negative lookbehind/lookahead is unsupported in std::regex (C++11/17), so patterning will be limited; better: a manual scan state machine for numbers, URLs, emails, ellipses, punctuation.
- Abbreviations:
  - Expand abbreviation detection: use a larger set and case-insensitive matching; include forms with trailing dot(s). Consider dynamic rule: if token is 1-3 letters capitalized followed by ".", treat as abbreviation.
  - Handle multiple trailing "." (ellipsis) vs abbreviation dot.
- Unicode:
  - If Unicode desired, std::regex with default locales won’t cut it. Consider ICU or utf8proc and custom scanning, or at least document limitation.

Code Sketch: safer possessive split
- Replace "(.)('s)\\b" with "\\b([A-Za-z]+)('s)\\b" and tokenization to "word" and "'s" if desired.

2) Performance (Rating: 4/10)
Issues
- Many std::regex_replace passes over the entire string (up to 1 + 5 + contractions passes). Each pass is O(n) with heavy backtracking; total O(k·n) with a large constant k; can be very slow on large text (catastrophic for worst-case regex).
- std::regex is notoriously slow and varies by implementation; some patterns like "(.)(n't)\\b" may backtrack.
- Allocations: Each replace creates new strings; multiple reallocations and copies.
- sregex_token_iterator also involves regex engine overhead.
- For large_text test (10k words), performance will be significantly degraded vs a single-pass tokenizer.

Improvements
- Replace most regexes with a single-pass deterministic scanner:
  - Iterate over UTF-8 bytes, classify chars, build tokens, handle punctuation, ellipses, numbers, contractions rules in code.
  - This reduces passes to O(n), minimizes allocations (reserve expected token count or reuse buffer).
- If regex must be used:
  - Combine delimiter replacements into fewer passes where possible.
  - Use pre-reserved string and token vector: tokens.reserve(input.size() / 4) heuristic.
  - Consider std::string_view for tokens if you keep a modified buffer.
- SIMD: Potential for classification using lookup tables and SIMD (e.g., simdjson-style classification) if needed.
- Avoid constructing temporary strings in contraction loops; but you already use static regex; good.

3) Maintainability (Rating: 7/10)
What’s good
- Static const regex members keep patterns centralized.
- Reasonably clear structure; inline tests are helpful.
- Names are mostly self-explanatory.

Issues
- Magic indices into DELIMITERS (0..4) reduce readability; risks mismatch between array and usage.
- Inconsistent naming: WONT_CONTRACTION vs SHANT_CONTRACTION; "SHAnt" vs "shan't". Some replacements cryptic ("$1ill not").
- isAbbreviation small, hardcoded, case-sensitive; not documented.
- No documentation comments explaining tokenization policy (e.g., hyphens kept, slashes kept, quotes handling).
- Tests and implementation expectations occasionally diverge (e.g., quotes).

Improvements
- Replace vector-based DELIMITERS with named constants or struct with fields accessed by name; or a single array but reference by named constexpr indices.
  Example:
    enum class DelimRule { NonWord, CommaSpace, ApostropheSpace, PeriodEOL, Ellipsis, Count };
- Add docstrings to split() and policy comments per rule.
- Refactor contraction handling into dedicated functions/modules with tests per rule.
- Use consistent casing-preservation policy and document.

4) Design (Rating: 6/10)
What’s good
- Simple API: split(const std::string&).
- Optional contraction splitting via constructor flag (assumed from tests).
- Self-contained tokenizer with static regex resources.

Issues
- API rigidity: no way to configure delimiter rules, abbreviation list, Unicode handling, or token policy.
- split copies input string immediately; could operate on a copy only when needed, or mutate a buffer.
- Uses std::string; no overloads for string_view.
- Mixed concerns: normalization (expanding contractions) and tokenization intertwined; better layered design.

Improvements
- API:
  - Add overload: std::vector<std::string> split(std::string_view input).
  - Provide configuration struct (e.g., TokenizerConfig) for flags: splitContractions, keepHyphens, treatApostrophePossessive, abbreviationSet, unicodeMode.
  - Expose isAbbreviation as configurable predicate.
- Separate normalization and tokenization stages for clarity and testability.
- Consider returning std::vector<std::string_view> over a stable buffer for performance.

5) Security (Rating: 7/10)
What’s good
- No raw pointers; uses std::string and STL safely.
- No direct input trust assumed; no I/O or system calls.

Issues
- Regex DoS potential on adversarial inputs due to multiple std::regex_replace passes (especially with nested quotes/apostrophes causing backtracking).
- Integer growth is safe here, but very large inputs could cause memory pressure due to repeated allocations.
- Unicode handling might split in the middle of multibyte sequences if using byte-wise assumptions in regex with \\w; but std::regex works on code units and won’t break memory safety—just correctness.

Improvements
- Limit maximum input length or make behavior configurable.
- Replace std::regex with linear-time scanner to avoid backtracking pitfalls.
- Avoid patterns like "(.)(n't)" which can be greedy over arbitrary characters; tighten classes.

6) Compliance and Portability (Rating: 8/10)
What’s good
- Uses standard C++ features; no UB evident.
- Static const regexs are fine; header presumably declares them.

Issues
- std::regex performance and feature support varies across libstdc++/libc++/MSVC; some engines historically buggy or slow.
- Reliance on \\w with default C locale: not portable for Unicode word definitions; results vary with locales if imbued.
- Case-insensitive flag with non-ASCII may behave inconsistently.

Improvements
- If portability/perf is critical, consider RE2 or Boost.Regex (faster) or custom scanner.
- Document ASCII-only semantics; if Unicode required, integrate ICU and treat UTF-8 properly.
- Provide CMake option to switch regex backend.

Specific Line-Item Notes and Fix Suggestions
- Lines 12-22: Contraction replacements
  Problem: Case and correctness.
  Fix approach: Replace with explicit word patterns and deterministic outputs, with simple casing preservation for first letter.
  Example:
    static const std::regex WONT("\\b(?i:won't)\\b");
    text = std::regex_replace(text, WONT, "will not");
  Then post-process capitalization when original starts uppercase:
    auto preserve_case = [](std::string_view src, std::string repl) {
      if (!src.empty() && std::isupper(static_cast<unsigned char>(src.front())))
        repl[0] = static_cast<char>(std::toupper(static_cast<unsigned char>(repl[0])));
      return repl;
    };
  Because std::regex_replace doesn’t pass match to callback, switch to manual search-loop:
    std::string out;
    out.reserve(text.size()*11/10);
    for (std::cmatch m; std::regex_search(p, end, m, WONT); ...) // build out preserving case

- Lines 24-33: DELIMITERS replacements
  Problem: Order-dependent; incomplete punctuation coverage.
  Fix: Make all punctuation tokens surrounded by spaces unless part of known constructs (ellipsis, decimals, URLs).
  If keeping regex, adjust:
    const std::regex NON_WORD("([^A-Za-z0-9\\.\\'\\-\\/,&%$])");
    text = std::regex_replace(text, NON_WORD, " $1 ");
    const std::regex COMMA("(,)");
    text = std::regex_replace(text, COMMA, " $1 ");
    const std::regex ELLIPSIS("(\\.{3,})");
    text = std::regex_replace(text, ELLIPSIS, " $1 ");
  Then handle periods at EOL and single periods: use a pass to separate trailing period not part of number or acronym.

- Lines 37-44: sregex_token_iterator with WHITESPACE
  Improvement: tokens.reserve(text.size() / 5); to mitigate allocations.

- Lines 46-53: Abbreviation handling
  Problem: Only merges final "." with previous token; limited set.
  Fix: Generalize:
    - If tokens.size()>=2 and tokens.back()=="." and tokens[tokens.size()-2].size()<=3 and std::isupper(all letters), merge.
    - Handle multi-dot abbreviations by scanning backward for repeating ".".
  Add case-insensitive isAbbreviation and include "Dr", "St", "Prof", "Ms", "Sr", "Jr".

- Lines 76-81: isAbbreviation
  Fix: Make abbr lowercased and compare case-insensitively:
    std::string low; low.reserve(tok.size());
    for (char c: tok) low.push_back(std::tolower((unsigned char)c));
    return abbr.count(low) > 0;

- Regex correctness
  - CONTRACTIONS2 first pattern "(.)('ll...)" risks catching punctuation. Replace "." with "[A-Za-z]".
    std::regex("\\b([A-Za-z]+)('ll|'re|'ve|'s|'m|'d)\\b", icase)
  - NOT_CONTRACTIONS second pattern "(.)(n't)\\b" → "\\b([A-Za-z]+)(n't)\\b"

- Quotes handling (tests expect "\"" token)
  Current NON_WORD rule excludes double-quote from the allowed set, so it becomes a separate token; single quote handling is inconsistent. Clarify and align with tests.

Alternative Design Proposal (Optional)
- Build a single-pass UTF-8 aware scanner:
  - Classes: letter, digit, whitespace, punctuation.
  - Rules:
    - Collect words including internal hyphens and slashes between letters/digits.
    - Apostrophes: if between letters, treat as part; if "'s" at end, optional split when config enabled.
    - Numbers: digits with optional single '.' for decimals; optional leading '-' when preceded by whitespace or start; optional '%' as separate token.
    - Ellipsis: sequence of three or more '.' -> single "..." token.
    - Punctuation: each char as its own token.
    - Contractions: post-process words per dictionary/rules.
    - Abbreviations: heuristic merging with trailing dot(s).
  - Return std::vector<std::string_view> over a mutable buffer.

What’s Done Well
- Clean separation of test compilation with NVS_ENABLE_INLINE_TESTS.
- Good coverage of many edge cases in tests: punctuation, numbers, whitespace, long words, performance baseline, consistency.
- Static regex definitions prevent repeated compilation.
- Simple, easy-to-read code flow.

Quick Win Changes
- Tighten regex character classes from "." to [A-Za-z] in contraction patterns.
- Fix contraction replacements to not rely on "$1ill" style; replace with direct whole-word replacements for special cases:
  - won’t/won't → will not
  - shan’t/shan't → shall not
  - can’t/can't → can not
- Expand comma handling to "(,)" not "(,\\s)".
- Reserve tokens capacity.
- Case-insensitive abbreviation check with a larger set and heuristic merge for trailing periods.
- Document ASCII-focused behavior and limitations.

Example Patch Fragments
- Comma rule:
  // Before: std::regex("(,\\s)")
  const std::regex SimpleTokenizer::COMMA("(,)");
  text = std::regex_replace(text, COMMA, " $1 ");

- Contractions2 tightened:
  const std::vector<std::regex> SimpleTokenizer::CONTRACTIONS2 = {
    std::regex("\\b([A-Za-z]+)('ll|'re|'ve|'s|'m|'d)\\b", std::regex_constants::icase),
    std::regex("\\b(D)('ye)\\b", std::regex_constants::icase),
    ...
  };

- NOT_CONTRACTIONS tightened:
  const std::vector<std::regex> SimpleTokenizer::NOT_CONTRACTIONS = {
    std::regex("\\b(can)('t|not)\\b", std::regex_constants::icase),
    std::regex("\\b([A-Za-z]+)(n't)\\b", std::regex_constants::icase)
  };

- tokens reserve:
  std::vector<std::string> tokens;
  tokens.reserve(text.size() / 5);

- Abbreviation heuristic at end:
  if (tokens.size() > 1 && tokens.back() == ".") {
      auto& prev = tokens[tokens.size()-2];
      auto is_short_caps = prev.size() <= 3 && std::all_of(prev.begin(), prev.end(),
        [](char c){ return std::isalpha((unsigned char)c) && std::isupper((unsigned char)c); });
      if (is_short_caps || isAbbreviation(prev)) {
          prev += ".";
          tokens.pop_back();
      }
  }

Final Ratings
- Correctness: 6/10
- Performance: 4/10
- Maintainability: 7/10
- Design: 6/10
- Security: 7/10
- Compliance/Portability: 8/10

If performance and correctness across diverse text (including Unicode) are priorities, consider migrating from regex-driven transformations to a single-pass, table-driven tokenizer with explicit state handling and optional Unicode support.

---

*Generated by GPT-5 Code Review Script*
