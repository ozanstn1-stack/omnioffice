# IzzyOnDroid app inclusion request — ready-to-file text

IzzyOnDroid moved its app requests from GitLab to Codeberg:

- Repository: <https://codeberg.org/IzzyOnDroid/repodata>
- New issue: <https://codeberg.org/IzzyOnDroid/repodata/issues/new/choose>
- Template: **App Inclusion Request** (`app-inclusion-request.yaml`)
- Title format: `[AppRequest] OmniOffice`
- Labels are added by the template: `app-request`, `needs/apk-scan`

The form is structured (one field per entry, some dropdowns), so this file is
the source of truth for the answers: copy each block into the matching field.
Everything below is filled in for OmniOffice; the only things left to the
submitter are the account and pressing "Create issue".

## Field: Guidelines (checkboxes)

- [x] I am the developer of the app.
- [x] The app complies with the [App Inclusion Policy](https://izzyondroid.org/docs/general/AppInclusionPolicy/).
- [x] The app is not already listed in the repo or issue tracker.
- [x] The [Fastlane](https://izzyondroid.org/docs/general/Fastlane/) folder is available in the app's repo.

## Field: Link to the source code

```text
https://github.com/ozanstn1-stack/omnioffice
```

## Field: Link to app in another app store

```text
Not listed in another store yet. The signed APKs and the iOS-free builds are on the GitHub release page:
https://github.com/ozanstn1-stack/omnioffice/releases
```

## Field: License used

```text
MIT
```

## Field: Categories (select at least one, max 5)

```text
Office
Writing
```

## Field: Summary

```text
Local-first office suite and PDF toolkit: Writer, Calc, Impress, the complete PDF toolbox, an on-device document scanner and offline Turkish AI task packs. No account, no cloud, no telemetry.
```

## Field: Description

```text
OmniOffice is a local-first productivity suite for Android and Windows. Everything runs on the device: documents are never uploaded, there is no telemetry, no advertising and no account.

Main features:
- Writer, Calc and Impress with DOCX, XLSX, PPTX and ODF import/export, plus templates with {{field}} placeholders and an encrypted local address book.
- Complete PDF toolset: merge, split, compress, OCR with a Turkish searchable text layer, redaction, signing and signature verification, forms, evidence bundles, provenance manifests and model-level document comparison.
- Document scanner: page corners are found on the device (Canny/Hough detection), perspective-corrected, filtered to grey or black and white, and assembled into a multi-page PDF.
- Turkish AI task packs (petition fixes, contract clauses, VAT summaries, notification deadlines) that run against a local Ollama model by default; the app ships no model and nothing leaves the device unless the user points it at a cloud provider.
- LAN sync: two devices on the same Wi-Fi pair with a one-time six-digit code, keep a pinned TLS connection and move documents between folders with a three-way plan; conflicts are never overwritten automatically and deletions require confirmation.
- Turkish spellcheck helpers, date/stamp tools and a privacy-preserving "report a problem" flow that shows the redacted diagnostics before anything is copied.

Privacy and platform:
- No Google Play services, no Firebase, no analytics SDKs, no ads, no in-app purchases.
- Macros and embedded scripts in office files are never executed.
- Network access only when the user enables cloud sync, fetches engines or shares a folder on the local network.
- Minimum Android 7.0 (API 24), split APKs for arm64-v8a and armeabi-v7a, MIT licensed.

App ID: io.github.ozanstn1.pdfswissarmyknife
```

## Field: Build instructions

```text
Build from source with the same steps as CI (Node 22, Rust stable, Android SDK/NDK, JDK 17):

1. npm ci
2. npx tauri android build -- --apk --ci --target aarch64

The Android release build needs a signing keystore; for a local test build use
the debug keystore (omit --ci and sign with the default debug key). CI uses
.github/workflows/android.yml, which also runs the frontend tests, clippy for
aarch64-linux-android and the Android unit/instrumented tests.

The release APK that IzzyOnDroid should pull is built by that workflow from the
tagged commit and attached to the GitHub release with this naming scheme:
https://github.com/ozanstn1-stack/omnioffice/releases/download/v%v/OmniOffice-Android-%v-arm64-v8a.apk
(%v = version name, e.g. 6.12.0; the versionCode is major*1000000+minor*1000+patch)
```

## Field: AI Tools Usage — Assistance Level (dropdown)

```text
Substantial – Used throughout development
```

## Field: AI Tool(s)

```text
DeepSeek Harness (DeepSeek V4), Claude (Anthropic)
```

## Field: What did the tools help with, and how?

```text
AI agents drafted and edited most of the code, documentation and tests under the
developer's direction, review and device testing: feature implementation (office
editors, PDF tools, scanner, LAN sync, AI task packs), test suites, CI workflows
and documentation. The release notes and the F-Droid/IzzyOnDroid metadata are
reviewed by the human developer before release.
```

## Field: AI Accountability (checkboxes)

- [x] The human developer(s) reviewed and edited all "AI"-generated outputs
- [x] The human developer(s) ran manual tests and manually verified all changes

## Field: Further Notices

```text
App ID: io.github.ozanstn1.pdfswissarmyknife

- The repository ships fastlane metadata at fastlane/metadata/android/{en-US,tr-TR}
  (title, short/full description, changelogs per versionCode). The Turkish
  listing is translated; changelogs are English (generated from CHANGELOG.md)
  and Turkish (hand-written).
- Release assets are stable and versioned: OmniOffice-Android-<version>-arm64-v8a.apk,
  ...-armeabi-v7a.apk and the two AABs (the AABs are for Play-style stores and
  are not needed by the repo). SHA256SUMS-android.txt is attached to every release.
- Please list the arm64-v8a APK; the armeabi-v7a variant stays a GitHub-only
  download unless a universal APK is added later.
- The APK is signed with a stable release key and the versionCode is monotonic;
  a GitHub Actions check compares the signing certificate with the previous
  release on every tag.
- The bundled PDF/OCR engines (pdfium, qpdf, tesseract) are free-licensed
  (BSD-3, Apache-2.0, Apache-2.0/BSD-2) and are downloaded at build time by the
  project's own script from the upstream prebuilt releases; no Play services,
  no Firebase, no trackers.
- Anti-features: none.
```

## Before submitting

1. The repository state is already prepared (fastlane metadata, changelogs,
   release rules in `docs/distribution.md`); this file only needs the account.
2. Check the newest release is published (not draft) and the arm64 asset exists.
3. Optional: add screenshots under `fastlane/metadata/android/en-US/images/`
   (phoneScreenshots) — the template does not require them.
4. File the issue, then check it back weekly; the review may ask for the
   `Binaries:` line to be adjusted or for a screenshot.
