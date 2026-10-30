# Distribution

How an OmniOffice version reaches users today and what is still missing. The
release itself is described in [`plan/AI-HANDOFF.md`](plan/AI-HANDOFF.md); this
file covers the packaging and store side.

## What every release ships

| Platform | Artifact | Built by | Status |
|---|---|---|---|
| Windows | `OmniOffice-Setup-X.Y.Z.exe` (NSIS, updater manifest signed) | `release.yml` | Signed updater, **unsigned exe** (SmartScreen warning) |
| Windows | `OmniOffice-Portable-X.Y.Z.zip` | `release.yml` | portable folder |
| Android | `OmniOffice-Android-X.Y.Z-<abi>.apk` and `.aab` | `android.yml` | release-signed by the `ANDROID_KEYSTORE_BASE64` secret, sideload + store ready |
| Linux | `OmniOffice-Linux-X.Y.Z-amd64.deb`, `.AppImage` | `linux-package.yml` (called by `release.yml`) | shipped since 6.7.0 |

Every artifact is listed in `SHA256SUMS*.txt` on the release page. The release
notes are generated from the version's `CHANGELOG.md` section.

## Linux packages

`linux-package.yml` builds the `.deb` and the `.AppImage` on `ubuntu-24.04`,
renames them to the versioned names above and attaches them to the release on a
`v*` tag. A manual dispatch runs it on a branch and only uploads the workflow
artifact, so the packaging can be tried without publishing:

```bash
gh workflow run linux-package.yml --ref <branch>
```

The engine binaries (`scripts/fetch-engines.ps1`) are fetched on Windows; the
Linux bundle ships without the bundled PDF engine and falls back to a system
`qpdf`, so the package is honest about that in the app's engine report.

## Windows Authenticode signing

The installer is still unsigned, so Windows SmartScreen shows "unknown
publisher". The updater manifest *is* signed (`latest.json.sig`), which is a
different key and does not change the exe warning. Three ways to fix it:

| Provider | Cost | Pros | Cons |
|---|---|---|---|
| Azure Trusted Signing | ~$9.99/month + identity validation | Microsoft-hosted, CI-friendly (`azure/trusted-signing-action`), short-lived certs | needs an Azure account and a verified identity (individual allowed in supported regions) |
| SignPath.io | free for open source | designed for OSS, GitHub Action, review by SignPath | request/approval flow, artifacts go through their service |
| Own OV/EV certificate | yearly certificate | full control | most expensive, key management on us, EV is required for instant SmartScreen reputation |

Whichever is chosen, the release workflow change lives in the Windows job of
`.github/workflows/release.yml`: sign `OmniOffice-Setup-X.Y.Z.exe` (and the app
exe inside the portable zip) after `tauri build`, before the release upload.
The signing step must be skipped when its secrets are absent so a fork or a
secret-less run still produces a working release. Required secrets per provider:

- Azure Trusted Signing: `AZURE_TENANT_ID`, `AZURE_CLIENT_ID`, `AZURE_CLIENT_SECRET`, `AZURE_TS_ENDPOINT`, `AZURE_TS_ACCOUNT`, `AZURE_TS_PROFILE`.
- SignPath: `SIGNPATH_API_TOKEN`, `SIGNPATH_ORGANIZATION_ID`, `SIGNPATH_PROJECT_SLUG`, `SIGNPATH_POLICY_SLUG`.
- Own certificate: `WINDOWS_CERT_PFX` (base64) and `WINDOWS_CERT_PASSWORD`.

The README and this file say "unsigned" until the step is live; the release
readiness must not claim a signature before the artifact is actually signed.

## F-Droid / IzzyOnDroid

The APK is release-signed with a stable key and the version code follows
`major * 1_000_000 + minor * 1_000 + patch`, which is what both catalogues
require.

- **IzzyOnDroid** picks releases up automatically from the GitHub release page:
  a signed APK with a monotonic version code is enough, and the fastlane
  metadata in `fastlane/metadata/android/` supplies the listing text. This is
  the shortest path and fits the local-first, no-Google-services profile.
- **F-Droid** needs a merge request to [`fdroiddata`](https://gitlab.com/fdroid/fdroiddata)
  with a build recipe. The bundled `pdfium`/`tesseract` binaries from
  `engines.lock.json` are fetched at build time and are not buildable from
  source in F-Droid's build environment, so the recipe has to either skip them
  (degraded PDF engine) or the app has to be accepted with a prebuilt-binary
  exception; this is why IzzyOnDroid is the recommended first step.

Fastlane layout (per-locale title, short and full description, plus one
changelog file per version code):

```
fastlane/metadata/android/en-US/title.txt
fastlane/metadata/android/en-US/short_description.txt
fastlane/metadata/android/en-US/full_description.txt
fastlane/metadata/android/en-US/changelogs/<versionCode>.txt
```
