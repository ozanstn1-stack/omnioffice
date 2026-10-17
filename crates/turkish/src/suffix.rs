//! Suffix analysis: strip Turkish inflectional and derivational suffixes with
//! vowel harmony, consonant assimilation and consonant softening, then look
//! the remaining root up in the lexicon.
//!
//! This is deliberately a *spell-checking* analyzer, not full morphology: it
//! answers "could this word be a valid inflected form?" and keeps false
//! positives low, at the cost of some precision on rare derivations.

use crate::casing::{is_front, is_rounded, is_unvoiced, is_vowel, last_vowel};
use crate::lexicon::Lexicon;

/// How many suffix layers a word may carry (`ev-imiz-de-ki-ler` is five).
const MAX_DEPTH: u32 = 5;

/// Suffix families; every variant is tried as a whole and `variant_allowed`
/// then enforces harmony and consonant assimilation.
const SUFFIXES: &[&[&str]] = &[
    &["ları", "leri"],
    &["lar", "ler"],
    &["ımız", "imiz", "umuz", "ümüz"],
    &["ınız", "iniz", "unuz", "ünüz"],
    &["sınız", "siniz", "sunuz", "sünüz"],
    &["ım", "im", "um", "üm"],
    &["yım", "yim", "yum", "yüm"],
    &["sı", "si", "su", "sü"],
    &["ına", "ine", "una", "üne"],
    &["ını", "ini", "unu", "ünü"],
    &["ıl", "il", "ul", "ül"],
    &["yı", "yi", "yu", "yü"],
    &["ı", "i", "u", "ü"],
    &["ya", "ye"],
    &["a", "e"],
    &["dan", "den", "tan", "ten"],
    &["da", "de", "ta", "te"],
    &["nın", "nin", "nun", "nün"],
    &["ın", "in", "un", "ün"],
    &["yla", "yle"],
    &["la", "le"],
    &["ki", "kü"],
    &["lı", "li", "lu", "lü"],
    &["sız", "siz", "suz", "süz"],
    &["lık", "lik", "luk", "lük"],
    &["cı", "ci", "cu", "cü", "çı", "çi", "çu", "çü"],
    &["ca", "ce"],
    &["madan", "meden"],
    &["maktan", "mekten"],
    &["yacak", "yecek"],
    &["yabil", "yebil"],
    &["abil", "ebil"],
    &["ıyor", "iyor", "uyor", "üyor"],
    &["yor"],
    &["acak", "ecek", "cak", "cek"],
    &["mış", "miş", "muş", "müş"],
    &["dı", "di", "du", "dü", "tı", "ti", "tu", "tü"],
    &["malı", "meli"],
    &["maz", "mez"],
    &["sa", "se"],
    &["ma", "me"],
    &["dır", "dir", "dur", "dür", "tır", "tir", "tur", "tür"],
    &["arak", "erek"],
    &["ınca", "ince"],
    &["ıp", "ip", "up", "üp"],
    &["an", "en"],
    &["yan", "yen"],
    &["ar", "er", "ır", "ir", "ur", "ür"],
    &["r"],
    &["sın", "sin", "sun", "sün"],
    &["ız", "iz", "uz", "üz"],
    &["m", "n", "k"],
];

/// Whether a suffix variant harmonizes with the stem it is stripped from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Harmony {
    /// The variant follows the regular vowel-harmony rules.
    Regular,
    /// The variant only fits an inverse-harmony root (`saat` + `i`).
    InverseOnly,
}

/// True when `word` is a valid inflected form of a lexicon root.
pub fn analyze(lexicon: &Lexicon, word: &str) -> bool {
    // Consonant assimilation is mandatory, and the fallback analyses (vowel
    // re-insertion) can accidentally "repair" a word that violates it:
    // `yapdı` must not be read as `yapıt`.
    if violates_assimilation(word) {
        return false;
    }
    analyze_depth(lexicon, word, MAX_DEPTH, false)
}

/// Endings whose first consonant must agree with the preceding consonant:
/// `d`/`c` never follow a hard consonant (`yapdı`, `kitapda`, `kitapcı`).
fn violates_assimilation(word: &str) -> bool {
    const SOFT: &[&str] = &[
        "dı", "di", "du", "dü", "da", "de", "dan", "den", "dır", "dir", "dur", "dür", "cı", "ci", "cu", "cü", "ca",
        "ce",
    ];
    for ending in SOFT {
        if let Some(stem) = word.strip_suffix(ending) {
            if let Some(last) = stem.chars().last() {
                if is_unvoiced(last) {
                    return true;
                }
            }
        }
    }
    false
}

