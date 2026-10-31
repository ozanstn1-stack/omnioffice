# Command ACL — the webview's IPC allow list (E1/4b)

Every tool in OmniOffice is a Tauri command: the frontend calls it over IPC and
the Rust backend does the work. Until 7.2 that surface was open — any script
running in the webview could invoke all 249 registered commands, and the only
gate was the Rust-side validation inside each one (path tokens, size caps).

Tauri v2 has an application-level ACL, but it is **off** until the app declares
an ACL manifest. That is the switch this slice flips:

| Piece | Where | What it does |
|---|---|---|
| Permissions | `src-tauri/permissions/*.toml` (generated) | one `allow-<domain>-commands` permission per IPC domain, listing that domain's commands |
| Manifest | `src-tauri/build.rs` (`AppManifest::new()`) | makes Tauri read the permission files and enforce them |
| Capability | `src-tauri/capabilities/default.json` | grants the eight permissions to the `main` window |
| Generator | `scripts/command-acl.mjs` (`npm run acl:sync` / `acl:check`) | derives the permission files from `src-tauri/src/ipc.rs` |
| Compiled check | `src-tauri/src/command_acl.rs` | reads the build's `allowed-commands.json` and fails when a registered command is missing |
| Runtime check | `e2e/smoke.mjs --acl` | drives the real IPC bridge: an unknown command is refused, `app_info` still answers |

## Why the manifest comes from the registry

`src-tauri/src/ipc.rs` is the single list of registered commands: eight
`domain!` blocks, each generating the domain's `names()` and its
`generate_handler!`. The generator parses those blocks, so:

- a command that is not in a domain list cannot be invoked (it is not
  registered), and
- a command added to a domain list shows up in the permission file on the next
  `npm run acl:sync`.

The generated files are committed. `npm run acl:check` runs in the Lint job and
fails when they are stale, when a permission file has no domain any more, or
when a permission is not granted by an unconditional capability (a permission
nobody references would deny its whole domain at runtime).

The command list deliberately ignores `#[cfg(...)]` attributes: one stable file
set covers every target, and declaring a permission for a command that only
exists on Android is harmless — permissions are data, and the runtime allow list
is keyed by name.

## What it actually closes

Once the manifest exists, Tauri denies **any** application command that no
capability allows, for local origins as well (`tauri::Webview::on_message`):
the check happens before the command body runs, so an injection in the webview
cannot reach backend code that the UI does not use. Combined with the CSP
(E1/1) and `freezePrototype`, a compromised renderer now has to work inside the
granted surface instead of the whole backend.

The eight permissions mirror the domain split, so the reviewable unit is "this
domain may run", not 249 individual grants:

| Permission | Commands | Examples |
|---|---|---|
| `allow-app-commands` | 45 | settings, files, jobs, updates, diagnostics, platform bridges |
| `allow-pdf-commands` | 77 | reading, page ops, conversion, redaction, V3 studio, signing, recipes |
| `allow-office-commands` | 46 | Writer/Calc/Impress, invoices, office side tools |
| `allow-ai-commands` | 31 | assistant, editor actions, AI library |
| `allow-vault-commands` | 9 | local document vault and its index |
| `allow-sync-commands` | 15 | WebDAV sync and OAuth providers |
| `allow-lan-commands` | 16 | LAN identity, host, peers, transfers |
| `allow-plugin-commands` | 10 | plugin runtime and its sandboxed host calls |

## Limits (honest list)

- **Domain granularity.** One grant per domain means a compromised webview can
  still call every command in a domain the app uses. Splitting further is
  possible (one permission per command, generated the same way) but each split
  multiplies the capability file; the domain is the seam the code already has.
- **Not a user boundary.** The ACL separates the webview from the backend. A
  local process running as the same user can talk to the app's files directly;
  the protection there is the secret store (E1/4) and the path policy (E1/2).
- **Plugins are unchanged.** Plugin host calls were already capability-scoped;
  they now sit under `allow-plugin-commands` like everything else.
- **Remote origins stay denied.** No capability carries a `remote` URL pattern,
  so even if the app ever loaded remote content it could not reach a command.
- **The compiled list is per profile.** `src-tauri/src/command_acl.rs` reads the
  artifact the same build wrote, so `cargo test` and the shipped binary agree;
  a stale `target/` cannot vouch for a different command set because the build
  script re-runs when `permissions/` changes.
