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
`PDF-Swiss-Army-Knife-Android-<version>-<abi>.apk`. Pass `-Bundle` to also
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
writes `app/tauri.properties` (`versionName=3.4.0`,
`versionCode=3004000` — `major*1e6 + minor*1e3 + patch`, monotonically
increasing).

Install on a device or emulator:

```powershell
adb install -r release-artifacts\PDF-Swiss-Army-Knife-Android-3.4.0-arm64-v8a.apk
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

The APK is self-contained: no network permission is used for anything except
the optional AI assistant (which needs `INTERNET`).

## File handling

Android does not give apps file paths for documents the user picks, and it
does not let apps open private files in other applications. The app bridges
this without changing any tool:

| Desktop | Android |
| --- | --- |
| `dialog.open()` returns a path | Storage Access Framework picker → the file is copied into the app cache and the tools receive a normal path |
| "Browse" chooses the output path | "Save as…" picks a destination (SAF); the finished file is copied there |
| Output is written next to the input | Output is written to the app Documents folder and then copied to `Downloads/PDF Swiss Army Knife/` (or the picked destination) |
| "Open folder" | "Share" opens the system share sheet for the result |

Nothing is uploaded anywhere: the copies live in the app's private storage and
the public Downloads folder.

## Engine notes

* **pdfium** is loaded from the app's native library directory with
  `Pdfium::bind_to_library`; the packaged build is the same Chromium revision
  as the Windows build (155.0.8057.0).
* **Tesseract** is the official CLI built for Android (statically linked,
  Apache-2.0). It is packaged as `libtesseract.so` because executables may only
  be started from the native library directory on Android 10+.
* **qpdf** is not bundled (it is only used for desktop-side diagnostics); the
  security features are implemented in Rust (`lopdf`, AES-256 R6).
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

## CI

`.github/workflows/android.yml` builds signed APKs **and AABs** for both ABIs on
every version tag (and on manual dispatch) on an Ubuntu runner with the Android
SDK/NDK, and attaches them to the GitHub release. It also validates the
Android version metadata against `tauri.conf.json` and the tag.

## V3.1 platform behavior

* **Open with**: `MainActivity` handles `ACTION_VIEW` and
  `ACTION_SEND`/`SEND_MULTIPLE`, validates the extension against a whitelist,
  copies the stream into app cache (256 MB cap, sanitized name) and the
  frontend drains `android_take_pending_open()` at startup, on new intents and
  on resume.
* **SAF**: office documents can be imported (`pickOfficeFiles`) and saved
  through a SAF destination or published to Downloads; `content://` URIs are
  never passed to the Rust backend.
* **Vault**: imports copies into app-private storage and indexes them.
* **Jobs**: persisted to `jobs.json`; jobs that were running when the process
  died come back as `interrupted` (no foreground service in V3.1).
* **Back navigation**: `handleBackNavigation = true` plus an in-app history so
  Back closes overlays/navigates before exiting.
* **Security**: `allowBackup=false`, data-extraction rules, narrowed
  FileProvider paths, network security config limiting cleartext to
  localhost/emulator hosts.
