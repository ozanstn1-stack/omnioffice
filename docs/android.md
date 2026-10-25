# Android build

The Android app is the same code base as the desktop app:

* the entire processing core (`pdfcore`) compiles unchanged for Android,
* all screens, tools, settings and the optional AI assistant are identical,
* pdfium and Tesseract ship **inside the APK**, so every operation works offline.

The only differences are in how the operating system lets an app touch files
(see [File handling](#file-handling)).

---

## Requirements

| Tool | Version | Notes |
| --- | --- | --- |
| JDK | 21 or newer | `JAVA_HOME` must point at it |
| Android SDK | platform 36, build-tools 36 | `ANDROID_HOME` / `ANDROID_SDK_ROOT` |
| Android NDK | 27.x (e.g. `27.3.13750724`) | `sdkmanager "ndk;27.3.13750724"` |
| Rust | 1.89+ with the Android targets | `rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android` |

The Gradle project lives in `src-tauri/gen/android` and is part of the
repository (only build outputs are ignored), so no `tauri android init` is
needed after a clone.

## Build

```powershell
npm install
npm run android:engines      # pdfium + Tesseract CLI + language models (once)
npm run android:build        # release APK for arm64-v8a
npm run android:build:all    # release APKs for arm64-v8a + armeabi-v7a
```

The APKs land in `release-artifacts/` as
`OmniOffice-Android-<version>-<abi>.apk`. Pass `-Bundle` to also
produce signed AABs:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -Command "& ./scripts/build-android.ps1 -Abi arm64-v8a,armeabi-v7a -Bundle"
```

Extra switches:

```powershell
powershell -File scripts/build-android.ps1 -Abi x86_64 -Debug    # emulator build
powershell -File scripts/build-android.ps1 -SkipFrontend         # reuse dist/
powershell -File scripts/build-android.ps1 -AndroidHome D:\Sdk -NdkHome D:\Sdk\ndk\27.3.13750724
```

The app version comes from `src-tauri/tauri.conf.json`; the build script
writes it into `src-tauri/gen/android/app/tauri.properties` as `versionName`
and `versionCode` (`major*1e6 + minor*1e3 + patch`, monotonically
increasing).

Install on a device or emulator:

```powershell
adb install -r release-artifacts\OmniOffice-Android-<version>-arm64-v8a.apk
```

> **Why not `tauri android build`?** The Tauri CLI prepares `jniLibs` with
> file symlinks, which Windows only allows in Developer Mode. The build script
> compiles the Rust library with cargo and copies it, so plain Windows
> machines and CI produce the same APK. `tauri android dev` still works for
> interactive development.

## What ships in the APK

| Component | Where | Purpose |
| --- | --- | --- |
| `libpdf_sak_lib.so` | `lib/<abi>/` | the Rust application (frontend embedded via `tauri/custom-protocol`) |
| `libpdfium.so` | `lib/<abi>/` | page rendering, text extraction, search (bblanchon/pdfium-binaries) |
| `libtesseract.so` | `lib/<abi>/` | Tesseract 4.1.0 CLI, executed from the app's native library directory |
| `tessdata/*` | `assets/` | 9 OCR languages + OSD, copied to the app's private files directory on first launch |

`npm run android:engines` downloads these into `src-tauri/resources/engines-android`
(native libraries) and `src-tauri/resources/android-assets/tessdata` (models);
both directories are ignored by git, exactly like the desktop engines.

The APK is self-contained: the network permission is used only for the
optional AI assistant, cloud sync and the update check.

## File handling

Android does not give apps file paths for documents the user picks, and it
does not let apps open private files in other applications. The app bridges
this without changing any tool:

| Desktop | Android |
| --- | --- |
| `dialog.open()` returns a path | Storage Access Framework picker → the file is copied into the app cache and the tools receive a normal path |
| "Browse" chooses the output path | "Save as…" picks a destination (SAF); the finished file is copied there |
| Output is written next to the input | Output is written to the app Documents folder and then copied to `Downloads/OmniOffice/` (or the picked destination) |
| "Open folder" | "Share" opens the system share sheet for the result |

Nothing is uploaded anywhere: the copies live in the app's private storage and
the public Downloads folder.

### Sharing an open document

The office workspace (Writer, Calc, Impress) and the PDF reader have a
**Share** button. Office tabs are written from the current model - unsaved
edits included - to `cacheDir/share/<id>/<title>.<ext>` in the format they were
opened from when the editor can write it (`.xls`/`.doc`/`.oswk` go out as
XLSX/DOCX/PPTX), or as a PDF. The file is handed to the system share sheet
(WhatsApp, mail, Drive, Bluetooth, ...) straight from private storage through
`ShareFilePlugin.kt` and the app's FileProvider, which grants read access to
the chosen app only; `share_document_file` accepts only regular files inside
the app cache or data directory. When that bridge refuses, the older route
publishes a copy to `Downloads/OmniOffice` and shares that copy.
`adb logcat -s PdfSakShare` shows each opened chooser.

## Engine notes

* **pdfium** is loaded from the app's native library directory with
  `Pdfium::bind_to_library`; the packaged build is the same Chromium revision
  as the Windows build (155.0.8057.0).
* **Tesseract** is the official CLI built for Android (statically linked,
  Apache-2.0). It is packaged as `libtesseract.so` because executables may only
  be started from the native library directory on Android 10+.
* **qpdf** is not bundled on Android (`engines::qpdf_path` returns `None`
  there). PDF repair does not need it: the built-in engine rebuilds the file
  from its raw objects, so Repair works on Android. Fast Web View
  (linearization) needs qpdf and is desktop-only. The security features are
  implemented in Rust (`lopdf`, AES-256 R6).
* OCR quality is identical to the desktop build: same `tessdata_fast` models,
  same preprocessing pipeline, same output modes (searchable PDF, text,
  Markdown).

## Signing

Release APKs must be signed to be installable. `app/build.gradle.kts` reads
`src-tauri/gen/android/keystore.properties` when it exists (both the file and
the keystore are gitignored) and otherwise falls back to the local debug
keystore, so a `-Debug` or quick release build still installs.

The official release keystore (`pdfsak-release.jks`) is backed up as GitHub
Actions secrets, and the workflow recreates the properties file before
building, so CI and local builds produce APKs with the same signature and
users can update over an existing install:

| Secret | Content |
| --- | --- |
| `ANDROID_KEYSTORE_BASE64` | `base64 pdfsak-release.jks` |
| `ANDROID_KEYSTORE_PASSWORD` | store password |
| `ANDROID_KEY_ALIAS` | `pdfsak` |
| `ANDROID_KEY_PASSWORD` | key password (same as the store password for PKCS#12) |

Keep a copy of the keystore and its passwords somewhere safe: losing them
means users have to uninstall the app before installing an update.

## Updates

The app checks the project's GitHub releases for a newer version and can
update in place on Android: it downloads the APK for the device's ABI from
the release, checks its SHA-256 against the release checksum, and opens the
system installer (`REQUEST_INSTALL_PACKAGES` permission). Android asks the
user to allow "install unknown apps" for OmniOffice the first time; every
release is signed with the same project keystore, so the update installs over
the existing app and keeps user data. When the release has no signed
manifest/checksum, the app falls back to opening the release page in the
browser.

## CI

`.github/workflows/android.yml` builds signed APKs **and AABs** for both ABIs on
every version tag (and on manual dispatch) on an Ubuntu runner with the Android
SDK/NDK, and attaches them to the GitHub release. It also validates the
Android version metadata against `tauri.conf.json` and the tag.

## V3.1 platform behavior

* **Open with**: `MainActivity` handles `ACTION_VIEW` (including a document
  handed over in the intent's `ClipData`) and `ACTION_SEND`/`SEND_MULTIPLE`.
  Acceptance is provider-independent: a display name with a routable extension
  is used as-is; otherwise the type comes from the MIME type
  (`ContentResolver.getType`, then the intent type) and the bytes of the copied
  file may refine it by content sniffing - PDF, OOXML/ODF ZIP containers,
  legacy `.doc`/`.xls`/`.ppt` compound files, RTF, EML, XML and text/CSV
  (images are accepted by their declared MIME type; legacy `.xls` included). The stream is copied into app cache (256 MB cap, sanitized
  name) and the frontend drains `android_take_pending_open()` at startup, on
  new intents and on resume; content that cannot be identified is deleted and
  never queued. A refused document shows an explicit message (too large,
  unreadable/no access, type not recognised, unsupported) instead of one
  generic toast. On a device, `adb logcat -s PdfSakIntent` shows every received
  intent and `IncomingFiles` logs the resolved MIME, provisional type, sniffed
  type and outcome.
* **SAF**: office documents can be imported (`pickOfficeFiles`) and saved
  through a SAF destination or published to Downloads; `content://` URIs are
  never passed to the Rust backend.
* **Vault**: imports copies into app-private storage and indexes them.
* **Jobs**: persisted to `jobs.json`; long jobs keep running through a
  `dataSync` foreground service (since 4.3.0; it shows a notification and
  Android 13+ asks for the notification permission once), and a job whose
  process does die comes back as `interrupted` to retry.
* **Back navigation**: `handleBackNavigation = true` plus an in-app history so
  Back closes overlays/navigates before exiting.
* **Security**: `allowBackup=false`, data-extraction rules, narrowed
  FileProvider paths, network security config limiting cleartext to
  localhost/emulator hosts.

## Open-with troubleshooting

The open-with pipeline runs in the Kotlin shell (intent handling and the cache
copy), in Rust (the pending queue) and in the webview (routing) at the same
time, and most failures only reproduce on a real phone. The app therefore
collects an on-device report; no adb is needed:

1. Reproduce the problem once (share the document from the other app, or open
   it from Files) so the failed attempt is recorded.
2. Open **Settings → Diagnostics → Show open-with report**.
3. Press **Copy** and paste the text into the issue.

The report has three sections:

| Section | What it shows |
| --- | --- |
| `open-with: Rust side` | The app `cache_dir`, the `pending-open.txt` path and whether it exists, and the queue contents. An entry left in `pending_open_contents` means the copy succeeded but the webview never drained it; an empty queue right after the failure means the document never reached the queue. |
| `open-with: Kotlin log` | One line per received intent (action, type, `content://` data, flags, ClipData count), every candidate URI and its copy outcome (`COPIED` plus the destination, or the `CopyResult` reason), every queue append with the cache path, and one `drain count=N` line for every webview poll. |
| `open-with: frontend` | App version, WebView user agent and the last 100 routing events: drained counts, `__error:<reason>` tokens, `route:<screen>` decisions and `open-failed:<message>` entries. |

No `drain count=N` line at all means the webview never called
`android_take_pending_open`; `drain count=0` lines mean it polls and finds
nothing. The report contains document display names and cache paths, so read it
before sharing it publicly. On a development device the same Kotlin lines are
still available with `adb logcat -s PdfSakIntent`.
