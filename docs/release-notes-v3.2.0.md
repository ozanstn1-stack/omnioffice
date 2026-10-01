# Office Swiss Army Knife v3.2.0

**Quality gates and test confidence. No feature changes.**

This release raises the confidence of the code base itself: the lint warning
backlog is gone, the WebDAV and Android intent pipelines have real end-to-end
tests, and the RustSec advisory exceptions are owned and time-boxed.

## Highlights

- **Zero-warning lint, every rule an error.** The React Compiler diagnostics
  from `eslint-plugin-react-hooks` v7 (`set-state-in-effect`, `refs`,
  `immutability`, `purity`, `use-memo`, `preserve-manual-memoization`,
  `globals`, `exhaustive-deps`) and the `jsx-a11y` interaction rules were
  reduced from 143 warnings to zero and promoted to errors. The per-rule
  warning budget is removed; `npm run lint` is a plain `eslint .` that fails
  the build on any warning.
- **Real fixes, not just allowances.** State keyed to the active document is
  now derived during render instead of being cleared in an effect (readers,
  PDF Studio, Redact, Metadata, AI, the spreadsheet/slide/word editors);
  drag-highlight and preview state replace ref reads during render; and
  impure calls (`Date.now()`) moved out of render into event handlers.
- **Accessibility pass.** Keyboard handlers, roles and `aria-label`s were
  added across the editors and screens: spreadsheet grid/sheet tabs, slide
  thumbnails and stage, writer paragraphs and image captions, modal
  backdrops, data/form tables. A `.sr-only` helper labels icon-only controls.
- **WebDAV end-to-end tests.** A self-contained in-process DAV server
  (PROPFIND/MKCOL/PUT/GET/DELETE with conditional writes) drives the real
  `WebDavProvider` through listing, streaming upload/download, deletion,
  HTTP 412 → `Conflict` mapping and `Depth: 1` listing assertions.
- **On-device Android intent tests.** The open-with / share pipeline now runs
  on an emulator in CI against a real `content://` provider
  (`TestDocumentProvider`): untrusted display names, the extension
  whitelist, the 256 MB copy cap and the cache hand-off are all verified
  through the same `IncomingFiles.copyToCache` the Activity uses.
- **Owned, time-boxed advisory exceptions.** `deny.toml` records an owner and
  a review date for every ignored RustSec advisory, and CI enforces the policy
  with `cargo deny check advisories`.

## Verification

- `npm run lint` (0 problems, all rules error), `npm test` (587 tests),
  `npx tsc --noEmit`, `npm run i18n:audit`, `npx vite build` +
  `npm run bundle:check`.
- `cargo test -p synccore -p aicore` (including the new WebDAV E2E suite),
  `cargo clippy -- -D warnings`, `rustfmt`.
- Android: `./gradlew :app:testUniversalDebugUnitTest` (JVM) and
  `./gradlew :app:connectedUniversalDebugAndroidTest` (emulator), both run by
  the Android release workflow.
- CI: `cargo audit` and `cargo deny check advisories`.
