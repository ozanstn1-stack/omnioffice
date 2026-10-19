# Lexicon data notice

The Turkish root lexicon in this directory is **not** original to OmniOffice:

| File | Source | License |
| --- | --- | --- |
| `master-dictionary.dict` | [zemberek-nlp](https://github.com/ahmetaa/zemberek-nlp) `morphology/src/main/resources/tr/master-dictionary.dict` | Apache License 2.0 |
| `first-10K` | [zemberek-nlp](https://github.com/ahmetaa/zemberek-nlp) `morphology/src/main/resources/tr/first-10K` | Apache License 2.0 |
| `proper-from-corpus.dict` | [zemberek-nlp](https://github.com/ahmetaa/zemberek-nlp) `morphology/src/main/resources/tr/proper-from-corpus.dict` | Apache License 2.0 |
| `proper-tr.txt` | Original to OmniOffice (provinces, countries, languages, frequent given names) | MIT (this repository) |

The vendored zemberek lists only carry *derived* proper forms
("Ankaragücü", "İstanbulboğazı"); the plain names live in their binary
lexicon, which this project does not load, so `proper-tr.txt` supplies them.

Retrieved from the `master` branch on 2026-10-17, upstream commit
`037202c7e18b977c95181a76db05cb1e95846554`.

SHA-256 of the vendored files:

```
c539e12904d1ccac4792073f05deeae2babe692249dc4a359da254f1bd338f6f  master-dictionary.dict
0264f2f84424c160cc3e85578092011040038b782c8f6df92d53c1359f32b414  first-10K
694b5f92b0c794e885d32686a0f2b9dff519de9f78289f2c3fff7274ff812874  proper-from-corpus.dict
```

Copyright 2018 Ahmet A. Akın, Mehmet D. Akın — licensed under the Apache
License, Version 2.0 (the "License"); you may not use these files except in
compliance with the License. A copy of the License is in the repository root
(`licenses/APACHE-2.0.txt`) and at
<http://www.apache.org/licenses/LICENSE-2.0>.

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the
License for the specific language governing permissions and limitations under
the License.
