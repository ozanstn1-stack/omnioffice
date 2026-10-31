# Phase D proposal — after Phase C

**Status: approved; v6.13.0–v7.0.0 delivered (D1–D9); v7.1.0 in progress (D13 delivered, D10 preparation landed).** Phase C is closed (all C1–C15 delivered between
v6.4.0 and v6.12.0, tracker #341 closed, every release verified with its
18-asset SHA-256 manifest). This document proposes what comes next; nothing
here is committed until the owner picks the items and a tracker issue is
opened.

## Progress

| Item | Status |
|---|---|
| D1. Real two-device LAN acceptance | Delivered in v6.13.0 (contract test and device guide; the manual pass records in the release notes) |
| D2. Deletion undo and history | Delivered in v6.13.0 |
| D3. Resumable Android transfer | Delivered in v6.13.0 (uploads; downloads stay whole-file) |
| D4. Real-photo scanner corpus and page separation | Delivered in v6.14.0 (photo-realistic corpus, measured 12/14 = 85 %, spread split and batch auto-crop) |
| D8. Mobile ergonomics audit | Delivered in v6.15.0 (thumb-reach action bar, keyboard inset, safe areas, landscape, bilingual 360 dp matrix for 55 routes) |
| D7. Provenance v2 | Delivered (DOCX manifests, unified source chain, automatic producer embedding) |
| D5. Local AI v2 | Delivered (eval harness, three new Turkish packs, strict JSON + repair, zero-network isolation, bundling decision) |
| D6. Office fidelity | Delivered for the measured scope (DOCX tracked changes, XLSX shared formulas, PPTX transition duration, round-trip corpus with a zero delta; images/charts/dynamic arrays remain open) |
| D9. Performance and memory | Delivered (500-page DOCX and 50k-cell XLSX budget tests, memory ceilings, calc virtualization, PDF worker isolation, named budgets in `docs/performance-budgets.md`) |
| D13. Command palette task commands | Delivered in v7.1.0 (ten desk tasks with shortcuts, file context, disabled reason; plugin registry unchanged) |
| D10. First signed Windows release | Preparation delivered (preflight, runbook, readiness gate); signed artifact pending the SignPath account and secrets (owner action) |

The rest of this document is the original proposal, kept as the planning record.

## Starting point (v6.12.0)

