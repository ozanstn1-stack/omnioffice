//! Suggestions for a misspelled word: a curated list of classic mistakes and
//! split-word errors first, then bounded Damerau-Levenshtein over the lexicon
//! ranked by edit distance and `first-10K` frequency.

use crate::casing::lower;
use crate::lexicon::Lexicon;

/// Classic mistakes the edit-distance engine would miss or rank badly,
/// including the split-word errors Turkish is famous for. Keys are single
/// tokens; corrections may contain a space.
const COMMON_MISTAKES: &[(&str, &str)] = &[
    ("herşey", "her şey"),
    ("birşey", "bir şey"),
    ("hiçbirşey", "hiçbir şey"),
    ("heryer", "her yer"),
    ("herkez", "herkes"),
    ("yanlız", "yalnız"),
    ("yanlızca", "yalnızca"),
    ("yalnış", "yanlış"),
    ("klavuz", "kılavuz"),
    ("şöför", "şoför"),
    ("şarz", "şarj"),
    ("orjinal", "orijinal"),
    ("labaratuvar", "laboratuvar"),
    ("üniverste", "üniversite"),
    ("proğram", "program"),
    ("acenta", "acente"),
    ("ambülans", "ambulans"),
    ("egzos", "egzoz"),
    ("ayrıyeten", "ayrıca"),
    ("ayriyeten", "ayrıca"),
    ("faliyet", "faaliyet"),
    ("hoşgeldiniz", "hoş geldiniz"),
    ("hoşbulduk", "hoş bulduk"),
    ("ilkönce", "ilk önce"),
    ("iyiki", "iyi ki"),
    ("malesef", "maalesef"),
    ("yada", "ya da"),
    ("makina", "makine"),
    ("döküman", "doküman"),
    ("eylence", "eğlence"),
    ("itibariyle", "itibarıyla"),
];

/// True when `word` is on the curated list of classic mistakes; those are
/// always flagged even when suffix analysis could "explain" them (`yada` is
/// `ya` + `da` morphologically, but it is still wrong).
pub fn is_known_mistake(word: &str) -> bool {
    COMMON_MISTAKES.iter().any(|(wrong, _)| *wrong == word)
}

/// Suggestions for `word`, best first, at most `limit`.
pub fn suggest(lexicon: &Lexicon, word: &str, limit: usize) -> Vec<String> {
    let query = lower(word);
    let mut results: Vec<(u32, u32, String)> = Vec::new();
    for (wrong, right) in COMMON_MISTAKES {
        if query == *wrong {
            results.push((0, 0, (*right).to_string()));
        }
    }
    for candidate in lexicon.words() {
        let length_gap = (candidate.chars().count() as i32 - query.chars().count() as i32).abs();
        if length_gap > 2 {
            continue;
        }
        let distance = damerau_within(&query, candidate, 2);
        if distance <= 2 {
            results.push((distance, lexicon.rank(candidate).unwrap_or(u32::MAX / 2), candidate.clone()));
        }
    }
    results.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    results.dedup_by(|a, b| a.2 == b.2);
    results.into_iter().take(limit).map(|(_, _, word)| word).collect()
}

/// Damerau-Levenshtein distance with a cap: returns `limit + 1` when the
/// distance exceeds `limit`, which lets the caller skip expensive rows.
fn damerau_within(left: &str, right: &str, limit: u32) -> u32 {
    let left: Vec<char> = left.chars().collect();
    let right: Vec<char> = right.chars().collect();
    let rows = left.len() + 1;
    let columns = right.len() + 1;
    if (rows as i32 - columns as i32).unsigned_abs() > limit {
        return limit + 1;
    }
    let mut previous_previous: Vec<u32> = vec![0; columns];
    let mut previous: Vec<u32> = (0..columns as u32).collect();
    let mut current: Vec<u32> = vec![0; columns];
    for i in 1..rows {
        current[0] = i as u32;
        let mut row_minimum = current[0];
        for j in 1..columns {
            let substitution = previous[j - 1] + u32::from(left[i - 1] != right[j - 1]);
            let insertion = current[j - 1] + 1;
            let deletion = previous[j] + 1;
            let mut value = substitution.min(insertion).min(deletion);
            if i > 1 && j > 1 && left[i - 1] == right[j - 2] && left[i - 2] == right[j - 1] {
                value = value.min(previous_previous[j - 2] + 1);
            }
            current[j] = value;
            row_minimum = row_minimum.min(value);
        }
        if row_minimum > limit {
            return limit + 1;
        }
        std::mem::swap(&mut previous_previous, &mut previous);
        std::mem::swap(&mut previous, &mut current);
    }
    previous[columns - 1]
}
