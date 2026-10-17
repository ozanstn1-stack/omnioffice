# Turkish spell checking

`crates/turkish` is the offline Turkish spell checker behind the Writer's
red-underlining and suggestions. It needs no network and no external service.

## Data and license

| File | Source | License |
| --- | --- | --- |
| `crates/turkish/data/master-dictionary.dict` | [zemberek-nlp](https://github.com/ahmetaa/zemberek-nlp) (upstream commit `037202c7e18b977c95181a76db05cb1e95846554`) | Apache-2.0 |
| `crates/turkish/data/first-10K` | same repository | Apache-2.0 |

Provenance, SHA-256 checksums and the license notice live in
`crates/turkish/data/NOTICE.md`; the full license text is vendored at
`licenses/APACHE-2.0.txt` (Apache-2.0 requires the copy to travel with the
data). The code itself stays under the repository's MIT license.

## How it works

1. **Lexicon** (`src/lexicon.rs`): the master dictionary (about 29k roots;
   verbs as `-mak`/`-mek` infinitives) plus the `first-10K` frequency list for
   suggestion ranking. Homonym lines are merged: inverse harmony holds when any
   sense has it, `NoVoicing` only when every sense has it.
2. **Suffix analysis** (`src/suffix.rs`): strips up to five suffix layers with
   vowel harmony (two-way and four-way), consonant assimilation (`kitapda` is
   rejected, `kitapta` accepted), consonant softening (`kitabı` -> `kitap`),
   vowel drop (`oğluma` -> `oğul`) and the `-ki`/`-kü` exception. A small
   irregular map covers `ye`/`de` (`yiyorum`, `diyorum`).
3. **Suggestions** (`src/suggest.rs`): a curated list of classic mistakes
   (including split words like `herşey` -> `her şey`) first, then bounded
   Damerau-Levenshtein over the lexicon ranked by edit distance and frequency.
4. **Text checking** (`src/lib.rs`): tokens are checked lowercase; capitalized
   words (proper nouns), numbers and all-caps acronyms are accepted. Words on
   the curated mistake list are always flagged.

## Accuracy contract

The acceptance tests in `crates/turkish/tests/spell_test.rs` pin the plan's
budget:

* at least 90% of the classic misspelling list is flagged (the curated list
  takes it to 100% of the tested cases),
* fewer than 2% false positives on a clean Turkish text,
* valid inflections (five-layer chains like `evlerimizdekiler`) are accepted,
* assimilation and harmony violations (`yapdı`, `evlar`, `kitapda`) are
  rejected.

Known trade-offs, deliberately chosen to keep false positives low: capitalized
misspellings are not flagged (proper-noun heuristic), and a few wrong forms
that are morphologically parseable (`kitapı` instead of `kitabı`) pass.

## Running the tests

```powershell
cargo test -p turkish
```
