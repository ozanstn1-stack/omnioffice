# LAN sync acceptance — two real devices (D1)

The LAN sync code is covered by unit tests and a loopback host/client test
(`crates/synccore/src/lan/tests.rs`), and `crates/synccore/tests/lan_two_host.rs`
runs the whole two-host flow (pair, list, fetch, push, conflict, confirmed
deletion, unpair) on loopback as a release contract test. Neither proves that
mDNS discovery, the firewall and the Android foreground service work on a real
Wi-Fi. This guide is the manual acceptance that closes C14's last gap.

Run it once per release that touches `synccore/src/lan*`, `src-tauri/src/lan_sync*`
or the LAN card, and record the result in the release notes. Keep the table in
the PR that adds the result.

## Prerequisites

- Two devices on the **same Wi-Fi** (no client isolation): one Windows PC and
  one Android phone, or two Android phones. A guest network without multicast
  is a separate row (the broadcast fallback must still work).
- The same OmniOffice build on both (`versionName` equal), installed from the
  release APK/installer.
- A test folder on each device with at least: one small `.docx`, one `.pdf`
  (a few MB), one file with a Turkish name (`örnek belge.pdf`), one nested
  folder (`test/alt/belge.docx`).
- Network notes: host listens on TCP 47820, discovery uses UDP 47821 and mDNS
  5353; Windows Firewall must allow the app on private networks (the first
  launch prompts). Android needs the notification permission for the sharing
  notification (Android 13+), granted once.
- Both screens: **Sync → LAN sync**. The card shows the device name,
  fingerprint prefix and the encryption state.

## Matrix

| Row | Devices | Network | Required |
|---|---|---|---|
| A | Windows ↔ Android | home/office Wi-Fi | yes |
| B | Android ↔ Android | home/office Wi-Fi | yes |
| C | Windows ↔ Windows | same Wi-Fi | best effort |
| D | any pair | guest Wi-Fi with multicast blocked | best effort (broadcast fallback) |

## Scenarios

Mark each row **P** (pass), **F** (fail with a note) or **S** (skipped with the
reason). A release is accepted when every scenario passes on rows A and B and
the best-effort rows are either passing or documented.

| # | Scenario | Steps | Expected |
|---|---|---|---|
| S1 | Discover | On the phone: “Find devices”. | The PC appears within ~3 s with its device name; the discovery annotation says mDNS or fallback. |
| S2 | Pair | Start sharing on the PC, read the six-digit code, enter it on the phone. Both devices on v7.2 or newer. | Phone stores the pairing; the PC lists the phone under host peers; the code disappears/expires and cannot be reused. A phone on an older build is told both devices need the same version. |
| S3 | Wrong code | Try a wrong code, then the right one before it expires. | The phone refuses the code (it never leaves the device, so the PC's attempt budget is untouched) and then the right code pairs. A device that guesses *against* the PC spends one of the five attempts; the fifth closes the window. |
| S4 | List and download | Phone: “Local folder”, “Compare”, then sync-safe. | The PC's files appear in the plan; a download arrives byte-identical (compare SHA-256); the Turkish file name stays intact. |
| S5 | Upload | Phone: add a new file to its folder, compare, sync-safe. | The file appears on the PC, readable in the app, same SHA-256. |
| S6 | Big file | Upload a ~50 MB PDF. | Progress is visible; memory stays bounded; the file verifies after the transfer. |
| S7 | Conflict | Edit the same file on both sides, compare. | The plan shows `conflict` and nothing is overwritten; “Keep both” writes a copy next to the local file; “Keep here/there” behaves as labelled. |
| S8 | Delete here | Delete a file on the phone folder, compare, confirm “Delete there”. | The PC's copy is gone after confirmation; nothing else disappears. |
| S9 | Delete there | Delete a file on the PC, compare on the phone, confirm “Delete here”. | The phone's copy is gone; the trash entry exists (D2) and can restore it. |
| S10 | Unpair | Unpair on the phone; then try “Compare” again. | The request is refused; the PC no longer accepts the phone's key; re-pairing works with a new code. |
| S11 | Background share | Start sharing on the phone, switch to another app, wait 5 minutes, then pair from the PC. | The phone still answers (foreground service + multicast lock); the notification is visible. |
| S12 | Interrupted transfer | During a large upload, stop the share (or kill the app) on the receiver, then resume. | No partial document appears in the shared folder; progress restarts from the peer's offset (not from zero) and the upload completes; the final SHA-256 matches. |
| S13 | Unicode and paths | Transfer `örnek belge.pdf` in `test/alt/`. | File name is preserved exactly; the nested path arrives in the right folder. |
| S14 | Blocked multicast | On a guest Wi-Fi where mDNS fails, discover again. | The broadcast fallback finds the host (annotation says fallback) or the row is recorded as a limitation. |

## Recording

Copy this table into the release PR/release notes and fill it:

```
Build: 6.13.0
A Windows↔Android: S1 P · S2 P · S3 P · S4 P · S5 P · S6 P · S7 P · S8 P · S9 P · S10 P · S11 P · S12 P · S13 P · S14 P
B Android↔Android: …
C best effort: …
Device notes: <model, Android version, Wi-Fi/AP>
Failures: <scenario, symptom, log/screenshot link>
```

## What is not covered here

- Throughput and battery measurements — a separate benchmark, not acceptance.
- More than two devices at once; the current model lists one folder per host.
- iOS; not a target.
- The background service limit (Android may still stop `dataSync` after its
  system cap); S11 only proves it survives a normal app switch.

## Automated part

`cargo test -p synccore --test lan_two_host` runs the flow end to end on
loopback: two hosts, two folders, pair with the printed code, list, download,
upload, a three-way conflict, a confirmed delete and an unpair refusal.
`crates/synccore/src/lan/pairing_tests.rs` runs the attacker side of S2/S3: a
certificate-substituting relay, a device guessing codes against the host, a
replayed signed request, a stalled peer and a pre-7.2 client. Both are the
regression net for the manual rows; a failure there blocks the release before
the device pass starts.
