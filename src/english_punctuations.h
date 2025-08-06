#pragma once
#include <unordered_set>
#include <string>

class EnglishPunctuations {
public:
    /// Returns the singleton instance
    static const EnglishPunctuations& getInstance() {
        static EnglishPunctuations instance;  // Thread-safe in C++11+
        return instance;
    }

    /// Test membership
    bool contains(const std::string& mark) const noexcept {
        return dict_.find(mark) != dict_.end();
    }

    /// Number of punctuation marks
    std::size_t size() const noexcept {
        return dict_.size();
    }

    /// Iteration support
    auto begin() const noexcept { return dict_.begin(); }
    auto end()   const noexcept { return dict_.end();   }

private:
    // Private ctor builds the fixed set
    EnglishPunctuations()
      : dict_{
          "[", "]", "(", ")", "{", "}", "<", ">", ":",
          ",", ";", "-", "--", "---", "!", "?", ".",
          "...", "`", "'", "\"", "/"
        }
    {}

    // Non-copyable, non-movable
    EnglishPunctuations(const EnglishPunctuations&) = delete;
    EnglishPunctuations& operator=(const EnglishPunctuations&) = delete;

    const std::unordered_set<std::string> dict_;
};