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

The goal is the same APK that the release page already ships, found by the two
free Android catalogues instead of Play. OmniOffice fits them on paper: MIT
licensed, no Google Play services, no Firebase, no analytics, no ads and no
account. The submission itself still needs the account owner's action - this
section prepares everything that can live in the repository.

### Rules a release must keep (both catalogues)

| Rule | Where it is enforced |
|---|---|
| Tag is `vX.Y.Z`; release is published (not draft, not prerelease) | `auto-tag.yml` creates the tag after the Desktop workflow is green; `release.yml` and `android.yml` publish the release |
| Asset names are stable: `OmniOffice-Android-X.Y.Z-arm64-v8a.apk` (and `-armeabi-v7a`) | `android.yml` copies the Gradle output under exactly these names |
| `versionCode = major * 1_000_000 + minor * 1_000 + patch`, strictly increasing | checked by `auto-tag.yml` and, on the built APK, by `.github/scripts/verify-android-release.sh` |
| Every APK is signed with the same certificate | `verify-android-release.sh` compares the fresh APK against the previous release's APK at tag time |
| The APK carries the version code it claims (not only the metadata) | same script, `aapt dump badging` |
| Listing text and changelogs are in `fastlane/metadata/android/<locale>/` | `npm run fastlane:check` in the Lint job |
| No tracking, no proprietary dependencies, source available | the build is public in `.github/workflows/android.yml`; no Play Services in the dependency tree |

If the signing key is ever lost, every installed app is stranded: the key is
the identity. It lives in the `ANDROID_KEYSTORE_BASE64` / `ANDROID_KEYSTORE_PASSWORD`
/ `ANDROID_KEY_ALIAS` repository secrets, must be backed up offline, and must
never be rotated for an existing app id unless the app is republished under a
new id.

### Fastlane metadata

```
fastlane/metadata/android/en-US/title.txt
fastlane/metadata/android/en-US/short_description.txt
fastlane/metadata/android/en-US/full_description.txt
fastlane/metadata/android/en-US/changelogs/<versionCode>.txt
fastlane/metadata/android/tr-TR/...            (same layout)
```

- `title.txt` is `OmniOffice`; the short description stays under F-Droid's 80
  character limit; the full description is listing copy (features, privacy,
  licence) and is not generated from the README.
- Changelogs are keyed by the integer version code. The English files are
  generated from `CHANGELOG.md` (`npm run fastlane:sync`) and checked in CI
  (`npm run fastlane:check`), so release notes cannot drift from the changelog.
  The Turkish files are hand-written per release; the check only requires the
  current version code to be present there.
- Old codes are kept from 6.6.0 on; `scripts/fastlane-changelogs.mjs` has the
  `SINCE_VERSION` constant that decides how far back the tree keeps them.
- The catalogue reads this directory directly; nothing is copied to the APK.

### IzzyOnDroid (recommended first step)

IzzyOnDroid is a curated FOSS repository that downloads the APK from the
upstream release page instead of building it, so the bundled PDF engine
binaries are not a problem there.

How it works once listed:

- The app's repository entry carries a `Binaries:` pattern that resolves the
  GitHub release asset, e.g.
  `https://github.com/ozanstn1-stack/omnioffice/releases/download/v%v/OmniOffice-Android-%v-armeabi-v7a.apk`
  (`%v` is the version name). The updater runs on a schedule (hourly to daily),
  compares the version code and fetches the APK when it grows.
- The listing text comes from the fastlane directory above; screenshots are
  optional.
- The repository needs: the app id (`io.github.ozanstn1.pdfswissarmyknife`), the
  source URL, a licence (`MIT`), a non-draft published release with a stable
  asset name, and a signing key that never changes.
- ABI note: the repository lists one APK per version. Arm64-v8a is the right
  default (virtually every current phone); armeabi-v7a remains a GitHub-only
  download unless a universal APK is added later. Document the split clearly in
  the request.
- No Play Services requirement means the "anti-feature" list stays empty; the
  app also has no tracking libraries, which IzzyOnDroid checks.

Request steps (account owner):

