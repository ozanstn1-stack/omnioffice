//! Rule-based Turkish hyphenation.
//!
//! Turkish syllabification is deterministic: every syllable has exactly one
//! vowel, and the consonants between two vowels are split so that the last one
//! starts the next syllable (`kal-dı-rım`, `Türk-çe`). No dictionary is needed,
//! which is why this is a small rule instead of a hyphenation table.
//!
//! The function returns byte offsets of the allowed break points (a break goes
//! *after* the syllable that ends at the offset). Words shorter than four
//! characters, words without vowels and words with non-letters are left alone;
//! a break never leaves a single letter on either side.

/// Turkish vowels, upper and lower case.
fn is_vowel(ch: char) -> bool {
    matches!(ch, 'a' | 'e' | 'ı' | 'i' | 'o' | 'ö' | 'u' | 'ü' | 'A' | 'E' | 'I' | 'İ' | 'O' | 'Ö' | 'U' | 'Ü')
}

/// Byte offsets of every syllable boundary (after each syllable but the last).
fn syllable_boundaries(word: &str) -> Vec<usize> {
    let characters: Vec<char> = word.chars().collect();
    if characters.len() < 2 || !characters.iter().all(|ch| ch.is_alphabetic()) {
        return Vec::new();
    }
    let vowels: Vec<usize> =
        characters.iter().enumerate().filter_map(|(index, ch)| is_vowel(*ch).then_some(index)).collect();
    if vowels.len() < 2 {
        return Vec::new();
    }
    let mut byte_offsets: Vec<usize> = Vec::with_capacity(characters.len() + 1);
    let mut offset = 0usize;
    for ch in &characters {
        byte_offsets.push(offset);
        offset += ch.len_utf8();
    }
    byte_offsets.push(offset);

    let mut boundaries = Vec::new();
    for pair in vowels.windows(2) {
        let (first, second) = (pair[0], pair[1]);
        let consonants = second - first - 1;
        // The last consonant before a vowel starts the next syllable; two
        // adjacent vowels break between them.
        let boundary = if consonants == 0 { second } else { second - 1 };
        boundaries.push(byte_offsets[boundary]);
    }
    boundaries.dedup();
    boundaries
}

/// Byte offsets where `word` may be broken for line breaking: the syllable
/// boundaries, minus any that would leave a single letter on a side.
///
/// Empty when the word is not hyphenatable: too short, no vowels, contains a
/// non-letter, or every boundary leaves a single letter.
pub fn break_offsets(word: &str) -> Vec<usize> {
    syllable_boundaries(word)
        .into_iter()
        .filter(|offset| word[..*offset].chars().count() >= 2 && word[*offset..].chars().count() >= 2)
        .collect()
}

/// The syllables of `word`, or the word itself when it is not hyphenatable.
pub fn syllables(word: &str) -> Vec<String> {
    let boundaries = syllable_boundaries(word);
    if boundaries.is_empty() {
        return vec![word.to_string()];
    }
    let mut out = Vec::with_capacity(boundaries.len() + 1);
    let mut start = 0usize;
    for offset in boundaries {
        out.push(word[start..offset].to_string());
        start = offset;
    }
    out.push(word[start..].to_string());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hand-checked against the Turkish syllable rule (TDK): the last consonant
    /// between two vowels starts the next syllable.
    #[test]
    fn hand_checked_words_break_at_the_right_syllables() {
        let cases = [
            ("İstanbul", "İs-tan-bul"),
            ("güneş", "gü-neş"),
            ("Türkiye", "Tür-ki-ye"),
            ("araba", "a-ra-ba"),
            ("kaldırım", "kal-dı-rım"),
            ("Türkçe", "Türk-çe"),
            ("kitap", "ki-tap"),
            ("saat", "sa-at"),
            ("merhaba", "mer-ha-ba"),
            ("öğretmen", "öğ-ret-men"),
            ("bilgisayar", "bil-gi-sa-yar"),
            ("yapıldı", "ya-pıl-dı"),
            ("kırlangıç", "kır-lan-gıç"),
            ("pencereden", "pen-ce-re-den"),
            ("gözlükçü", "göz-lük-çü"),
            ("özgürlük", "öz-gür-lük"),
        ];
        for (word, expected) in cases {
            assert_eq!(syllables(word).join("-"), expected, "{word}");
        }
    }

    #[test]
    fn short_or_unbreakable_words_are_left_alone() {
        for word in ["ev", "su", "ki", "ay", "tren", "kral"] {
            assert!(break_offsets(word).is_empty(), "{word}");
            assert_eq!(syllables(word), vec![word.to_string()]);
        }
        assert!(break_offsets("12.34").is_empty());
        assert!(break_offsets("Türkiye'de").is_empty(), "an apostrophe is not a letter");
        assert!(break_offsets("").is_empty());
    }

    #[test]
    fn breaks_never_leave_a_single_letter() {
        for word in ["araba", "kaldırım", "gözlükçü", "bilgisayar"] {
            for offset in break_offsets(word) {
                assert!(word[..offset].chars().count() >= 2, "{word} at {offset}");
                assert!(word[offset..].chars().count() >= 2, "{word} at {offset}");
            }
        }
    }

    #[test]
    fn the_break_view_drops_single_letter_syllables() {
        // The true syllables keep the one-letter syllable; a line break must
        // not end with it.
        assert_eq!(syllables("araba").join("-"), "a-ra-ba");
        assert_eq!(break_offsets("araba"), vec![3]);
    }

    /// Regression fixture of 500 words: it locks the current output so a rule
    /// change is a visible diff. The hand-checked cases above are the semantic
    /// truth; this list is generated from the same rule.
    #[test]
    fn the_golden_list_matches() {
        let fixture = include_str!("../tests/fixtures/hyphen-tr-golden.txt");
        let mut checked = 0usize;
        for line in fixture.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((word, expected)) = line.split_once('\t') else {
                panic!("fixture line without a tab: {line}");
            };
            assert_eq!(syllables(word).join("-"), expected, "{word}");
            checked += 1;
        }
        assert_eq!(checked, 500, "the golden list must hold 500 words");
    }

    /// Regenerates the golden fixture from a plain word list (one word per
    /// line). Ignored by default; run it only when the rule intentionally
    /// changes:
    ///   `$env:HYPHEN_WORD_LIST=...; cargo test -p officecore --lib write_golden_fixture -- --ignored`
    #[test]
    #[ignore]
    fn write_golden_fixture() {
        let source = std::env::var("HYPHEN_WORD_LIST").expect("HYPHEN_WORD_LIST");
        let text = std::fs::read_to_string(source).expect("word list");
        let mut words: Vec<&str> = text
            .lines()
            .map(str::trim)
            .filter(|word| word.chars().count() >= 5 && word.chars().all(|c| c.is_alphabetic()))
            .collect();
        words.sort_unstable();
        words.dedup();
        let mut out = String::from("# 500 Turkish words with their syllables, generated from the rule\n");
        for word in words.into_iter().take(500) {
            out.push_str(&format!("{word}\t{}\n", syllables(word).join("-")));
        }
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/hyphen-tr-golden.txt");
        std::fs::write(path, out).expect("write fixture");
    }
}
