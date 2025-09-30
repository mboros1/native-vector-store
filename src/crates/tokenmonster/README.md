# tokenmonster

Greedy tiktoken-like tokenizer with an embedded vocabulary, intended for fast, allocation-light tokenization.

Features

- Greedy tokenization compatible with common LLM vocabularies
- Zero-copy where possible; minimal allocations
- Optional tiny test vocabulary via the `tiny_vocab` feature

License: MIT

