# Reproducibility report (D11)

F-Droid and other third parties can only **verify** a release binary against
the source when the inputs are pinned and visible. This page records the inputs
this repository controls, what still differs between two builds of the same
commit, and the commands a reviewer can run. It is a report, not a promise:
the measured delta is stated per item.

Run the input gate with `npm run repro:check` (also part of the test suite).

## Inputs that are pinned

| Input | State | Where |
|---|---|---|
| Rust dependencies | `Cargo.lock` committed, not ignored | repository root |
| npm dependencies | `package-lock.json` committed; workflows use `npm ci` | repository root, `.github/workflows` |
| Workspace version | one version in `Cargo.toml`, every crate inherits `version.workspace = true` | `Cargo.toml`, `crates/*`, `src-tauri` |
| GitHub actions | every `uses:` pinned to a 40-character commit SHA | `.github/workflows` |
| Release artifact list | built from a sorted file list, not directory enumeration order | `scripts/package.ps1` |
| Declared MSRV | `rust-version = "1.89"` in the workspace | `Cargo.toml` |
| Engine binaries | `engines.lock.json` records URL and SHA-256 per engine | `src-tauri/resources/engines` |

The gate fails if a lockfile disappears, a crate starts carrying its own
version, an action loses its SHA pin, a workflow calls `npm install`, the
packaging script stops sorting, or this page loses one of its sections.

## What is not reproducible yet

| Item | Why it differs | Impact |
|---|---|---|
| Rust toolchain channel | `rust-toolchain.toml` pins the channel name (`stable`) and its components, not an exact compiler release | two builds months apart can still use different rustc versions (the MSRV and fuzz jobs override the file, so their gates are unaffected) |
| ZIP archives | `Compress-Archive` stores file timestamps and its ordering is not documented as stable | the Windows portable ZIP bytes differ between builds |
| NSIS installer | the NSIS stub and resource timestamps are embedded | the Setup `.exe` bytes differ |
| `.deb` / `.AppImage` | Debian changelog timestamps and AppImage squashfs ordering | bytes differ |
| Android APK/AAB | signing block and ZIP alignment; `zipalign` output is not byte-stable across tool versions | bytes differ |
| `latest.json`, SBOMs | generated with a timestamp and the run's version | bytes differ by design |

**Consequence:** today a reviewer can reproduce the *inputs* and rebuild a
working app, but cannot compare a released binary byte-for-byte against the
source. The release `build-info.json` and `SHA256SUMS` are the comparison
point, not a reproducibility proof.

## How to verify

1. Check the Rust toolchain actually used by a release: the release job logs
   `rustc --version`; compare with `rust-version` in `Cargo.toml`.
2. Rebuild from the tag with the same OS image (Ubuntu 24.04 for Linux) and
   record `sha256sum` of the produced files next to `SHA256SUMS-linux.txt`.
3. Compare the **embedded** version and commit: `build-info.json` in the
   release carries the version and the commit SHA; the app shows the same
   version in About.
4. For Android, compare the APK against `SHA256SUMS-android.txt` and check the
   signing certificate fingerprint (release keystore) rather than bytes.
5. Run `npm run repro:check` on the checkout: it re-validates every input above.

## Path to bit-reproducible Linux builds (next step)

1. Replace `rust-toolchain.toml`'s floating `stable` channel with an exact
   release and let CI install it; run the whole test suite with it before
   pinning. (The file itself now exists, so the components and channel are at
   least declared in one place.)
2. Build archives with `SOURCE_DATE_EPOCH` (ZIP mtime via a fixed date) and a
   deterministic file order; replace `Compress-Archive` for the portable ZIP.
3. Pin the AppImage tool and the NSIS version; verify two consecutive builds
   share SHA-256 before claiming anything.
4. Only then extend the report with "matched" rows.

## Türkçe özet

- **Sabitlenen girdiler:** `Cargo.lock` ve `package-lock.json` commit'li; tek
  workspace sürümü; her GitHub action commit SHA'sına pinli; `npm ci`;
  arşivler sıralı listeden; MSRV 1.89; motor SHA'ları `engines.lock.json`'da.
- **Henüz tekrarlanabilir değil:** rustc kanalı pinli değil; ZIP/NSIS/deb/
  AppImage/APK baytları zaman damgası, sıralama ve imza bloğu yüzünden
  değişiyor. Yani girdiler yeniden üretilebilir, ikili dosya bayt bayt değil.
- **Doğrulama:** `npm run repro:check`, sürüm job'ındaki `rustc --version`,
  `build-info.json` içindeki commit, APK sertifika parmak izi ve SHA-256
  listeleri. Bit-düzeyi tekrarlanabilirlik için yol: toolchain pin + sabit ZIP
  zamanı/sırası + pinli NSIS/AppImage araçları.
