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

/// A misspelled word with its UTF-16 code-unit range in the original text.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Misspelling {
    pub word: String,
    pub start: usize,
    pub end: usize,
}

/// True when `word` is a lexicon root or a valid inflected form of one.
///
/// Capitalized words are accepted (proper nouns), as are numbers and
/// all-caps acronyms.
pub fn check_word(word: &str) -> bool {
    let lexicon = Lexicon::embedded();
    let core = word.split('\'').next().unwrap_or(word);
    if core.is_empty() {
        return true;
    }
    let characters: Vec<char> = core.chars().collect();
    if characters.len() < 2 || characters.iter().any(|c| c.is_ascii_digit()) {
        return true;
    }
    if characters.iter().all(|c| !c.is_lowercase()) || casing::starts_uppercase(core) {
        return true;
    }
    let lowered = casing::lower(core);
    if suggest::is_known_mistake(&lowered) {
        return false;
    }
    lexicon.contains(&lowered) || suffix::analyze(lexicon, &lowered)
}

/// Misspelled words in `text` (lowercase tokens only, see `check_word`).
pub fn check_text(text: &str) -> Vec<Misspelling> {
    let lexicon = Lexicon::embedded();
    let mut out = Vec::new();
    let mut token_start: Option<usize> = None;
    for (index, character) in text.char_indices() {
        if character.is_alphabetic() || character == '\'' {
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
    let core = token.split('\'').next().unwrap_or(token);
    if core.is_empty() {
        return;
    }
    let characters: Vec<char> = core.chars().collect();
    if characters.len() < 2 || characters.iter().any(|c| c.is_ascii_digit()) {
        return;
    }
    if characters.iter().all(|c| !c.is_lowercase()) || casing::starts_uppercase(core) {
        return;
    }
    let lowered = casing::lower(core);
    let start_utf16 = text[..start].encode_utf16().count();
    let end_utf16 = start_utf16 + core.encode_utf16().count();
    if suggest::is_known_mistake(&lowered) {
        out.push(Misspelling { word: core.to_string(), start: start_utf16, end: end_utf16 });
        return;
    }
    if lexicon.contains(&lowered) || suffix::analyze(lexicon, &lowered) {
        return;
    }
    out.push(Misspelling { word: core.to_string(), start: start_utf16, end: end_utf16 });
}

/// Suggestions for `word`, best first, at most `limit`.
pub fn suggestions(word: &str, limit: usize) -> Vec<String> {
    suggest::suggest(Lexicon::embedded(), word, limit)
}
