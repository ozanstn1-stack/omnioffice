//! Offline Turkish spell checking.
//!
//! * root lexicon: the vendored Zemberek master dictionary plus the `first-10K`
//!   frequency list (Apache-2.0, see `data/NOTICE.md`),
//! * suffix analysis with vowel harmony, consonant assimilation and consonant
//!   softening (`crates/turkish/src/suffix.rs`),
//! * suggestions: curated classic mistakes plus bounded Damerau-Levenshtein
//!   over the lexicon ranked by frequency.
//!
//! No network, no external service: everything runs from the embedded data.
//!
//! ```no_run
//! use turkish::{check_word, check_text, suggestions};
//!
//! assert!(check_word("evlerimizdekiler"));
//! assert!(!check_word("yanlız"));
//! let text = "Bu yanlız bir cümle.";
//! let misspellings = check_text(text);
//! assert_eq!(misspellings.len(), 1);
//! assert_eq!(misspellings[0].word, "yanlız");
//! assert!(suggestions("yanlız", 3).contains(&"yalnız".to_string()));
//! ```

pub mod casing;
pub mod lexicon;
pub mod suffix;
pub mod suggest;

pub use lexicon::Lexicon;

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// A misspelled word with its UTF-16 code-unit range in the original text.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Misspelling {
    pub word: String,
    pub start: usize,
    pub end: usize,
}

/// Cuts a token at an apostrophe (`Ankara'da`, `Ankara’da`): only the part
/// before it is the word; the rest is a suffix.
fn split_at_apostrophe(word: &str) -> &str {
    word.split(['\'', '\u{2019}']).next().unwrap_or(word)
}

/// Memoized [`suffix::analyze`] result: the Writer checks the whole document,
/// so the same words are asked over and over. Bounded; the cache resets when
/// it grows past the limit.
fn cached_word_ok(word: &str, compute: impl FnOnce() -> bool) -> bool {
    const LIMIT: usize = 50_000;
    static CACHE: OnceLock<Mutex<HashMap<String, bool>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(guard) = cache.lock() {
        if let Some(value) = guard.get(word) {
            return *value;
        }
    }
    let value = compute();
    if let Ok(mut guard) = cache.lock() {
        if guard.len() > LIMIT {
            guard.clear();
        }
        guard.insert(word.to_string(), value);
    }
    value
}

/// True when `word` is a lexicon root or a valid inflected form of one.
///
/// All-caps acronyms are accepted; a capitalized word passes when it is a
/// known proper noun (the corpus list) or a valid lowercase word, so a
/// sentence-initial misspelling like `Yanlız` is flagged.
pub fn check_word(word: &str) -> bool {
    let lexicon = Lexicon::embedded();
    let core = split_at_apostrophe(word);
    if core.is_empty() {
        return true;
    }
    let characters: Vec<char> = core.chars().collect();
    if characters.len() < 2 || characters.iter().any(|c| c.is_ascii_digit()) {
        return true;
    }
    if characters.iter().all(|c| !c.is_lowercase()) {
        return true;
    }
    // Mixed case after the first letter is a brand/acronym ("OmniOffice",
    // "iPhone"), not a Turkish word; never flag those.
    if characters[1..].iter().any(|c| c.is_uppercase()) {
        return true;
    }
    let lowered = casing::lower(core);
    if suggest::is_known_mistake(&lowered) {
        return false;
    }
    if casing::starts_uppercase(core) && lexicon.contains_proper(core) {
        return true;
    }
    cached_word_ok(&lowered, || lexicon.contains(&lowered) || suffix::analyze(lexicon, &lowered))
}

/// Misspelled words in `text` (UTF-16 offsets; see [`Misspelling`]).
pub fn check_text(text: &str) -> Vec<Misspelling> {
    let lexicon = Lexicon::embedded();
    let mut out = Vec::new();
    let mut token_start: Option<usize> = None;
    for (index, character) in text.char_indices() {
        if character.is_alphabetic() || character == '\'' || character == '\u{2019}' {
            if token_start.is_none() {
                token_start = Some(index);
            }
        } else if let Some(start) = token_start.take() {
            consider(text, start, index, lexicon, &mut out);
        }
    }
    if let Some(start) = token_start {
        consider(text, start, text.len(), lexicon, &mut out);
    }
    out
}

fn consider(text: &str, start: usize, end: usize, lexicon: &Lexicon, out: &mut Vec<Misspelling>) {
    let token = &text[start..end];
    let core = split_at_apostrophe(token);
    if core.is_empty() {
        return;
    }
    let characters: Vec<char> = core.chars().collect();
    if characters.len() < 2 || characters.iter().any(|c| c.is_ascii_digit()) {
        return;
    }
    if characters.iter().all(|c| !c.is_lowercase()) {
        return;
    }
    if characters[1..].iter().any(|c| c.is_uppercase()) {
        return;
    }
    let lowered = casing::lower(core);
    let flagged = if suggest::is_known_mistake(&lowered) {
        true
    } else if casing::starts_uppercase(core) && lexicon.contains_proper(core) {
        false
    } else {
        !cached_word_ok(&lowered, || lexicon.contains(&lowered) || suffix::analyze(lexicon, &lowered))
    };
    if flagged {
        let start_utf16 = text[..start].encode_utf16().count();
        let end_utf16 = start_utf16 + core.encode_utf16().count();
        out.push(Misspelling { word: core.to_string(), start: start_utf16, end: end_utf16 });
    }
}

/// Suggestions for `word`, best first, at most `limit`.
pub fn suggestions(word: &str, limit: usize) -> Vec<String> {
    suggest::suggest(Lexicon::embedded(), word, limit)
}