fn analyze_depth(lexicon: &Lexicon, word: &str, depth: u32, inverse_only: bool) -> bool {
    if stem_ok(lexicon, word, inverse_only, depth < MAX_DEPTH) {
        return true;
    }
    if depth == 0 || word.chars().count() < 3 {
        return false;
    }
    for family in SUFFIXES {
        for variant in *family {
            let Some(stem) = word.strip_suffix(variant) else {
                continue;
            };
            if stem.chars().count() < 2 {
                continue;
            }
            let Some(harmony) = variant_allowed(stem, variant) else {
                continue;
            };
            let inverse = inverse_only || harmony == Harmony::InverseOnly;
            // The aorist `-r` is a closing suffix and only verbs take it: it
            // never stacks further suffixes, which keeps nonsense chains
            // (`her-yer` -> `her-ye-r`) from passing, and `bilgisayar-r` is
            // not a verb form.
            if *variant == "r" {
                if verb_ok(lexicon, stem, inverse) {
                    return true;
                }
                if let Some(unvoiced) = unvoiced_stem(lexicon, stem) {
                    if verb_ok(lexicon, &unvoiced, inverse) {
                        return true;
                    }
                }
                continue;
            }
            if analyze_depth(lexicon, stem, depth - 1, inverse) {
                return true;
            }
            // A softened final consonant only appears before a vowel-initial
            // suffix: kitab-ı -> kitap, ağac-ı -> ağaç, çocuğ-u -> çocuk.
            if variant.chars().next().map(is_vowel).unwrap_or(false) {
                if let Some(unvoiced) = unvoiced_stem(lexicon, stem) {
                    if analyze_depth(lexicon, &unvoiced, depth - 1, inverse) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// True when `stem` is a root: directly, as a verb lemma, after a dropped
/// vowel, or (when `inverse_only`) only with inverse harmony. `allow_reduce`
/// permits the doubled-consonant repair (`ett` -> `et`) and is only set once
/// at least one suffix has been stripped, so a plain `öğretmenn` stays wrong.
fn stem_ok(lexicon: &Lexicon, stem: &str, inverse_only: bool, allow_reduce: bool) -> bool {
    let accepts = |root: &str| lexicon.contains(root) && (!inverse_only || lexicon.attributes(root).inverse_harmony);
    if accepts(stem) {
        return true;
    }
    // Doubled consonants from assimilation: et-ti-m -> et, hisset-ti-m -> hisset.
    if allow_reduce {
        if let Some(reduced) = reduce_doubled(stem) {
            if accepts(&reduced) {
                return true;
            }
            if let Some(lemma) = verb_lemma(&reduced) {
                if accepts(&lemma) {
                    return true;
                }
            }
        }
    }
    if let Some(lemma) = verb_lemma(stem) {
        if accepts(&lemma) {
            return true;
        }
    }
    // Dropped vowel before a vowel-initial suffix: oğl-um -> oğul, ağz-ı -> ağız.
    for vowel in ['ı', 'i', 'u', 'ü'] {
        if let Some(restored) = insert_vowel(stem, vowel) {
            if lexicon.contains(&restored) {
                return true;
            }
        }
    }
    false
}

/// True when `stem` is a verb root (`oku` -> `okumak`).
fn verb_ok(lexicon: &Lexicon, stem: &str, inverse_only: bool) -> bool {
    let Some(lemma) = verb_lemma(stem) else {
        return false;
    };
    lexicon.contains(&lemma) && (!inverse_only || lexicon.attributes(&lemma).inverse_harmony)
}

/// The infinitive a verb stem would have (`git` -> `gitmek`), with the two
/// irregular verbs whose stem vowel narrows (`ye` -> `yi`, `de` -> `di`).
fn verb_lemma(stem: &str) -> Option<String> {
    if stem == "yi" {
        return Some("yemek".to_string());
    }
    if stem == "di" {
        return Some("demek".to_string());
    }
    let vowel = last_vowel(stem)?;
    Some(format!("{stem}{}", if is_front(vowel) { "mek" } else { "mak" }))
}

/// Re-inserts a dropped vowel before the final consonant (`oğl` -> `oğul`).
fn insert_vowel(stem: &str, vowel: char) -> Option<String> {
    let mut chars: Vec<char> = stem.chars().collect();
    let last = *chars.last()?;
    if is_vowel(last) {
        return None;
    }
    chars.insert(chars.len() - 1, vowel);
    Some(chars.into_iter().collect())
}

/// Reduces a doubled final consonant (`ett` -> `et`).
fn reduce_doubled(stem: &str) -> Option<String> {
    let mut chars: Vec<char> = stem.chars().collect();
    let last = *chars.last()?;
    if chars.len() < 2 || chars[chars.len() - 2] != last || is_vowel(last) {
        return None;
    }
    chars.pop();
    Some(chars.into_iter().collect())
}

/// The unsoftened form of a stem whose final consonant was voiced
/// (`kitab` -> `kitap`, `yapacağ` -> `yapacak`), unless the root is marked
/// `NoVoicing`. The caller keeps analyzing the result: the unsoftened form is
/// often itself an inflected form (`yapacak`).
fn unvoiced_stem(lexicon: &Lexicon, stem: &str) -> Option<String> {
    let mut chars: Vec<char> = stem.chars().collect();
    let replacement = match chars.last()? {
        'b' => 'p',
        'c' => 'ç',
        'd' => 't',
        'ğ' => 'k',
        _ => return None,
    };
    chars.pop();
    chars.push(replacement);
    let candidate: String = chars.into_iter().collect();
    if lexicon.attributes(&candidate).no_voicing {
        return None;
    }
    Some(candidate)
}

/// Harmony and consonant-assimilation rules for one variant.
fn variant_allowed(stem: &str, variant: &str) -> Option<Harmony> {
    let last = stem.chars().last()?;
    let first = variant.chars().next()?;
    // The `y` buffer and `yor` attach to vowel-final stems; everything else
    // that starts with a vowel follows a consonant.
    if variant.starts_with('y') {
        if !is_vowel(last) {
            return None;
        }
    } else if is_vowel(first) && is_vowel(last) {
        return None;
    }
    // Consonant assimilation: hard consonants turn `d`/`c` into `t`/`ç`.
    match first {
        'd' if is_unvoiced(last) => return None,
        't' if !is_unvoiced(last) => return None,
        'c' if is_unvoiced(last) => return None,
        'ç' if !is_unvoiced(last) => return None,
        _ => {}
    }
    // The one-letter person endings (`geldi-m`, `araba-m`) only follow a
    // vowel; `telefon-n` is not a form.
    if matches!(variant, "m" | "n" | "k") && !is_vowel(last) {
        return None;
    }
    // The `-ki`/`-kü` suffix is special: it keeps `i` after every unrounded
    // vowel (`akşamki`, `evdeki`) and `ü` after rounded ones (`bugünkü`).
    if matches!(variant, "ki" | "kü") {
        let stem_vowel = last_vowel(stem)?;
        let wants_rounded = matches!(variant, "kü");
        return Some(if wants_rounded == is_rounded(stem_vowel) { Harmony::Regular } else { Harmony::InverseOnly });
    }
    if !is_vowel(first) {
        // Consonant-initial suffixes still carry a vowel that must harmonize
        // (`ev` + `lar` is wrong: the `a` is back).
        let Some(variant_vowel) = variant.chars().find(|c| is_vowel(*c)) else {
            return Some(Harmony::Regular);
        };
        let stem_vowel = last_vowel(stem)?;
        return Some(harmony_of(stem_vowel, variant_vowel));
    }
    let stem_vowel = last_vowel(stem)?;
    Some(harmony_of(stem_vowel, first))
}

/// Vowel harmony between the stem's last vowel and the suffix's first vowel.
fn harmony_of(stem_vowel: char, suffix_vowel: char) -> Harmony {
    let regular = match suffix_vowel {
        'a' => !is_front(stem_vowel),
        'e' => is_front(stem_vowel),
        'ı' => !is_front(stem_vowel) && !is_rounded(stem_vowel),
        'i' => is_front(stem_vowel) && !is_rounded(stem_vowel),
        'u' => !is_front(stem_vowel) && is_rounded(stem_vowel),
        'ü' => is_front(stem_vowel) && is_rounded(stem_vowel),
        _ => true,
    };
    if regular {
        Harmony::Regular
    } else {
        Harmony::InverseOnly
    }
}