- Windows, Linux (`.deb`, `.AppImage`) and Android (arm64-v8a, armeabi-v7a) are
  shipped from one CI line; Windows installer stays unsigned until a
  Authenticode provider account exists (#501, prepared in #528).
- Android catalogue preparation is done (#502, #526, #527): fastlane metadata,
  changelogs, release identity checks and the ready-to-file IzzyOnDroid
  request; the submission and the F-Droid decision need the account owner.
- LAN sync works (pairing, pinned TLS, mDNS, three-way plan, confirmed
  deletions, Android foreground service) but has not been accepted on real
  device pairs yet.
- The local AI packs, the on-device scanner, the model-level office diff and
  the provenance/verification screens exist; their follow-ups are listed below.
- Honest state of the build: `RELEASE_READINESS.md`. Deeper leftovers:
  `AUDIT_REPORT.md`.

## Principles (unchanged)

- Local-first and private: no account, no telemetry, no ads; network only when
  the user configures it.
- No silent overwrite: conflicts and deletions are always a user decision.
- Honest claims: every `Implemented` claim carries a test marker; limitations
  are listed, not hidden.
- Small versioned PRs, each with tests, i18n, docs and a release section; heavy
  builds and tests run in CI, not on the VPS.
- Measured acceptance: features that need a corpus or a device pair name the
  measurement up front.

## Workstream A — harden what shipped

### D1. Real two-device LAN acceptance (C14 follow-up)

**Scope.** A repeatable acceptance matrix: Windows↔Android, Android↔Android,
same Wi-Fi and guest network; mDNS discovery, pairing with the six-digit code,
listing/fetch/push up to the 512 MiB cap, a conflict resolved three ways, a
confirmed deletion on each side, unpair on either side, app backgrounded
during a transfer.

**Approach.** A device checklist in `docs/lan-sync.md` (or its own
`docs/lan-sync-acceptance.md`) plus a small two-device harness script that
drives the plan through the Rust API on loopback-simulated peers where
possible. Real-device runs are manual and recorded in the release notes.

**Acceptance.** All matrix combinations pass; every failure is written down
with its workaround; the threat-model document points at the results.

### D2. Deletion undo and history (C14 follow-up)

**Scope.** The deletion confirmation currently ends in a permanent delete.
Add a local trash for confirmed deletions (sealed store + files under the app
data dir, 30-day retention), a "Recently deleted" screen with restore, and a
tombstone history per peer so a re-sync does not resurrect or re-delete files
unexpectedly.

**Acceptance.** A confirmed deletion can be restored from the app within the
retention window; after retention it is gone with a clear explanation; a
re-pair does not replay old tombstones.

### D3. Resumable Android transfer (C14 follow-up)

**Scope.** Chunked transfer with offsets and per-chunk SHA-256, resume after
the foreground service is stopped/restarted, progress in the notification,
cancellation that cleans partial files, and a memory cap for large files.

**Approach.** Extend the framed protocol with a `begin`/`chunk`/`finish` shape
(the current `put` sends the whole body). Keep the three-way plan and the
conflict policy untouched.

**Acceptance.** A transfer survives a background service restart and completes
after resume; an interrupted transfer leaves no partial document in the shared
folder; the cap and cancellation are tested.

## Workstream B — v7 local capabilities

### D4. Real-photo scanner corpus and page separation (C12 follow-up)

**Scope.** A committed corpus of real photos (various lighting, backgrounds,
angles, double-page shots), a golden test that reports the share of usable
quads, automatic page separation for two-page captures, and batch auto-crop.

**Acceptance.** At least 80 % of the corpus gets a usable quadrilateral without
manual correction; double-page shots split into two pages; the acceptance
number is published in `RELEASE_READINESS.md`.

**Status (v6.14.0).** Delivered in #531 with a photo-realistic synthetic corpus
(real photos cannot be committed: privacy and size), 14 images covering
perspective, rotation, dark/light desks, shadow gradients, warm/cool light,
vignetting, paper texture, defocus, edge crop and three spreads. Measured
**12/14 usable (85 %)** against the 80 % target; every spread splits into two
pages. The two failing light-desk/shadow photos are documented rather than
hidden — see `docs/scanner-corpus.md`. Batch auto-crop now detects all captured
photos in one `office_scan_detect_batch` call and the scanner screen shows a
two-page capture as two page rows. The real-camera pass stays manual in the D1
device matrix.

### D5. Local AI v2

**Scope.** Decide whether a tiny model may be bundled (size, licence, device
RAM, update story) or Ollama stays the only local path; a Turkish prompt/eval
harness with golden outputs and a regression score per task pack; more packs
(e-invoice, KEP/tebligat, court petition); strict JSON retries and a latency
budget.

**Acceptance.** Every pack passes the eval harness on the local provider; the
decision about bundling a model is recorded with its trade-offs; no pack makes
a network call in local mode.

**Status (v7.0.0).** Delivered: seven Turkish packs with golden and negative
eval cases (the suite scores 100 % on the goldens and fails under the 90 %
floor when the model degrades), one repair turn on invalid JSON, a test proving
the harness and runner make no network/telemetry call, and
`docs/local-ai-decision.md` with the bundling decision (no bundled model in
v7.0.0) and the criteria to revisit it.

### D6. Office fidelity

**Scope.** DOCX tracked-changes import (show revisions instead of flattening
them), deeper XLSX formula support (shared formulas, tables, dynamic arrays at
least in read/calculate), PPTX transitions/animations parity as far as the
model supports them, and a round-trip fuzz corpus for ODF/DOCX/XLSX.

**Acceptance.** The round-trip corpus reports its fidelity delta per format;
tracked changes render in the editor and survive a save.

**Status (v7.0.0).** Delivered for the measured scope: DOCX tracked changes
render with author and date and survive a save, XLSX shared formulas expand on
import, PPTX transition durations round-trip, and
`docs/office-fidelity-report.md` records a zero measured delta for DOCX, ODT,
XLSX and PPTX constructs in the programmatic corpus. Images/charts/comments
counters, dynamic arrays, shared-formula output and a random fuzzer stay open
and are listed as such.

### D7. Provenance v2 (C8 follow-up)

**Scope.** DOCX provenance manifests and automatic manifests from the producer
screens (OCR, merge, convert, invoice export), shown in the verification screen
as a source chain.

**Acceptance.** A generated DOCX and an OCR'd PDF both carry a manifest; a
tampered source is detected on verify.

**Status (v7.0.0).** Delivered: DOCX manifests (one package part, canonical
content digest so re-compression does not report a false change), the unified
source chain with `intact`/`changed`/`absent`/`unsupported` verdicts and the
four producer hooks (convert to DOCX, OCR PDF, merge PDF, plus the generic
write path for a manual copy). The chain puts a red tamper warning before the
recorded sources when the digest no longer matches, and
`docs/provenance-v2.md` documents the format, the rules and the limits (no
XMP in DOCX, a self-consistent forged manifest is out of scope).

### D8. Mobile ergonomics audit (C11 follow-up)

**Scope.** Walk every screen at 360 dp: reachable primary actions, one-hand
bottom sheets, safe areas, landscape, keyboard overlap; a small screenshot
matrix for both languages.

**Acceptance.** No screen needs a second hand for its primary action; the
matrix is attached to the release.

**Status (v6.15.0).** Delivered: the shared screen shell moves header actions
into a 44 px bottom action bar on phones (25 of 55 routes; the rest keep the
primary action in the flow), the keyboard inset lifts the bar instead of hiding
Save, safe-area padding covers the notch and the system bars, landscape keeps
one scrolling action row and icon-only tabs, and
`docs/mobile-ergonomics-audit.md` carries the bilingual 360 dp matrix plus the
14-scenario device record sheet. The physical phone pass stays manual.

### D9. Performance and memory

**Scope.** Large-document streaming in Writer/Calc (500-page Writer, 50k-cell
Calc), PDF worker isolation hardening, startup budget, and benchmark trends
wired into CI.

**Acceptance.** Named budgets for open/save/render and cold start; a regression
fails CI when a benchmark exceeds its trend band.

**Status (v7.0.0).** Delivered: always-on 500-page DOCX and 50 000-cell XLSX
open/save tests with printed measurements, counting-allocator memory ceilings
(384/256 MiB), a calc render-window test for 50 000 rows, a pinned 2–8 slot
heavy-work bound with panic-release tests, and
`docs/performance-budgets.md` naming the product budgets, test bounds, the
isolation rule and the criterion trend gate. Cold start stays a manual release
measurement, and the criterion trend check is advisory by policy.

## Workstream C — distribution and trust

### D10. First signed Windows release (#501)

**Scope.** Choose SignPath.io (recommended) or Azure Trusted Signing, complete
the account/identity validation, add the secrets, dry-run the release workflow,
then publish the first signed installer and update "unsigned" notes only after
the artifact is verified.

**Acceptance.** `Get-AuthenticodeSignature` and `signtool verify` pass on the
installer and the portable exe; `latest.json` still verifies in the app.

**Status (v7.1.0 preparation).** The workflow side is ready: three-provider
detection with an unsigned fallback, a preflight that fails fast and names a
missing secret, the SignPath request/restore steps, the updater-signature
refresh and both verification steps; `docs/signing.md` carries the operator
runbook and `npm run signing:check` pins all of it. **Acceptance stays open**
until the owner completes the SignPath OSS application, identity validation
and the five secrets, and the first signed installer passes the two
verification commands.

### D11. Catalogue listings and reproducible builds (#502)

**Scope.** File the IzzyOnDroid request, decide on F-Droid main (prebuilt
engine blockers), and scope a reproducible-build effort (pinned toolchain,
deterministic archive ordering) so third parties can verify a release binary
against the source.

**Acceptance.** The listing exists, or the decision to skip is recorded; the
reproducibility report names what matched and what did not.

### D12. Platform breadth

**Scope.** Decide on macOS packaging and notarization (current macOS support is
CI-build-only), Flatpak or a sandboxed Linux format, and whether Windows
ARM64 is worth packaging.

**Acceptance.** Each platform is either shipped with the same verification as
Windows/Linux/Android, or explicitly declined with the reasons.

### D13. Command palette task commands (C1 follow-up)

**Scope.** The command palette today navigates and opens files; let it run the
profession-desk tasks ("bu PDF'i karart", "KDV özeti çıkar", "delil klasörü
oluştur") with their context, including a clear disabled reason when the
current selection does not fit.

**Acceptance.** Every desk task is reachable from the palette with a shortcut,
a disabled command explains why, and the plugin command registry keeps working.

**Status (v7.1.0).** Delivered: ten desk tasks with `Ctrl+Alt+…` shortcuts take
the current file selection (the VAT task also selects its AI pack), a query
shows a not-runnable task with its reason and `aria-disabled`, and the plugin
registry suite is untouched. Scoping by an editor text selection is not covered
yet and is listed as a limitation.

## Prioritisation

| Order | Item | Impact | Effort | Risk | Depends on |
|---|---|---|---|---|---|
| 1 | D1 real two-device acceptance | closes the only unproven feature | 2–3 d | low | two devices |
| 2 | D2 deletion undo | trust after a destructive action | 2–3 d | low | D1 findings |
| 3 | D3 resumable transfer | makes LAN sync usable on phones | 3–5 d | medium | D1 |
| 4 | D10 first signed release | removes the SmartScreen warning | 1–2 d + provider | low | provider account |
| 5 | D4 scanner corpus | measurable C12 acceptance | 3–4 d | medium | photo corpus |
| 6 | D8 mobile ergonomics audit | daily usability on phones | 2–3 d | low | – |
| 7 | D5 local AI v2 | differentiator quality | 5–8 d | medium | eval harness |
| 8 | D7 provenance v2 | trust story for generated files | 2–3 d | low | – |
| 9 | D6 office fidelity | lock-in for the office core | 8–12 d | high | round-trip corpus |
| 10 | D9 performance | keeps the suite usable at scale | 4–6 d | medium | benchmarks |
| 11 | D11 listings + reproducible builds | distribution and verifiability | 3–5 d + reviews | medium | accounts |
| 12 | D12 platform breadth | reach | 5–10 d per platform | high | decisions |
| 13 | D13 palette task commands | daily speed for power users | 1–2 d | low | – |

## Proposed release train

| Release | Contents | Why together |
|---|---|---|
| v6.13.0 "LAN kapanışı" | D1, D2, D3 | finish and prove the C14 story before the next feature round |
| v6.14.0 "Tarayıcı korpusu" | D4 | a measured scanner baseline with the real-photo corpus |
| v6.15.0 "Telefon denetimi" | D8 | small, user-visible ergonomics round |
| v7.0.0 "Yerel, eksiksiz" | D5, D6, D7, D9 | the next-generation local capability set; the major bump matches the scope |
| v7.1.0 "Dağıtım ve platform" | D10–D12 | trust/distribution work and platform decisions after the core |

The order puts acceptance first (D1), then the destructive-action fix (D2) and
the transfer reliability (D3) — the three things that make the shipped feature
trustworthy. v7.0.0 is the first release where the AI, fidelity, provenance and
performance work lands together, so it justifies the major number.

## Open decisions for Phase D

1. Deletion retention: 30 days in the app data dir, or "until the user empties
   Recently deleted"? The second is safer but uses storage indefinitely.
2. Resumable transfer: chunk size, resume window and whether the receiver
   keeps partial chunks encrypted at rest.
3. Tiny model: bundle a pinned model in `engines.lock.json`, or keep Ollama as
   the only local path? This decides the installer size question for good.
4. macOS: package and notarize (Apple fee, hardware), or keep macOS as a
   CI-only target?
5. F-Droid main: invest in prebuilt-engine exceptions or stay with
   IzzyOnDroid only?
6. How much office-fidelity scope belongs in v7: tracked changes only, or also
   formula and animation depth?

## Non-goals (unchanged)

- No cloud account, no hosted service, no telemetry and no Play Services.
- No silent conflict resolution or automatic deletion.
- No feature claim without a test marker and a verification path.
- No release that skips the asset/SHA verification described in
  `docs/plan/AI-HANDOFF.md`.

## Process

1. Approve this proposal and pick the first release (recommended: v6.13.0 with
   D1–D3).
2. Open a Phase D tracker issue with one checkbox per item, mirroring #341.
3. Each item: its own branch (`feat/<issue>-…`), PR with tests and docs, a
   `CHANGELOG.md` section, README/`RELEASE_READINESS` updates, then a
   `release/vX.Y.Z` PR after CI is green.
4. Keep the verification discipline: 18-asset list and SHA-256 checks after
   every tag; update the roadmap/tracker as items land.
