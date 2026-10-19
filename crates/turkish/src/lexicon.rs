//! Root lexicon: the vendored Zemberek master dictionary plus the `first-10K`
//! frequency list (see `data/NOTICE.md` for source and license).

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

/// Morphophonology flags that change how a root takes suffixes.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Attributes {
    /// The final consonant does not soften (`flebit` + `i` -> `flebite` is
    /// wrong; `flebiti` is right).
    pub no_voicing: bool,
    /// Foreign roots with inverse vowel harmony (`saat` + `i` -> `saati`).
    pub inverse_harmony: bool,
}

pub struct Lexicon {
    words: HashSet<String>,
    /// Capitalized proper nouns from the corpus list (`proper-from-corpus.dict`).
    proper: HashSet<String>,
    attributes: HashMap<String, Attributes>,
    frequency: HashMap<String, u32>,
}

const MASTER: &str = include_str!("../data/master-dictionary.dict");
const FIRST_10K: &str = include_str!("../data/first-10K");
const PROPER: &str = include_str!("../data/proper-from-corpus.dict");
const PROPER_TR: &str = include_str!("../data/proper-tr.txt");

impl Lexicon {
    /// The embedded lexicon, parsed once per process.
    pub fn embedded() -> &'static Lexicon {
        static LEXICON: OnceLock<Lexicon> = OnceLock::new();
        LEXICON.get_or_init(|| Lexicon::from_data(MASTER, FIRST_10K, PROPER, PROPER_TR))
    }

    /// Parses the master dictionary (`word [P:POS; A:Attrs]` per line), the
    /// frequency list (one word per line, most frequent first) and the
    /// capitalized proper-noun lists (the vendored corpus list plus this
    /// project's curated one, `data/proper-tr.txt`).
    ///
    /// Homonym lines (`kalp [A:Voicing, InverseHarmony]`, `kalp [P:Adj;
    /// A:NoVoicing]`, ...) are merged: inverse harmony holds when *any* sense
    /// has it, and `NoVoicing` only when *every* sense has it.
    pub fn from_data(master: &str, frequency: &str, proper: &str, proper_tr: &str) -> Lexicon {
        let mut words = HashSet::new();
        let mut sense_counts: HashMap<String, (u32, u32, u32)> = HashMap::new();
        for line in master.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let (word, marker) = match line.find(" [") {
                Some(index) => (&line[..index], &line[index..]),
                None => (line, ""),
            };
            let word = word.trim();
            // Punctuation entries and multi-word lemmas do not belong in a
            // word checker.
            if word.is_empty() || word.contains(' ') || marker.contains("P:Punc") {
                continue;
            }
            words.insert(word.to_string());
            let entry = sense_counts.entry(word.to_string()).or_insert((0, 0, 0));
            entry.0 += 1;
            if marker.contains("NoVoicing") {
                entry.1 += 1;
            }
            if marker.contains("InverseHarmony") {
                entry.2 += 1;
            }
        }
        let mut attributes = HashMap::new();
        for (word, (senses, no_voicing, inverse)) in sense_counts {
            let parsed = Attributes { no_voicing: no_voicing == senses, inverse_harmony: inverse > 0 };
            if parsed != Attributes::default() {
                attributes.insert(word, parsed);
            }
        }
        let mut frequency_rank = HashMap::new();
        for (rank, line) in frequency.lines().enumerate() {
            let word = line.trim();
            if !word.is_empty() {
                frequency_rank.entry(word.to_string()).or_insert(rank as u32);
            }
        }
        let mut proper_words = HashSet::new();
        for source in [proper, proper_tr] {
            for line in source.lines() {
                let word = line.trim();
                if word.is_empty() || word.starts_with('#') || word.contains(' ') {
                    continue;
                }
                proper_words.insert(word.to_string());
            }
        }
        Lexicon { words, proper: proper_words, attributes, frequency: frequency_rank }
    }

    pub fn contains(&self, word: &str) -> bool {
        self.words.contains(word)
    }

    /// True when the capitalized form is a known proper noun.
    pub fn contains_proper(&self, word: &str) -> bool {
        self.proper.contains(word)
    }

    pub fn attributes(&self, word: &str) -> Attributes {
        self.attributes.get(word).copied().unwrap_or_default()
    }

    /// Frequency rank (0 = most frequent) from the `first-10K` list.
    pub fn rank(&self, word: &str) -> Option<u32> {
        self.frequency.get(word).copied()
    }

    pub fn words(&self) -> impl Iterator<Item = &String> {
        self.words.iter()
    }

    pub fn len(&self) -> usize {
        self.words.len()
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }
}
