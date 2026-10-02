# Roadmap

Living plan for the releases after 3.3.x. Each phase is independently
shippable; the detailed audit leftovers live in `AUDIT_REPORT.md` and the
honest status of the current build in `RELEASE_READINESS.md`.

Effort estimates assume a single developer.

| Phase | Scope | Status |
|---|---|---|
| 0 | Docs consistency, `compat.rs` freshness contract, frontend coverage baseline, roadmap | Done in 3.4.0 |
| 1 (v3.4) | Quick wins: command palette + keybindings, Settings completion, Home IA, a11y pass, dead ends | Done in 3.4.0 |
| 2 (v3.5) | Quality infrastructure: coverage thresholds, screen tests, desktop E2E, fuzzing, benchmark trends, CI hardening | Partial: coverage gate + 24 screen tests in 3.5.0; E2E, fuzzing, benchmark trends remain |
| 3 (v3.6) | PDF depth: content-stream object editing, render cache, Reader, Studio UX, PDF/A subsetting, signature chain (RFC 3161/OCSP) | Planned |
| 4 (v3.7) | Office depth: Writer fields/revisions, Calc data tools + chart UI, Impress timeline + media, format fidelity | Planned |
| 5 (v4.x) | Platform: auto-update, macOS/Linux packaging, stores, Android foreground service + Keystore, sync OAuth, Vault 2.0, plugins, AI | Planned |

## 3.4.0 — Quick wins (delivered)

- **Command platform is real.** Every screen is a palette command; the
  registry drives the keybindings (`matchKeybinding`), the former hard-coded
  shortcut handler is gone, and `Ctrl+O` routes office documents to the
  workspace and everything else through the active screen.
- **Settings**: `midnight`/`paper` themes, office default formats, version
  history and import-warning toggles that actually reach the save/open paths,
  corrected shortcut list with an editor-shortcut hint.
- **Home**: the drop suggestion keeps every dropped file, the tool grid is
  searchable and grouped (all 42 screens reachable), recent PDFs open in the
  Reader, and the reveal button is a real labeled control.
- **Accessibility**: `aria-current` navigation, modal focus trap + focus
  restore, roving tabindex and arrow keys in segmented controls, live region
  for background jobs, `lang` follows the UI language, translated control
  labels, `prefers-reduced-motion` support.
- **Calc**: `CUMIPMT`/`CUMPRINC` implemented with Excel reference values.
- **Quality**: frontend coverage runs in CI; `officecore` has a contract test
  that keeps the Compatibility Center honest about DSS vs. RFC 3161.

## 3.4.0 — Not done (honest)

- Coverage is informational, not a blocking threshold yet. (Closed in 3.5.0.)
- Desktop E2E, fuzzing and benchmark trends are still Phase 2.
- Jobs "Retry" has no registered handler yet; `vault_clear` has no UI path.
- `AUDIT_REPORT.md` items not touched by 3.4.0 remain as listed there.

## 3.5.0 — Coverage gate and screen tests (delivered)

- `npm run test:coverage` enforces statement/branch/function/line floors
  (58/68/46/58) measured from the suite; the frontend CI job runs it.
- 24 new tests cover Merge, Split, Compress, Security, Watermark, Metadata,
  Info, History, Home, Settings, Notes, Planner, Data, Draw, Templates,
  Converter, Cleaner, PDF forms, AI Library, Jobs, Compatibility, OCR and
  PDF→images; statements moved 49.2 % → 60.7 %.
- Templates contract test for ids, metadata and model kind.

## 3.5.0 — Not done (honest)

- Desktop E2E (tauri-driver or a mocked-invoke browser suite) and fuzzing are
  still not started; benchmark trend tracking is not started either.
- The coverage floors keep a few points of headroom and should be raised as
  the suite grows.
