/**
 * Case- and diacritic-insensitive matching for Turkish text.
 *
 * `toLowerCase()` alone misses the Turkish pairs: "Şişli" has to be found by
 * "sisli", "SİSLİ", "sısli" and "ŞİŞLİ". The four i/I/ı/İ letters fold to one
 * character and the cedilla/breve/diaeresis letters fold to their ASCII base.
 * The mapping is 1:1 (one input character -> one output character) so callers
 * can keep using `includes` on the folded strings.
 */
const FOLD: Record<string, string> = {
  İ: "i",
  I: "i",
  ı: "i",
  i: "i",
  Ş: "s",
  ş: "s",
  Ğ: "g",
  ğ: "g",
  Ç: "c",
  ç: "c",
  Ö: "o",
  ö: "o",
  Ü: "u",
  ü: "u",
};

/** Folds `text` for search: lower case plus the Turkish letter equivalences. */
export function foldSearch(text: string): string {
  let out = "";
  for (const ch of text) out += FOLD[ch] ?? ch.toLowerCase();
  return out;
}

/** True when `haystack` contains `needle` after folding both. */
export function searchIncludes(haystack: string, needle: string): boolean {
  return foldSearch(haystack).includes(foldSearch(needle));
}
