# Office Swiss Army Knife v3.1.0

**Cross-platform completion, Office/PDF fidelity, secure signing and
production release.**

This release turns the V3.0 platform into a working family across Windows and
Android: the Writer paginated view is a real editing canvas, PDFs get genuine
CMS/PKCS#7 signatures and fillable forms, PDF/A conversion embeds missing
fonts, XLSX/PPTX/ODT/RTF imports keep far more of the original file, and the
Android app gains intents, SAF file handling, touch editing, vault import and
persistent jobs. A sandboxed plugin runtime and a local-first WebDAV sync
foundation land with data-loss protection in front of every lossy save.

## Highlights

- **Writer**: type directly on the page; caret crosses page boundaries;
  click-to-caret on continuation fragments; touch and mouse selection.
- **Real digital signatures**: detached CMS/PKCS#7 (X.509, SHA-256, RSA and
  ECDSA P-256), visible appearance, ByteRange integrity, re-verification of
  the written file; certificates from the Windows store or a PKCS#12 file on
  any platform; validation reports signer, chain, expiry-relative-to-now and
  modification status. Trust is explicitly reported as `unknown` offline.
- **PDF Studio**: fill and validate AcroForm fields (text, checkbox, radio,
  dropdown, list), regenerate appearances, flatten afterwards, and move/
  resize/rotate/delete annotations, form widgets and drawn images.
- **PDF/A**: font discovery + metric-compatible substitution + `/FontFile2`
  embedding for simple fonts, real sRGB ICC `/DestOutputProfile`, and honest
  reporting for what cannot be embedded (CID/Type0, symbolic, custom
  encodings).
- **XLSX import fidelity**: charts, pictures with anchors/rotation, print
  settings (page setup, margins, breaks, Print_Area/Titles, header/footer),
  sheet protection and lossless pivot parts.
- **PPTX charts**: cached categories/values plus an embedded workbook, so
  charts open with data elsewhere.
- **ODT/RTF notes and tracked changes** round-trip.
- **Android**: open-with intents (content:// safe copy), SAF import/export for
  office formats, Document Vault import + indexing, background job
  persistence, touch UX for all editors and the reader, in-app back
  navigation, hardened manifest.
- **Plugins**: sandboxed Web Worker runtime with a manifest permission model
  and a sample plugin.
- **Cloud sync foundation**: WebDAV upload/download with three-way conflict
  detection and manual resolution; off by default; OneDrive/Google Drive
  declared as OAuth-only and explicitly unavailable in this build.
- **Data Loss Protection**: every lossy save/export shows the compatibility
  matrix (Supported?/Imported?/Exported?/Transformed?/Lost?) with Continue,
  Cancel and Save as `.oswk`.

## Release assets

| Platform | File |
|---|---|
| Windows | `Office Swiss Army Knife_3.1.0_x64-setup.exe` (NSIS installer) |
| Windows | `Office-Swiss-Army-Knife-Setup-3.1.0.exe` (same installer, legacy name) |
| Windows | `Office-Swiss-Army-Knife-Portable-3.1.0.zip` (portable) |
| Android | `PDF-Swiss-Army-Knife-Android-3.1.0-arm64-v8a.apk` |
| Android | `PDF-Swiss-Army-Knife-Android-3.1.0-armeabi-v7a.apk` |
| Android | `PDF-Swiss-Army-Knife-Android-3.1.0-arm64-v8a.aab` |
| Android | `PDF-Swiss-Army-Knife-Android-3.1.0-armeabi-v7a.aab` |
| Checksums | `SHA256SUMS.txt`, `SHA256SUMS-android.txt` |

Android release APKs are signed with the project release keystore
(`CN=PDF Swiss Army Knife`), `versionName=3.1.0`, `versionCode=3001000`,
minSdk 24, targetSdk 36.

## Android behavior notes

- Documents open from other apps through ACTION_VIEW/SEND; the file is copied
  into app cache (size/extension guarded) before it is parsed.
- Storage uses the Storage Access Framework: import copies live in the app's
  private vault storage; exports go through a SAF save target or the public
  Downloads folder.
- The vault indexes imported documents, not live folders (SAF has no Rust-
  browsable paths); Windows keeps folder scanning.
- Cloud/AI are opt-in; cleartext HTTP is restricted to localhost/emulator
  hosts by the network security config (Rust-side LAN access for local model
  servers is unaffected).
- Backups are disabled for app data, so API keys and documents are not copied
  into Android auto-backup.

## Verification

- Rust workspace tests, frontend tests and `tsc --noEmit` gate the build.
- Golden `.oswk` fixtures pin V3.1 feature survival across `.oswk` and
  DOCX/XLSX/PPTX round trips (the same engine runs on Android).
- Performance guards cover Writer pagination/PDF export, Calc 20k/100k/500k
  cell recalculation chains and PDF merge/text/render workloads.
- The Windows installer was installed, launched, association-checked and
  uninstalled on a real machine during packaging. Android APKs were built,
  signed, and their manifest/native-libs verified; emulator smoke coverage is
  described in the release report.

## Known limitations

See the "Known limitations" section of `README.md` for the honest list,
including: contiguous tracked structural revisions, PDF content-stream text/
vector editing, Type0/CID PDF/A font embedding, OAuth cloud providers,
Android foreground services, and offline-only signature trust evaluation.
