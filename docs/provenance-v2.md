# Provenance v2 — DOCX manifests and the source chain (D7)

C8 put a source manifest inside generated PDFs. D7 carries the same idea to
**DOCX** and turns the record into a visible **derivation chain**: original
inputs → transformation step → output, with a digest check at every link that
the app can verify on its own.

Everything here is local: the manifest is written into the file, read back
without a network call, and no Google or cloud service is involved.

## What a manifest says

A manifest is a small JSON record embedded in the document:

| Field | Meaning |
|---|---|
| `formatVersion` | Manifest format written by this build (`1`). |
| `createdAt` | RFC 3339 UTC timestamp. |
| `appVersion` | OmniOffice version that produced the output. |
| `module` | Producer: `convert`, `merge`, `ocr`, or the manual label. |
| `note` | Free-text note the user may add. |
| `sources` | Each source's file name, size, SHA-256 and role. |
| `ai` | Provider and model when an AI step contributed, with citation count. |
| `documentSha256` | Digest of the document with the manifest stripped. |

## Where it lives

- **PDF** (C8): an embedded file attachment plus an XMP packet
  (`pdfcore::provenance`), so the record is readable by ordinary PDF tools.
- **DOCX** (D7): one extra package part,
  `omnioffice/provenance.json`, written by
  `officecore::docx_provenance`. The rest of the package is not touched, so
  Word still opens the file.

### The DOCX digest is content-canonical

A ZIP package's bytes depend on compression levels, timestamps and entry
order, so hashing the file itself would report a false change after any tool
rewrites it. The digest is therefore computed over the package's entries in a
canonical order:

```
for each entry sorted by name, skipping omnioffice/provenance.json:
    u64le(name length) · name · u64le(content length) · content
```

SHA-256 over that stream. Consequences:

- re-zipping the same content (any tool, any compression) still verifies;
- editing `word/document.xml`, a style, an image or the relationships part
  breaks the digest;
- re-embedding a manifest replaces the old part and keeps the digest stable.

The same rule is what makes tampering detectable *for the whole office part
tree*, not only for one file.

## The source chain

`provenance_read_chain` (app layer) sniffs the container by magic bytes
(`%PDF` or `PK\x03\x04`) and returns one shape for both formats:

1. **Source links** — every recorded source, its size and SHA-256.
2. **Transformation link** — the producing module, app version and timestamp.
3. **Output link** — the document's SHA-256 recomputed while reading, compared
   with the digest recorded when the manifest was written.

The verification screen renders the links in order and gives one verdict:

| Verdict | Meaning | UI |
|---|---|---|
| `intact` | Recomputed digest equals the recorded one. | Green badge. |
| `changed` | The document differs from the bytes the manifest was written for. | Red alert: *"This document was changed after its source manifest was written…"* — the recorded sources are no longer trustworthy. |
| `absent` | No manifest part in the document. | Neutral hint and the "add manifest" action. |
| `unsupported` | The file is neither a PDF nor a DOCX package. | Warning badge. |

## Automatic embedding in producers

A produced file never fails because of provenance; the hooks are best effort
and only run after the output is written.

| Producer | Container | Module |
|---|---|---|
| Format conversion to DOCX (`office_convert`) | DOCX | `convert` |
| OCR (`ocr_pdf`) | PDF | `ocr` |
| Merge (`merge_pdfs`) | PDF | `merge` |

Screens that hand a model to the user's own save dialog (for example the
invoice export through the office save path, or a manual Writer export) keep
the explicit **"Add source manifest"** action; it writes a copy with the
manifest embedded and never touches the original.

Signed PDFs are still refused: rewriting a signed document would break every
signature. The manifest is added to an unsigned copy instead.

## Limits, honestly

- DOCX has no XMP equivalent here; the manifest lives only in the package part,
  so third-party readers that do not look inside a DOCX will not see it. The
  app always reads it back.
- The chain is only as strong as the recorded `documentSha256`: it detects
  changes to the document, not a forged manifest (an attacker who rewrites
  both the document and the manifest can produce a self-consistent record).
  The screen shows the recorded digests so the user can compare them
  out-of-band.
- A source file is not re-read while verifying; the chain shows the digests
  that were recorded when the manifest was written.
- On Android the card reads manifests but adding one needs the desktop save
  dialog.

## Tests

- `crates/officecore/src/docx_provenance.rs#embedding_round_trips_the_manifest_and_the_digest`
- `crates/officecore/src/docx_provenance.rs#changing_a_part_after_the_manifest_is_detected`
- `crates/officecore/src/docx_provenance.rs#the_digest_does_not_depend_on_zip_order_or_timestamps`
- `src-tauri/src/provenance.rs#a_produced_docx_gets_the_manifest_attached_automatically`
- `src-tauri/src/provenance.rs#the_chain_lists_sources_then_the_transformation_then_the_output`
- `src/components/provenance-card.test.tsx#warns clearly when the document no longer matches its manifest`

## Türkçe özet

D7, C8'in PDF kaynak bildirgesini DOCX'e taşır ve bildirgeyi görünür bir
**türetim zincirine** dönüştürür: kaynaklar → dönüşüm adımı → çıktı.

- DOCX bildirgesi pakete tek parça olarak gömülür
  (`omnioffice/provenance.json`); paketin diğer parçaları değişmez, Word
  dosyayı normal açmaya devam eder.
- DOCX özeti dosya baytları yerine **içerikten** hesaplanır: adlarına göre
  sıralı paket parçaları (bildirge hariç) üzerinden SHA-256. Böylece dosyayı
  yeniden sıkıştırmak yanlış "değişti" uyarısı vermez; `word/document.xml`
  gibi bir parça değişince özet bozulur ve zincirde kırmızı uyarı çıkar.
- Doğrulama ekranı iki biçim için de aynı zinciri gösterir: kaynaklar
  (ad, boyut, SHA-256), dönüşüm (modül, sürüm, tarih) ve çıktı özeti.
- Üreticiler bildirgeyi otomatik gömer: DOCX'e dönüştürme (`convert`), OCR
  (`ocr`) ve birleştirme (`merge`). Hata durumunda üretim durmaz.
- İmzalı PDF reddedilir (imza bozulur); bildirge imzasız kopyaya eklenir.
- Sınırlar: DOCX'te XMP yok (bildirge yalnızca paket içinde), zincir kaydedilen
  özet kadar güçlü (hem belgeyi hem bildirgeyi yeniden yazan biri tutarlı sahte
  kayıt üretebilir), kaynak dosyalar doğrulama sırasında yeniden okunmaz.
