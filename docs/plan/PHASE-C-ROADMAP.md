# Phase C roadmap — closed at v6.12.0

**Status: 100 % delivered.** All fifteen Phase C product directions (C1–C15)
tracked in [#341](https://github.com/ozanstn1-stack/omnioffice/issues/341) are
coded, tested and released between v6.4.0 and v6.12.0; every release shipped
the full 18-asset set with verified SHA-256 manifests. The tracker issue is
closed. The follow-ups that used to be listed here as "remain" are tracked as
Phase D work in [`PHASE-D-PROPOSAL.md`](PHASE-D-PROPOSAL.md) — they are new
work, not open Phase C items.

Phase A closed in v6.0.1 and every Phase B item is merged and closed. The
deeper audit leftovers stay in `AUDIT_REPORT.md` and the honest status of the
current build in `RELEASE_READINESS.md`.

Each item shipped as its own PR with its own release section, and the tracker
checklist was updated when it landed. This document is kept as the delivery
record and as the starting point for Phase D.

## Phase C closure summary

| Release | Phase C work | PR |
|---|---|---|
| v6.4.0 | C1 profession desks | #445 |
| v6.4.1 | C10 pre-send check | #448 |
| v6.4.2 | C2 notification deadline calculator | #450 |
| v6.4.3 | C4 period close audit | #451 |
| v6.4.4 | C11 phone ergonomics (bottom bar, touch targets) | #454 |
| v6.5.0 | C3 evidence bundle, C5 official letter | #484, #487 (release #488) |
| v6.5.1 | C11 follow-up: device-round interface fixes | #489 |
| v6.6.0 | C8 source manifests (PDF), C9 verification screen, C15 report a problem | #498, #494, #496 (release #499) |
| v6.7.0 | C6 template fields + encrypted person cards; Linux packages | #508, #509, #500 |
| v6.8.0 | C13 model-level office diff | #515 (release #516) |
| v6.9.0 | C7 Turkish local AI task packs | #518 |
| v6.10.0 | C12 on-device scanner, filters, batch capture | #520 |
| v6.11.0 | C14 LAN sync first cut (pairing, pinned TLS, transfer, plan) | #522 |
| v6.12.0 | C14 phase 2: mDNS, symmetric pairing, confirmed deletions, Android service | #524 |

Verification at closure: v6.12.0 release with 18 assets and 14 SHA-256 records
checked; CI green on the whole matrix (frontend, lint, Rust on three hosts,
MSRV, Android Kotlin/target, desktop E2E, dependency audit).

### Carried into Phase D (not open Phase C items)

| Follow-up | Where it is tracked now |
|---|---|
| C1 palette task commands (run a desk task from the command palette) | Phase D item D13 |
| C8 DOCX provenance manifests and automatic manifests from producer screens | Phase D item D7 |
| C11 per-screen mobile audit (every screen at 360 dp, one-hand reach) | Phase D item D8 |
| C12 real-photo corpus and acceptance measurement | Phase D item D4 |
| C14 real two-device acceptance, deletion undo history, resumable Android transfer | Phase D items D1–D3 |

## Where Phase C stands

| Item | Status | Evidence |
|---|---|---|
| C1. Profession desks | Delivered in 6.4.0 | `profession-desk.tsx`, `desks.ts`; palette runs navigation/file/plugin commands only |
| C2. Notification deadline calculator | Delivered in 6.4.2 | `DeadlineCalculator.tsx` |
| C3. Evidence bundle | Delivered in 6.5.0 (#484) | `pdfcore/src/evidence.rs` |
| C4. Period close audit | Delivered in 6.4.3 | `Invoices.tsx`, `duplicate-invoices.ts` |
| C5. Official letter | Delivered in 6.5.0 (#487) | `official-letter-template.ts` |
| C6. Smart template form and person cards | Delivered in 6.7.0 | `template-fields.ts` + form, `templates-fields.ts`, encrypted `person_cards.rs` with the cards dialog and CSV/vCard IO |
| C7. Turkish AI task packs, local by default | Delivered in 6.9.0 | `ai-task-packs.ts` (four packs, JSON shapes, verification), the AI packs screen with the local badge and the Ollama switch, and the aicore loopback test |
| C8. Provenance-backed output | Delivered in 6.6.0 (PDF manifests) | `crates/pdfcore/src/provenance.rs`, `pdf_provenance_write/read`; card on the verification screen |
| C9. Verification chain (verification only) | Delivered in 6.6.0 | verification screen over the existing certificate/revocation/LTV modules; trust stays unknown |
| C10. Pre-send check | Delivered in 6.4.1 | `DocumentHealthScreen.tsx`, `document-health.ts` |
| C11. Mobile ergonomics | Delivered in 6.4.4 and 6.5.1 | `bottom-nav.tsx`, touch CSS tests |
| C12. Real scanner | Delivered in 6.10.0 | `pdfcore/src/page_detect.rs` (Canny/Hough detection, Otsu fallback, filters), `office_scan_detect`, auto-crop and batch capture in the scanner screen; the manual picker stays the fallback |
| C13. Make differences visible | Delivered in 6.8.0 | `office-diff-*.ts` engines, `OfficeDiffView`, Compare Office mode and the version-history comparison |
| C14. Office-internal sync | Delivered in 6.11.0, extended in 6.12.0 | pinned-TLS pairing and transfer, three-way plan, sealed store and LAN card; 6.12.0 adds mDNS/DNS-SD, symmetric pairing, confirmed deletion propagation and the Android sharing service. The real two-device pass remains |
| C15. Privacy-preserving feedback | Delivered in 6.6.0 | `report-problem.tsx`, `feedback.ts`; the diagnostics report is copied and the issue pre-filled, nothing is uploaded |

## Delivered items — details kept for reference

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

### C6. Smart template form and person cards — delivered in 6.7.0

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

### C13. Make differences visible — delivered in 6.8.0

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

### C7. Turkish AI task packs, local by default — delivered in 6.9.0

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

### C12. Real scanner — delivered in 6.10.0

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

### C14. Office-internal sync — delivered in 6.11.0 (first cut)

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

### C1 follow-up — palette task commands (moved to Phase D)

C1 delivered the profession desks and the task routing behind them; running a
desk's own task (not only navigation, file and plugin commands) from the
command palette is new work and is tracked as Phase D item D2. It is not an
open Phase C item.

## Release train — delivered

| Release | Contents | Status |
|---|---|---|
| v6.4.0 | C1 | Done |
| v6.4.1 | C10 | Done |
| v6.4.2 | C2 | Done |
| v6.4.3 | C4 | Done |
| v6.4.4 | C11 (first part) | Done |
| v6.5.0 | C3, C5 | Done |
| v6.5.1 | C11 follow-up | Done |
| v6.6.0 | C8, C9, C15 | Done |
| v6.7.0 | C6, Linux packages | Done |
| v6.8.0 | C13, documentation cleanup | Done |
| v6.9.0 | C7 | Done |
| v6.10.0 | C12 | Done |
| v6.11.0 | C14 first cut | Done |
| v6.12.0 | C14 phase 2 | Done |

The train followed the plan: trust features first (the legal and accounting
desks are the differentiator), then the features that remove daily repeated
work, then the highest-risk item last. The Phase D release train is proposed in
[`PHASE-D-PROPOSAL.md`](PHASE-D-PROPOSAL.md).

## Track B — distribution and trust (parallel to Phase C)

- **Linux packaging** — done in 6.7.0 (`.deb`, `.AppImage`, first release with
  all three platforms packaged).
- **F-Droid / IzzyOnDroid** — repository preparation done (#502, #526, #527):
  fastlane metadata and changelogs, catalogue rules, release identity checks,
  and the ready-to-file inclusion request in
  [`docs/izzyondroid-submission-template.md`](../izzyondroid-submission-template.md).
  The submission itself needs the account owner.
- **Windows Authenticode signing** — prepared (#501, #528): a provider-gated
  step in `release.yml` (SignPath, Azure Trusted Signing or PFX), the updater
  signature refresh and the provider decision in [`docs/signing.md`](../signing.md).
  The account/identity validation is owner-only.
- **Documentation and tracker hygiene** — done (#492, #513, #516, #526): the
  README heading, `RELEASE_READINESS` title, tracker checkboxes and this
  roadmap's closure.

## Decisions made during Phase C

1. C9 trust anchors: the OS store on desktop and the platform store on Android;
   an unknown chain is reported as "trust unknown" instead of being presented
   as valid.
2. C7: tutor Ollama only — no model is bundled into the installer; the AI packs
   default to a local provider and say so in the UI.
3. C14 threat model: written in [`docs/lan-sync.md`](../lan-sync.md) (assets,
   actors, pairing, pinning, revocation, residual risks).
4. C12: the detector is proven on a deterministic synthetic corpus; the
   real-photo corpus and the acceptance threshold are Phase D item D1.
5. Person cards (C6): every field optional, KVKK warnings on import/export and
   on the card store; the UI never claims more safety than the platform cipher
   gives.

Open questions for the next phase live in
[`PHASE-D-PROPOSAL.md`](PHASE-D-PROPOSAL.md).

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

At closure every item satisfied these; the checklists are ticked and the
tracker is closed.
