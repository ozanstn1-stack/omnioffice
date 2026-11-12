# Architecture

OmniOffice is a [Tauri 2](https://tauri.app) application: a React + TypeScript interface in the
system webview, and a Rust backend that does all file work. The same code builds the Windows and
Linux desktop apps and the Android app. This page is the map; the linked documents hold the
details.

```text
┌─────────────────────────── webview (src/) ───────────────────────────┐
│ screens/  office/ (Writer, Calc, Impress)  components/  lib/          │
│        typed command calls (src/lib/generated/specta.ts)              │
└───────────────────────────────┬──────────────────────────────────────┘
                                │ Tauri IPC (command ACL, file tokens)
┌───────────────────────────────┴──────────── src-tauri/ ──────────────┐
│ commands, jobs and progress, file-token registry, worker process,     │
│ AI, vault and search index, LAN/cloud sync, updater, Android bridges   │
└──────┬──────────┬───────────┬──────────┬──────────┬──────────┬───────┘
   pdfcore   officecore    aicore    synccore    turkish     compat
```

## Rust crates (`crates/`)

All library crates are `#![forbid(unsafe_code)]` and refuse `unwrap`/`expect` in production code:
they parse untrusted documents and must return errors, not unwind.

| Crate | What it does |
| --- | --- |
| `pdfcore` | PDF tools: merge, split, organize, compress, OCR pipeline, annotate and flatten, redact, sign and verify, compare, N-up imposition, PDF/A, forms. Uses pdfium and qpdf (`npm run engines:fetch`). |
| `officecore` | Office formats and model: DOCX, XLSX, PPTX, ODF (ODT/ODS/ODP), RTF, CSV, EML, UDF, UBL e-invoices; the formula engine, pivot tables, layout and PDF export. |
| `aicore` | AI providers (OpenAI-compatible, Anthropic, Gemini, Ollama ...): chat, streaming, embeddings, prompts and citations. |
| `synccore` | Opt-in folder sync: WebDAV and OAuth cloud providers, LAN sync, merge of conflicting edits. Never goes online on its own. |
| `turkish` | Offline Turkish spell checking (Zemberek lexicon, suffix analysis, suggestions) and Turkish casing. |
| `compat` | Scores the compatibility corpus ([`docs/corpus.md`](corpus.md)). |

`src-tauri/` is the application crate: the Tauri commands (`commands.rs`, `office.rs`, `ai.rs`,
`vault.rs`, ...), background jobs with progress and cancellation (`jobs.rs`), and the platform
glue for Android (`android_*.rs`).

## Security boundaries

- **No raw paths in the webview.** A native picker registers the chosen file in the Rust token
  registry and the interface only sees an opaque token
  ([`docs/file-tokens.md`](file-tokens.md)).
- **Command allow list.** The webview can call only the commands on the IPC ACL
  ([`docs/command-acl.md`](command-acl.md)).
- **Untrusted parsers run in a worker process.** DOCX, XLSX, PPTX, ODF, UDF, RTF, UBL, EML parsing
  and PDF repair run in a short-lived, memory-limited child process with a time limit
  (`src-tauri/src/worker.rs`), so a malformed file cannot take the app down.
- **Network only on request.** Cloud sync, the updater and AI with your own key are the only
  features that go online; secrets are kept in the OS keyring (`src-tauri/src/secret/`).
- Release provenance and signing: [`docs/provenance-v2.md`](provenance-v2.md),
  [`docs/signing.md`](signing.md).

## Frontend (`src/`)

| Folder | What it holds |
| --- | --- |
| `screens/` | One file per tool or hub (Home, Tools, PDF tools, AI, Vault, Settings, ...). Screens are loaded lazily through the screen registry. |
| `office/` | The editors: `WriterEditor.tsx`, `CalcEditor.tsx`, `ImpressEditor.tsx`, with their parts in `office/writer/`, `office/calc/` and `office/impress/`. Calc's formula engine runs in TypeScript (`office/calc/`). |
| `components/` | Shared UI: layout, dialogs, the command palette, file pickers, the recent-files table. |
| `lib/` | State stores (`store.ts`, Zustand), the API layer (`api.ts`, `typed-call.ts`), i18n (`i18n/en.json`, `i18n/tr.json`), formats and helpers. |
| `lib/generated/` | `specta.ts`: TypeScript bindings generated from the Rust commands (`npm run bindings:specta`); never edited by hand. |

Commands are called through the generated, typed wrappers; long jobs go through
`trackedRequest`, which records them so the Jobs screen can show progress and retry them.

## Tests and quality gates

- Rust: `cargo test --workspace` (unit tests next to the code, integration tests in each crate's
  `tests/`), Clippy with warnings as errors, fuzzing of the parsers (`fuzz/`).
- Frontend: Vitest (`*.test.ts(x)` beside the code), ESLint, Prettier, the i18n audit.
- End to end: `e2e/` drives the built desktop app (WebView2 on Windows, tauri-driver on Linux).
- Ratchets: file sizes only shrink (`scripts/file-size-baseline.json`), the bundle has a budget
  ([`docs/performance-budgets.md`](performance-budgets.md)), and every claim in
  `RELEASE_READINESS.md` names the test that proves it.

How to run all of these is in [`CONTRIBUTING.md`](../CONTRIBUTING.md).
