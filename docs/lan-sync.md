# LAN sync — design and threat model (C14)

Two devices on the same Wi-Fi move office documents between their folders
without an account, a cloud or a relay. This file is the threat model the
roadmap asked for: what a device on the network may do, how pairing works,
what revocation means, and what is deliberately absent from the first cut.

The code lives in `crates/synccore/src/lan.rs` (protocol, pairing, plan),
`crates/synccore/src/lan/transport.rs` (pinned TLS and framing),
`src-tauri/src/lan_sync.rs` (commands, sealed store, host lifecycle) and
`src/screens/lan-sync-card.tsx` (the UI). The loopback test in
`crates/synccore/src/lan/tests.rs` performs a real TLS pairing, list, download,
upload and an unpair refusal.

## Assets

| Asset | Where it lives | Why it matters |
|---|---|---|
| Documents in the shared folder | user's filesystem | content, names, sizes |
| Device private key + certificate | sealed store (`lan-sync.store`) | impersonating the device |
| Pairing shared keys | sealed store | read/write on a peer's shared folder |
| Device identity (id, name, fingerprint) | plain in the store, broadcast on discovery | traffic analysis only |

There is no internet endpoint and no telemetry. The host listens only while the
screen says it is sharing, so an app that is not sharing has no listening
socket.

## Actors

* **The paired peer** — a device whose six-digit code was entered correctly and
  whose HMAC key is stored. It may list, download and upload documents inside
  the shared folder, and nothing else.
* **An unpaired device on the same network** — may see discovery replies (name,
  fingerprint, port) and may open a TCP connection, but every command except
  `pair` needs a valid HMAC key, and `pair` needs the live six-digit code.
* **A malicious app on either device** — out of scope: it can read the sealed
  store only through the same DPAPI/Keystore protection the rest of the app
  uses, and file access is already the platform's sandbox.
* **An active network attacker (MITM)** — can relay and drop packets; see the
  residual-risk section.

## Pairing

1. The host shares a folder and shows a six-digit code: random, 120 s lifetime,
   five wrong attempts, one-time. The code is compared digit by digit without an
   early exit.
2. The client connects over TLS. During pairing only it accepts an unknown
   certificate and records its SHA-256 fingerprint; the host reports its own
   fingerprint inside the encrypted answer, and a mismatch aborts the pairing
   (a relay cannot present the host's certificate).
3. A correct code inside the 120 s window returns a fresh 32-byte HMAC key and
   closes the pairing window. The client stores the key plus the pinned
   fingerprint; the host stores the key against the client's device id.
4. Both stores are sealed with the platform cipher (DPAPI on Windows, Android
   Keystore on Android, `plain:` with the UI's warning elsewhere).

What the six-digit code is **not**: a password. It is a one-time presence
proof with a short life and a small attempt budget. See the residual risks for
what an active attacker can still do.

## After pairing

* The client pins the host certificate: every later connection refuses any
  other certificate. There is no CA, no public CA trust and no
  trust-on-first-use after pairing.
* Every request carries `device_id`, a Unix timestamp, a random nonce, the
  operation, the path and the SHA-256 of the body, signed with HMAC-SHA256
  under the pairing key. The host refuses a stale timestamp (±5 minutes), and a
  request whose body length or hash does not match.
* Paths are validated: only relative paths, no `..`, no absolute paths, no
  hidden paths, and only document extensions (`docx`, `xlsx`, `pptx`, `odt`,
  `ods`, `odp`, `pdf`, `rtf`, `txt`, `csv`, `oswk`, `png`, `jpg`, `jpeg`).
  A transfer is capped at 512 MiB on both sides; a listing at 10,000 files.
* There is no delete operation and no remote code path. The host only reads,
  lists and writes inside the folder the user shared.

## Sync decisions

`scan_folder` hashes the local folder, `list` returns the same shape from the
peer, and `plan_sync` classifies every path with the existing three-way
`metadata::detect_state` against the last common hash the store kept:

| state | default action |
|---|---|
| `synced` | nothing |
| `local_only`, `local_ahead` | upload |
| `cloud_ahead` | download |
| `conflict` | **nothing** — the UI asks |

Conflicts are resolved by the user only; `lan::resolve_action` delegates to the
existing `merge::plan` policy, so `keep both` saves the peer copy next to the
local file (via `merge::unique_cloud_copy_name`), `keep local` uploads and
`keep remote` downloads. "Sync all safe changes" applies the default actions
and lists conflicts untouched.

## Revocation

* `lan_unpair` on the client forgets the peer (key and pin).
* `lan_host_unpair` removes the key on the host; the very next request from
  that device is refused, and its stored base hashes are deleted.
* Stopping the host closes the listener and the pairing window.
* Key material is never reused after a pairing is removed; re-pairing issues a
  new 32-byte key.

## Background and platform limits

The host runs only while the app is open and the user started sharing; there
is no foreground service in the first cut. On Android the process may be
killed when it goes to the background, so a long transfer is not promised;
the client shows a plain error and the plan can be run again. A future version
can add a foreground service when the store/transport story is tested on a
real device.

## Discovery

The first cut uses a UDP broadcast probe on port 47821 and a JSON reply
(`device_id`, `name`, `port`, `fingerprint`). It needs no dependency and works
on Android without a multicast lock. mDNS is the roadmap's stated discovery
mechanism and the follow-up: it gives service names and SRV/TXT records without
broadcast storms, at the cost of a dependency and multicast handling on Android.

## Residual risks (honest list)

1. **Pairing MITM.** A six-digit code is not a PAKE. An active attacker that
   sits between the two devices can relay the handshake, learn the code and
   pair its own device (the legitimate client aborts on the fingerprint
   mismatch, but the host sees a paired device). Mitigations present: short
   lifetime, five attempts, one-time code, paired-device list visible in the
   UI, unpair works immediately. Mitigation planned: bind the code to a QR
   code carrying the certificate fingerprint, or move to SPAKE2.
2. **Traffic analysis.** The content is encrypted, but a passive observer sees
   that two devices talk, how much and when.
3. **Discovery metadata.** The broadcast reply carries the device name and
   certificate fingerprint to anyone on the LAN. This is deliberate (it makes
   the UI understandable) and leaks no document data.
4. **One role per device id.** A device is either a host or a client for a
   given peer; symmetric pairing is a follow-up.
5. **No delete propagation.** Removing a file locally does not delete it on the
   peer (it would be downloaded again on the next plan).
6. **Single-threaded host.** One request is handled at a time; a stalled peer
   delays the queue for at most the 60 s socket timeout.
7. **Symlinked folders.** The host resolves paths under the shared root and
   rejects traversal, but a symlink created inside the folder by the user is
   followed by the filesystem. Documents are read, never executed.
