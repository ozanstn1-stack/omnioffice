/**
 * Rule-based Turkish hyphenation (TypeScript twin of
 * `crates/officecore/src/hyphen_tr.rs`).
 *
 * Every syllable has one vowel; the last consonant between two vowels starts
 * the next syllable (`kal-dı-rım`, `Türk-çe`). `syllablesTr` returns the true
 * syllables; `hyphenBreakOffsets` drops the boundaries that would leave a
 * single letter on a line.
 */
const VOWELS = "aeıioöuüAEIİOÖUÜ";

function isVowel(ch: string): boolean {
  return VOWELS.includes(ch);
}

/** Offsets (UTF-16) of every syllable boundary, after each syllable but the last. */
function syllableBoundaries(word: string): number[] {
  const characters = [...word];
  if (characters.length < 2 || !characters.every((ch) => /\p{L}/u.test(ch))) return [];
  const vowels = characters.map((ch, index) => (isVowel(ch) ? index : -1)).filter((index) => index >= 0);
  if (vowels.length < 2) return [];
  const boundaries: number[] = [];
  for (let index = 0; index + 1 < vowels.length; index += 1) {
    const first = vowels[index];
    const second = vowels[index + 1];
    const consonants = second - first - 1;
    boundaries.push(consonants === 0 ? second : second - 1);
  }
  return [...new Set(boundaries)];
}

/** Offsets where a line may break: syllable boundaries minus single-letter sides. */
export function hyphenBreakOffsets(word: string): number[] {
  const characters = [...word];
  return syllableBoundaries(word).filter((offset) => offset >= 2 && characters.length - offset >= 2);
}

/** The syllables of `word`, or the word itself when it is not hyphenatable. */
export function syllablesTr(word: string): string[] {
  const characters = [...word];
  const boundaries = syllableBoundaries(word);
  if (boundaries.length === 0) return [word];
  const out: string[] = [];
  let start = 0;
  for (const offset of boundaries) {
    out.push(characters.slice(start, offset).join(""));
    start = offset;
  }
  out.push(characters.slice(start).join(""));
  return out;
}
