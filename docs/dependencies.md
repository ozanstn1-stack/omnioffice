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

`vendor/calamine` is a verbatim copy of crates.io 0.37.0 with one change:
`cell_ref_to_row_col` uses `checked_mul`/`checked_add` so a hostile cell
reference (`AAAAAAAA...`, a giant row number) is a parse error instead of an
integer-overflow panic. cargo-fuzz builds with overflow checks, so the panic
aborted the `office_readers` target (the app's worker would have caught it, but
the fuzz gate must stay green for releases).

Both `Cargo.toml` files patch the crate (`[patch.crates-io]`). Remove the
directory and the patches when calamine ships the same guard upstream
(`vendor/calamine/README.md` has the details). The rustfmt gate skips
`vendor/` because it is third-party source.
