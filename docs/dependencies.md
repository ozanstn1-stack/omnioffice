# Dependency updates

The v6.0.0 audit (B8) found the dependency PR queue drifting: seven open
Dependabot branches, several red for weeks, and the Tauri npm/Cargo halves
bumped separately even though `tauri build` refuses a mismatched pair.

## Renovate (the target setup)

`renovate.json` at the repository root is the reviewed policy:

* the **Tauri desktop stack** (npm `@tauri-apps/*` and the Cargo `tauri*`
  crates) moves as one grouped PR, so the plugin/version check cannot fail
  because only one side moved;
* **GitHub Actions** are grouped and kept SHA-pinned;
* the **Chrome extension** (its own lock file) moves separately;
* **major** updates wait for a dashboard approval - they need a migration, not
  a rubber stamp;
* lock-file maintenance runs monthly.

Activating it needs the Renovate GitHub App installed on this repository,
which only the platform owner can do:

1. install the [Renovate app](https://github.com/apps/renovate) for
   `ozanstn1-stack/omnioffice`,
2. delete `.github/dependabot.yml` (one file, one commit).

Until the app is installed, Dependabot stays configured so updates keep
flowing; the two bots must not run at the same time for long, or the queue
splits again.

## Current queue (v6.0.1)

* Merged once the foundation PRs landed: jsdom, jest-dom, the Actions group
  and the Chrome-extension group (all green).
* Closed rather than merged: the Windows 0.62, rcgen 0.14 and x509-cert 0.3
  majors (API migrations are their own work) and the duplicate frontend-group
  branches; Renovate re-opens them behind the approval rule.

## Vendored patch: calamine 0.37.0

`vendor/calamine` is a verbatim copy of crates.io 0.37.0 with two arithmetic
guards, both found by nightly fuzzing (cargo-fuzz builds with overflow checks,
so the panic aborted a fuzz target even though the app's worker would catch it;
the fuzz gate must stay green for releases):

* `cell_ref_to_row_col` uses `checked_mul`/`checked_add` so a hostile cell
  reference (`AAAAAAAA...`, a giant row number) is a parse error instead of an
  integer-overflow panic;
* `get_dimension` normalizes a reversed `<dimension ref="O1:C4"/>` to its
  bounding box (and the table header/totals offsets use saturating arithmetic),
  so the `end - start` maths - and `Dimensions::len` - cannot underflow.

Both `Cargo.toml` files patch the crate (`[patch.crates-io]`). Remove the
directory and the patches when calamine ships the same guards upstream
(`vendor/calamine/README.md` has the details). The rustfmt gate skips
`vendor/` because it is third-party source.

## tauri-specta (B2/2 scaffold, #316)

The migration is pinned to the release candidate pair that resolves together:
`tauri-specta =2.0.0-rc.26`, `specta =2.0.0-rc.26`, `specta-typescript 0.0.13`.
It sits behind the `bindings` Cargo feature so normal builds and the release
binaries are unchanged; `npm run bindings:specta` exports the current scaffold
to `src/lib/generated/specta.ts`.

The RC risk is handled by moving **command groups one PR at a time**: each group
gets `#[specta::specta]`, joins `collect_commands!`, and its frontend call site
switches to the generated wrapper. Until a group lands, the hand-written
`invoke` path stays authoritative.

The groups are the domains of `src-tauri/src/ipc.rs` (#338). Migrated so far:

- **vault** (9 commands, #316 group 1):
  - **Types:** the DTOs derive `specta::Type` behind `bindings`. `pdfcore` gets a `bindings` feature for `ErrorCode` and a hand-written `Type` for `PdfError`, matching its `{ code, message }` serializer. `u64`/`usize` fields are marked `specta_typescript::Number`: the exporter refuses BigInt types, and these are byte counts and counters far below 2^53.
  - **Screens:** `Vault.tsx` and the command palette call `commands.*` through `unwrap` (`src/lib/typed-ipc.ts`, which rejects with the error object as `invoke` does). Their hand-written types became aliases of the generated ones.
  - **Exception:** `vault_scan` stays on `invokeTracked` (the job tracker needs the command name), with the generated request type.

`npm run bindings:check` regenerates `src/lib/generated/specta.ts` and fails on drift; the Linux Rust job runs it. Next groups, in order of size: plugin (10), sync (15), ai (31), app (42), office (43), pdf (70).

## Direct dependency: imageproc (C12)

The document scanner's automatic page detection
(`crates/pdfcore/src/page_detect.rs`, C12) adds one direct dependency:
`imageproc` 0.27 (MIT), with the default features off - `text`, `fft` and
`sdl2` stay out, only the edge, filter, contrast, contour and geometry modules
are compiled.

- **Licenses:** every transitive crate is already covered by the `[licenses]`
  allow list in `deny.toml` (MIT / Apache-2.0 / Zlib): `nalgebra` 0.35
  (Apache-2.0), `approx`, `simba`, `matrixmultiply`, `rawpointer`, `itertools`,
  `rand_distr`, `glam`, and `safe_arch`/`wide`. `cargo deny check` therefore
  needed no rule change; the "Dependency audit" job is the gate.
- **MSRV:** the newest requirements are `nalgebra`, `safe_arch` and `wide` at
  Rust 1.89, exactly the workspace's declared floor, so the dependency does not
  raise it. The MSRV job re-checks this.
- **Size:** only Canny, Gaussian blur, Otsu and contour geometry are used; the
  text/FFT/display-window features stay off, and the release profile still
  strips and LTOs.

## Direct dependency: rustls + rcgen for LAN sync (C14)

The same-network sync (`crates/synccore/src/lan*.rs`, C14) adds four direct
dependencies to `synccore`:

- **`rustls` 0.23** (MIT/Apache-2.0) with the `ring` provider - the same TLS
  crate `reqwest` already uses for the cloud providers, now also used for the
  pinned self-signed transport. No OpenSSL, no native TLS.
- **`rcgen` 0.13** (MIT/Apache-2.0) generates the device certificate; it was
  already in the tree as a `pdfcore` dev-dependency, so this only makes it a
  normal dependency of one crate.
- **`hmac` 0.12** and **`getrandom` 0.2** for the request signature and the
  pairing/code randomness. Both are already pulled in by `pdfcore`.

No `deny.toml` change was needed: every transitive crate is MIT/Apache-2.0 and
the newest MSRV requirement in the added tree is Rust 1.89 (`safe_arch`/`wide`
via `imageproc`), the workspace floor. `cargo deny check` in the dependency
audit job is the gate.

## Direct dependency: spake2 for code-authenticated pairing (E1/3)

The six-digit pairing code is a weak password, so pairing runs SPAKE2 instead
of sending it (`crates/synccore/src/lan/pairing.rs`). That adds one direct
dependency to `synccore`:

- **`spake2` 0.4** (MIT/Apache-2.0, `#![no_std]`, `forbid(unsafe_code)`) with
  its default `Ed25519Group` parameter set and the `getrandom` feature, so it
  draws its ephemeral scalars from the system RNG.
- It pulls **`curve25519-dalek` 4.1.3** (BSD-3-Clause, already on the
  `deny.toml` allow list) and **`hkdf` 0.12** (MIT/Apache-2.0, already in the
  tree via the OAuth path). `rand_core` 0.6 is the only extra crate version,
  because `spake2` pins the 0.6 line.

`cargo deny check` stays the gate: the license list needed no change, and
`curve25519-dalek` 4.1.3 is the release that carries the RUSTSEC-2024-0344
timing fix, so no advisory exception was added.
