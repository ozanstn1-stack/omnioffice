# Phase C roadmap — after 6.5.1

Living plan for the remaining Phase C product directions tracked in
[#341](https://github.com/ozanstn1-stack/omnioffice/issues/341). Phase A closed
in v6.0.1 and every Phase B item is merged and closed, so this document covers
what is left of Phase C only; the deeper audit leftovers stay in
`AUDIT_REPORT.md` and the honest status of the current build in
`RELEASE_READINESS.md`.

Estimates assume one developer plus AI assistance and are given in working
days. They are estimates, not commitments. Each item ships as its own PR with
its own release section; the issue checklist is updated when an item lands.

## Where Phase C stands

| Item | Status | Evidence |
|---|---|---|
| C1. Profession desks | Delivered in 6.4.0; palette task commands remain | `profession-desk.tsx`, `desks.ts`; palette runs navigation/file/plugin commands only |
| C2. Notification deadline calculator | Delivered in 6.4.2 | `DeadlineCalculator.tsx` |
| C3. Evidence bundle | Delivered in 6.5.0 (#484) | `pdfcore/src/evidence.rs` |
| C4. Period close audit | Delivered in 6.4.3 | `Invoices.tsx`, `duplicate-invoices.ts` |
| C5. Official letter | Delivered in 6.5.0 (#487) | `official-letter-template.ts` |
| C6. Smart template form and person cards | Open | `office/templates.ts`, `MailMerge.tsx` exist; no form/person layer |
| C7. Turkish AI task packs, local by default | Partial | Ollama provider preset exists; no task packs or local-default flow |
| C8. Provenance-backed output | Delivered in 6.6.0 for PDFs; DOCX and automatic producers remain | `crates/pdfcore/src/provenance.rs`, `pdf_provenance_write/read`; card on the verification screen |
| C9. Verification chain (verification only) | Delivered in 6.6.0 | verification screen over the existing certificate/revocation/LTV modules; trust stays unknown |
| C10. Pre-send check | Delivered in 6.4.1 | `DocumentHealthScreen.tsx`, `document-health.ts` |
| C11. Mobile ergonomics | Delivered in 6.4.4 and 6.5.1; per-screen audit remains | `bottom-nav.tsx`, touch CSS tests |
| C12. Real scanner | Partial | Camera capture and manual four-corner picker only; no automatic edge detection |
| C13. Make differences visible | Partial | PDF Compare exists (`pdfcore/src/compare.rs`); no office-document or version-history comparison |
| C14. Office-internal sync | Partial | WebDAV/Drive/OneDrive sync and `synccore/src/merge.rs`; no same-network device pairing |
| C15. Privacy-preserving feedback | Delivered in 6.6.0 | `report-problem.tsx`, `feedback.ts`; the diagnostics report is copied and the issue pre-filled, nothing is uploaded |

## Open and partial items

### C9. Document verification screen — delivered in 6.6.0

**Goal.** One screen that answers "is this document real, and what changed
since it was signed?" for signed PDFs.

**Scope.** Number of signatures, signer, signing time, whether the document was
modified after signing (incremental-update aware), certificate chain, timestamp,
LTV coverage, PDF/A and encryption state. A short "verification report" that can
be saved as PDF (shared with C8).

**Approach.** UI over the existing `certificate`, `revocation`, `ltv`,
`security` and `evidence` modules. Most of the work is trust-anchor policy and
honest wording, not new crypto.

**Decisions needed.** Trust roots: operating-system store on desktop and a
pinned root list on Android; an unknown chain is reported as "trust unknown"
instead of being presented as valid (consistent with the current limitation).

**Acceptance.** A PDF modified after signing fails verification with the reason;
an unsigned PDF says "no signature"; a valid test signature shows signer and
time; both UI languages.

### C8. Provenance-backed output — delivered in 6.6.0 for PDFs

**Goal.** Every generated PDF/DOCX can prove where it came from.

**Scope.** A manifest embedded in the output: source documents with SHA-256,
generation time, producing module, and the AI provider/model and citations when
AI was involved. XMP plus an optional visible "Source" footer for PDF; a custom
XML part for DOCX. Reopening the document shows the manifest; a hash mismatch is
reported.

**Approach.** Reuse the embed-probe-strip-hash pattern already used by
`evidence.rs` and the redaction certificate. Never rewrite a document that
already carries a signature — rewriting breaks it.

**Acceptance.** A generated document reopened in the app shows its sources and
hashes; a modified manifest is detected; signed inputs are refused with an
explanation.

### C6. Smart template form and person cards — M, 2–3 days

**Goal.** Turn a template with `{{name}}`-style variables into a form, and keep
the people you fill it for.

**Scope.** Variables (`{{name}}`, `{{date}}`, …), a form view, preview and
PDF/DOCX output built on `office/templates.ts` and the Petition/OfficialLetter
wizard patterns. Person cards: a local address book (name, VKN/TCKN, address,
phone, e-mail) that feeds MailMerge and the official letter, with CSV/vCard
import and export behind an explicit personal-data warning.

**Approach.** Cards are personal data: store them encrypted with the same
DPAPI/Keystore mechanism used for saved signatures, and exclude them from
backups unless the user opts in.

**Acceptance.** One template produces a 20-person mail merge in one action; add,
edit and delete a card; export requires confirmation; both UI languages.

### C13. Make differences visible — M, about 2 days

**Goal.** See what changed between two versions of an office document.

**Scope.** Model-level diff for Writer paragraphs, Calc cells and Impress
slides; a "Compare with current" action on History snapshots; office documents
in the existing Compare screen (PDF visual diff already exists).

**Approach.** Diff the `officecore` model rather than rendered output, so the
result works on large documents and can say "paragraph 12 changed" instead of
"page 3 differs". Golden-file tests for the diff output.

**Acceptance.** A Writer document with three changed paragraphs highlights them;
a Calc range `Sheet1!A1:B7` lists the changed cells; the result opens from
History.

### C7. Turkish AI task packs, local by default — M–L, 3–5 days

**Goal.** Ready-made Turkish task packs that keep document text on the machine
by default.

**Scope.** Task packs such as "düzelt dilekçe", "özetle sözleşme maddesi",
"çıkar KDV özeti", "tebligattan süre çıkar", each with a prompt set, expected
output shape and a verification step. A visible "local" badge; the Ollama
preset is the default local path so no model is bundled into the installer.

**Approach.** Prefer tutoring Ollama over embedding a model: the installer stays
small and the license question stays with the user. If a model is ever bundled,
pin it in `engines.lock.json` like the other engines. Tests run against the
mock provider (`scripts/mock-deepseek.mjs`).

**Risks.** Model license and size, device RAM, Turkish output quality.

**Acceptance.** Four task packs; local mode makes no network request (asserted by
the existing netpolicy test); both UI languages.

### C12. Real scanner — M, 3–4 days

**Goal.** Detect the page automatically instead of asking for four corners by
hand.

**Scope.** Grayscale → blur → Canny → largest quadrilateral → perspective
correction, with page separation, a black-and-white filter and batch capture.
The existing manual picker stays as the fallback and correction surface. No
Google Play services; ML Kit stays out for the reason recorded in 6.3.2.

**Approach.** Rust `image` plus `imageproc` (new dependencies; update
`deny.toml`); the algorithm runs on a downscaled copy and the corners are
applied to the full-resolution photo.

**Risks.** Quality on poor lighting and busy backgrounds; needs a small real-photo
corpus as a regression set.

**Acceptance.** At least 80 % of the corpus gets a usable quadrilateral without
manual correction; manual correction still works; both UI languages.

### C14. Office-internal sync — L, 5–8 days

**Goal.** Move documents between the office PC and phone on the same network
without OAuth or a cloud account.

**Scope.** Same-Wi-Fi discovery (mDNS), six-digit pairing code plus pinned TLS,
transfer of office documents, and `synccore/merge.rs` for conflicts. Manual
conflict resolution stays, as it is elsewhere.

**Approach.** Threat-model first: what a device on the same network may request,
how pairing is revoked, what the phone's background service may do. Then the
transport, then the UI.

**Risk.** Highest of the set: discovery differs by platform, and Android
background limits shape the design.

**Acceptance.** Two devices on the same network pair once and sync a folder;
a conflicting change shows the existing manual resolution; unpair revokes
access.

### C15. Privacy-preserving feedback — delivered in 6.6.0

**Goal.** A feedback path that never leaks document content.

**Scope.** Settings → "Report a problem": a local diagnostic bundle (version,
device class, crash log, job log; never document content), a preview of exactly
what it contains, and a button that copies it or opens a pre-filled GitHub
issue. No automatic upload.

**Acceptance.** The bundle contents are listed before sending; no network request
without an explicit action; both UI languages.

### C1 remainder — palette task commands — S, 1–2 days

**Goal.** The command palette performs tasks, not only navigation.

**Scope.** Commands such as "redact this PDF", "extract the VAT summary", "build
an evidence bundle", registered through the existing `registerCommand` registry
and enabled by context. `App.tsx` today registers only `view.*`, `file.open`,
`app.commandPalette` and `app.globalSearch`.

**Acceptance.** The palette lists the task commands with their shortcuts; a
disabled command says why; plugin commands keep working.

## Proposed release train

| Release | Contents | Effort |
|---|---|---|
| v6.6.0 "Verify" | C9, C8, C15 | Done in 6.6.0 (C8: PDF manifests only) |
| v6.7.0 "Templates" | C6, C1 remainder | 3–4 days |
| v6.8.0 "Compare" | C13, tracker and README/RELEASE_READINESS cleanup | 2–3 days |
| v6.9.0 "Local AI" | C7 | 3–5 days |
| v6.10.0 "Scanner" | C12 | 3–4 days |
| v7.0.0 "Sync" | C14 | 5–8 days |

The order is deliberate: trust features first (the legal and accounting desks
are the differentiator), then the features that remove daily repeated work,
then the highest-risk item last. Release numbers are a proposal and follow the
pace of the work, not the other way around.

## Track B — distribution and trust (parallel to Phase C)

- **Windows Authenticode signing** — the installer still warns that the app is
  unsigned. SignPath.io (free for open source) or Azure Trusted Signing; 1–2
  days plus the provider's identity validation. Highest impact per day of work.
- **Linux packaging** — Desktop CI already builds and tests all three
  platforms but packages only Windows. `.deb` and `.AppImage` are mostly free
  once the workflow is extended (~1 day).
- **F-Droid / IzzyOnDroid** — the local-first, no-Google profile fits; ~1 day of
  preparation plus review time. No Play Services dependency to remove.
- **Documentation and tracker hygiene** — #341 checkboxes for the delivered
  items, the README "What's new" heading, the RELEASE_READINESS title, and the
  stale Phase B note (~0.5 day, this PR).

## Open decisions

1. C9 trust anchors: OS store on desktop and a pinned root list on Android, or a
   single bundled root list everywhere?
2. C7: tutor Ollama only, or also bundle a small model pinned in
   `engines.lock.json`? The first keeps the installer small; the second makes
   "local by default" work without installing anything else.
3. C14 threat model: what a paired device may read and write, and how pairing is
   revoked.
4. C12: acceptance threshold and the size of the real-photo regression corpus.
5. Person cards (C6): which fields are mandatory under KVKK, and what the export
   warning must say.

## Definition of done, per item

An item is done only when all of these are true:

- Tests cover the change (Rust and/or Vitest), with a red test first for fixes.
- `src/lib/i18n/en.json` and `tr.json` both carry the new strings.
- `CHANGELOG.md` has the release section; the README "What's new" and download
  links follow the release; `RELEASE_READINESS.md` carries the `test:<file>#<name>`
  markers that prove the claim.
- The released assets are verified against the list in `docs/plan/AI-HANDOFF.md`
  (Setup .exe, Portable .zip, two APKs, two AABs, `latest.json` and the SHA-256
  manifests).
- The #341 checklist entry is ticked.
