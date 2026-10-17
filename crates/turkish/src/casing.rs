//! Turkish-aware casing: `İ`/`i` and `I`/`ı` are distinct letters, and Rust's
//! default `to_lowercase` maps `İ` to `i` + combining dot, which would break
//! every lexicon lookup.

/// Lowercases a word with Turkish rules (`İ` -> `i`, `I` -> `ı`).
pub fn lower(word: &str) -> String {
    let mut out = String::with_capacity(word.len());
    for character in word.chars() {
        match character {
            'İ' => out.push('i'),
            'I' => out.push('ı'),
            other => out.extend(other.to_lowercase()),
        }
    }
    out
}

/// True when the first character is an uppercase Turkish letter.
pub fn starts_uppercase(word: &str) -> bool {
    word.chars().next().map(|c| c.is_uppercase()).unwrap_or(false)
}

/// True for the Turkish vowels, including the dotted/dotless pairs.
pub fn is_vowel(character: char) -> bool {
    matches!(character, 'a' | 'e' | 'ı' | 'i' | 'o' | 'ö' | 'u' | 'ü')
}

/// The last vowel of a word, if it has one.
pub fn last_vowel(word: &str) -> Option<char> {
    word.chars().rev().find(|c| is_vowel(*c))
}

/// Front vowels (`e`, `i`, `ö`, `ü`): they pick the `e`/`i` suffix variants.
pub fn is_front(vowel: char) -> bool {
    matches!(vowel, 'e' | 'i' | 'ö' | 'ü')
}

/// Rounded vowels (`o`, `ö`, `u`, `ü`): they pick the `u`/`ü` variants.
pub fn is_rounded(vowel: char) -> bool {
    matches!(vowel, 'o' | 'ö' | 'u' | 'ü')
}

/// Unvoiced (hard) consonants: suffixes starting with `d`/`c` turn into
/// `t`/`ç` after them (`kitap` + `da` -> `kitapta`).
pub fn is_unvoiced(character: char) -> bool {
    matches!(character, 'p' | 'ç' | 't' | 'k' | 'f' | 'h' | 's' | 'ş')
}