1. Open an issue in [`IzzyOnDroid/repo`](https://gitlab.com/IzzyOnDroid/repo)
   ("App request") with: app name, app id, GitHub repository, release URL,
   licence, the ABI to publish and a note that the APK is built by GitHub
   Actions from the same source.
2. Wait for the review (a few days to two weeks). They may ask for a
   `Binaries:` URL in the exact asset shape above; no repository change is
   needed on our side as long as the release names stay stable.
3. After listing, every new `vX.Y.Z` release flows automatically, provided the
   rules table above holds.

### F-Droid (official, source build)

F-Droid builds every app on its own infrastructure from source and re-signs
it, so it is a separate distribution channel, not a mirror of the GitHub APK.
The recipe lives in [`fdroiddata`](https://gitlab.com/fdroid/fdroiddata) as
`metadata/io.github.ozanstn1.pdfswissarmyknife.yml`.

What the recipe needs:

```yaml
Categories: [Office, Science & Education]
License: MIT
AuthorName: OmniOffice contributors
WebSite: https://github.com/ozanstn1-stack/omnioffice
SourceCode: https://github.com/ozanstn1-stack/omnioffice
IssueTracker: https://github.com/ozanstn1-stack/omnioffice/issues
Summary: Local-first office suite and PDF toolbox
Description: |-
  (short version of the fastlane full description)
Builds:
  - versionName: 6.12.0
    versionCode: 6012000
    commit: <tag or commit>
    # Tauri v2 Android: rust + node + Android SDK/NDK, then gradle assembleRelease
    sudo: [apt-get install -y ...]
    prebuild: |
      npm ci
      rustup target add aarch64-linux-android
      (fetch the engine binaries - see the blocker below)
    ndk: r27c
    gradle:
      - yes
AutoUpdateMode: Version
UpdateCheckMode: Tags ^v[0-9.]+$
CurrentVersion: 6.12.0
CurrentVersionCode: 6012000
AllowedAPKSigningKeys: <our certificate SHA-256, only if F-Droid keeps the upstream signature>
```

Blockers to resolve before a `fdroiddata` merge request makes sense:

1. **Prebuilt engine binaries.** `scripts/fetch-engines.ps1` /
   `fetch-engines-android.ps1` download `pdfium`, `qpdf` and `tesseract`
   binaries listed in `engines.lock.json`. F-Droid's source-build rule expects
   every binary to be built from source in the build environment. Options: add
   a `prebuild` block that builds them (a large amount of work and build time),
   or remove the app's dependency on them for the F-Droid flavour (degraded PDF
   engine/OCR), or request an explicit prebuilt exception with the licences
   documented (pdfium BSD-3, qpdf Apache-2.0, tesseract Apache-2.0, Leptonica
   BSD-2) - the main repo is strict here, which is why IzzyOnDroid comes first.
2. **Toolchain.** Tauri v2 Android needs Rust (NDK targets), Node and the
   Android Gradle plugin inside the fdroiddata buildserver. That is possible
   with `prebuild`/`sudo`/`ndk` entries but has to be proven in a test build;
   the app must build without the Windows/Linux engine binaries finding step.
3. **Reproducibility and signature.** F-Droid re-signs the APK with its own
   key, so F-Droid users cannot update to the GitHub/IzzyOnDroid APK and vice
   versa. Pick the catalogue deliberately; if both are wanted, keep two
   flavours and say so in the store listing.
4. **Size and metadata.** The APK is ~36 MB per ABI; the full description and
   changelogs come from the fastlane directory, so the yml only needs `Summary`
   and the short description.

Merge-request steps (account owner): fork `fdroiddata`, add the yml, run
`fdroid lint` and `fdroid build --server` where possible, push a branch and
open the MR with the "[new app]" template. Review can take weeks and the
reviewer will pick apart the prebuilt binaries - do this only after the
blockers above are decided.

### Verifying a published APK by hand

The CI check runs on tag builds; the same commands work locally with the
Android build-tools (`apksigner`, `aapt`):

```bash
gh release download v6.12.0 --pattern 'OmniOffice-Android-6.12.0-arm64-v8a.apk' -D /tmp/apk
apksigner verify --print-certs /tmp/apk/OmniOffice-Android-6.12.0-arm64-v8a.apk
aapt dump badging /tmp/apk/OmniOffice-Android-6.12.0-arm64-v8a.apk | grep versionCode
```

- The certificate SHA-256 must equal the previous release's value; a change is
  a breaking change for every installed user.
- The `versionCode` must be `6012000` for 6.12.0 (and follow the formula for
  every later release).
- The AAB is for Play-style stores, not for these catalogues; only the APKs
  are submitted.
