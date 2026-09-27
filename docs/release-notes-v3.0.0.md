# Office Swiss Army Knife 3.0.0

Local-first Office + PDF + document intelligence platform. Everything runs on
your machine: no telemetry, no cloud upload, AI is opt-in.

## Downloads

| Platform | File |
|---|---|
| Windows (installer) | `Office-Swiss-Army-Knife-Setup-3.0.0.exe` |
| Windows (portable) | `Office-Swiss-Army-Knife-Portable-3.0.0.zip` |
| Android (arm64, most phones) | `PDF-Swiss-Army-Knife-Android-3.0.0-arm64-v8a.apk` |
| Android (armv7, older devices) | `PDF-Swiss-Army-Knife-Android-3.0.0-armeabi-v7a.apk` |

Checksums: `SHA256SUMS.txt` (Windows), `SHA256SUMS-android.txt` (Android).

## Highlights

- **Writer**: real sections with per-section page setup and first/even
  headers, footnotes/endnotes with a reserved note area, tracked changes
  (insert/delete/format) with accept/reject, comments with replies, bookmarks
  and cross-reference fields. All of it round-trips through DOCX and renders
  into the exported PDF. Clicking a page fragment places the caret at the
  clicked character.
- **Calc**: Excel-style structured tables with structured references
  (`=SUM(Sales[Amount])`, `Sales[@Amount]`), formula autocomplete with argument
  hints, trace precedents/dependents auditing, and real XLSX import fidelity
  (styles, widths, heights, merges, freeze panes, validation, conditional
  formatting, hyperlinks, comments, names, tables).
- **Impress**: master slides and layouts with placeholder inheritance, real
  nested shape groups, PPTX chart import/export, an animation engine that runs
  in the slideshow, and a presenter view.
- **PDF**: a real sanitizer (JavaScript, embedded files, actions, unsafe
  annotations, metadata), annotation/form flattening, PDF/A-1b/2b/3b validation
  with honest conversion, redaction verification, and OCR preprocessing
  (deskew/denoise/threshold/orientation) that actually runs. New PDF Studio
  screen.
- **Document Vault**: opt-in local indexing of folders you choose with
  full-text/phrase/fuzzy search, snippets and preview. Nothing is scanned
  unless you add the folder.
- **Platform**: command palette (`Ctrl+Shift+P`), global search
  (`Ctrl+Shift+F`), background job center, Compatibility Center, `.oswk`
  schema versioning with migrations (V2.x documents open and upgrade).
- **AI (opt-in)**: DeepSeek, OpenAI-compatible, Ollama (local), Gemini and
  custom providers; per-document consent, send scope, and document chat with
  `[page N]` citations.

## Notes

- Android APKs are signed with the project release key; installs over older
  versions keep working.
- Old `.oswk` documents open unchanged and are migrated in memory.
- Known limitations are listed in the README (digital signatures, plugin
  runtime and cloud sync are declared as not implemented in 3.0.0).

Full changelog: [CHANGELOG.md](https://github.com/ozanstn1-stack/pdf-swiss-army-knife/blob/master/CHANGELOG.md)
