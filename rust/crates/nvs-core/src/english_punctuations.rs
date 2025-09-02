use std::collections::HashSet;
use std::sync::OnceLock;

static PUNCTS: OnceLock<HashSet<&'static str>> = OnceLock::new();

fn build() -> HashSet<&'static str> {
    // Mirrors src/english_punctuations.h content
    HashSet::from([
        "[", "]", "(", ")", "{", "}", "<", ">", ":",
        ",", ";", "-", "--", "---", "!", "?", ".",
        "...", "`", "'", "\"", "/",
    ])
}

pub fn contains(mark: &str) -> bool {
    PUNCTS.get_or_init(build).contains(mark)
}

pub fn size() -> usize { PUNCTS.get_or_init(build).len() }

