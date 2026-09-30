# Office Swiss Army Knife v3.1.1

**Security and supply-chain patch. No feature changes.**

This patch closes the high-impact findings of the pre-release review: plain
HTTP was accepted for remote AI/WebDAV endpoints, two engine downloads were
not pinned, the Chrome extension context menu ignored the selected PDF, and
the Android WebView held standing grants to shared-storage-looking paths.

## Highlights

- **Transport security enforced.** AI provider and WebDAV endpoints must be
  `https://`. Plain `http://` is accepted only for loopback servers
  (`localhost` / `127.0.0.1` / `::1`); WebDAV additionally requires an
  explicit opt-in in the sync settings. A redirect that downgrades to a
  public `http://` endpoint is refused before credentials (Basic Auth /
  Bearer token) or document bytes can leave the machine. Both policies are
  unit-tested, including a live loopback redirect test.
- **Engine downloads fully pinned.** The Tesseract 4.1.0 source archive and
  the Liberation fonts archive are now in `engines.lock.json`. A missing pin
  is a hard failure (no warning-and-continue), and CI runs `-VerifyLock` to
  prove the lock covers every URL both fetch scripts can download.
- **Chrome extension context menu fixed.** "Open with PDF Swiss Army Knife"
  passes the linked PDF through the URL fragment; the app validates it
  (only absolute `http(s)`), shows a consent prompt, requests the optional
  host permission for that single origin and opens the downloaded document.
  Hostile `javascript:` / `data:` / `file:` links are ignored. Covered by
  unit tests, in-page checks and a headless-Chrome E2E assertion.
- **Android storage narrowed.** The WebView no longer holds a standing grant
  to `$DOCUMENT/**` / `$DOWNLOAD/**`; PDF/office intermediates live in
  app-private data/cache, AI library files moved to app-private storage, and
  every user-visible file goes through SAF. The open-with intent pipeline
  (untrusted display names, extension whitelist, 256 MB copy cap) gained JVM
  unit tests run by the Android release workflow.
- **OS integration hardened.** Opening or revealing a file no longer uses the
  opener plugin permission from the webview; validated Rust commands accept
  only existing documents with extensions the app itself produces.
- **Streaming WebDAV transfers.** Uploads and downloads hash while streaming
  instead of buffering up to 512 MB; downloads stage into a temporary sibling
  and are only promoted after the conflict check passes.
- **Smaller startup and guarded maintenance.** Screens are lazy-loaded with a
  CI bundle budget (entry chunk 315 KB → 155 KB gzip), the ESLint gate is a
  per-rule baseline (four accessibility rules promoted to error), the Chrome
  self test runs in CI, and every GitHub Action is pinned to a commit SHA.

## Compatibility

No document format, API or settings file changes. Existing AI/WebDAV settings
that point at a public `http://` endpoint are refused with a clear message on
save or first use; switch the endpoint to `https://` (or a loopback address
with the opt-in) to continue.

## Verification

- `cargo test -p synccore -p aicore` (including the new streaming and
  redirect-refusal tests), `cargo clippy -- -D warnings` for the changed
  crates and the Tauri crate.
- `npm test` (587 tests), `npx tsc --noEmit`, `npm run lint` (per-rule
  baseline, zero errors), `npm run i18n:audit`, `npx vite build` +
  `npm run bundle:check`.
- Chrome extension: `npm run test`, `npm run build`, `npm run verify` and the
  headless-Chrome self test (14/14 checks plus the deep-link assertions).
- `pwsh -File scripts/fetch-engines.ps1 -VerifyLock` and the Android variant.
