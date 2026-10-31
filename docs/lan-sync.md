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
  the two pairing rounds needs a valid HMAC key, and those rounds only produce
  a key for a device that knows the live six-digit code.
* **A malicious app on either device** — out of scope: it can read the sealed
  store only through the same DPAPI/Keystore protection the rest of the app
  uses, and file access is already the platform's sandbox.
* **An active network attacker (MITM)** — can relay and drop packets; see the
  residual-risk section.

## Pairing

1. The host shares a folder and shows a six-digit code: random, 120 s lifetime,
   five failed attempts, one-time. The code is never compared as a string and
   never crosses the wire.
2. Pairing is a **SPAKE2 handshake** with the code as the password
   (`crates/synccore/src/lan/pairing.rs`). Only a device that knows the code
   derives the key both sides end up with, so a recorded handshake gives an
   attacker nothing to test offline: a wrong guess costs one online attempt.
3. **Both certificates are hashed into the key.** The client records the
   certificate it sees, the host states its own fingerprint inside the
   encrypted answer, and the key - plus every confirmation tag - covers that
   certificate pair. A device that terminates TLS with its own certificate and
   relays the handshake therefore derives two different keys, so both
   confirmations fail: nothing is pinned and no peer is recorded. That is the
   attack the pre-7.2 handshake allowed, and
   `crates/synccore/src/lan/pairing_tests.rs` runs it as a regression test.
4. Three messages over two connections:

   ```text
   client -> host  pair_begin   session, spake, device_id, name, fingerprint, port
   host -> client  pair_begin   spake, fingerprint, confirm (host tag)
   client -> host  pair_finish  session, device_id, confirm (client tag)
   host -> client  pair_finish  ok, device_id, name, fingerprint
   ```

   The second connection is pinned to the certificate the first answer came
   over, so the confirmation cannot be redirected. A pairing session is
   one-shot (it is taken out of the host's in-flight map before it is checked),
   a device's retry replaces its earlier session, and at most eight sessions
   wait at once.
5. A matching confirmation closes the pairing window and both sides store the
   same 32-byte key: the client with the pinned fingerprint, the host with the
   client's certificate pin and the address it connected from (plus the port
   the client announced when it is sharing too). **Pairing is symmetric:** the
   same entry lets either side call the other, as long as the other side is
   sharing a folder.
6. A failed confirmation spends one of the five attempts and the fifth closes
   the window; the answer is the same "the pairing code is not correct" a wrong
   code always produced.
7. Both stores are sealed with the platform cipher (DPAPI on Windows, Android
   Keystore on Android, `plain:` elsewhere) and the file is always written
   owner-only (`0600`): the vault tier of `secret.rs` holds single secrets, not
   a store that grows with every paired device.
8. A device on the pre-7.2 protocol sent the code in the clear; this host
   refuses that operation with an update message instead of accepting it, so
   there is no silent downgrade. Both devices must run v7.2 or newer.

What the six-digit code is **not**: a password to be typed into a peer. It is
the password of a PAKE, backed by a 120 s life and a five-attempt budget; see
the residual risks for what an active attacker can still do.

## After pairing

* The client pins the host certificate: every later connection refuses any
  other certificate. There is no CA, no public CA trust and no
  trust-on-first-use after pairing.
* Every request carries `device_id`, a Unix timestamp, a random nonce, the
  operation, the path and the SHA-256 of the body, signed with HMAC-SHA256
  under the pairing key. The host refuses a stale timestamp (±5 minutes), a
  request whose body length or hash does not match, and a request it has
  already accepted: a bounded 8,192-entry window remembers the nonces of the
  last five minutes, so replaying a captured request inside the freshness
  window is refused (`crates/synccore/src/lan/replay.rs`).
* Paths are validated: only relative paths, no `..`, no absolute paths, no
  hidden paths, and only document extensions (`docx`, `xlsx`, `pptx`, `odt`,
  `ods`, `odp`, `pdf`, `rtf`, `txt`, `csv`, `oswk`, `png`, `jpg`, `jpeg`).
  A transfer is capped at 512 MiB on both sides; a listing at 10,000 files.
* There is no remote code path. The host only reads, lists, writes and - after
  a confirmed plan on the other side - deletes inside the folder the user
  shared; every one of those operations goes through the same path validation.

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

**Deletions are propagated only after a confirmation.** `plan_deletions` looks
at paths in the last common base: a path missing locally but present on the
peer is a local deletion, the reverse is a remote deletion. Nothing is removed
by a plan; the user confirms `delete_remote` / `restore_local` (for a local
deletion) or `delete_local` / `restore_remote` (for a remote one). The `delete`
request is HMAC-signed like every other operation and goes through the same
path validation, so it can only remove a shared document.

## Resumable transfers

A large upload is no longer one request that dies with the app. The protocol
splits it into `begin_put` / `put_chunk` / `finish_put`:

- the client begins with the total size and the SHA-256 of the whole document;
  the host answers with the offset it already has (0 for a new upload, the
  length of the partial file after an interruption);
- each chunk (512 KiB by default, at most 4 MiB accepted) carries the byte
  offset it starts at, so an out-of-order chunk is refused instead of appended;
- `finish_put` checks the assembled length, streams the file through SHA-256 and
  only then moves it into place; a hash mismatch drops the partial file so a
  poisoned resume cannot corrupt the document.

Partial files live in the host's shared folder under `.parts/` with a hashed
name, are never listed as documents (hidden directories are skipped) and are
cleaned up when they are older than 24 hours. The client retries up to three
times and resumes from the host's offset, so a dropped connection or an Android
foreground-service restart continues instead of restarting. Progress is emitted
as a `lan-progress` event (`path`, `sent`, `total`) and shown in the LAN card;
downloads are still whole-file and not resumable yet.

## Deletion safety: trash and tombstones

A confirmed deletion used to be permanent. Every confirmed `delete_remote` or
`delete_local` now first copies the document into a **local trash** for 30 days:
a sealed index plus the bytes under the app config directory (200 entries and
the transfer cap bound it; expired entries are pruned on list/push and can be
cleared explicitly). "Recently deleted" in the LAN card restores an entry back
into the selected shared folder - the restore is local, the next comparison
uploads it like any other local change.

Alongside the trash, the pairing store keeps a **tombstone** per deleted path
and peer (path, SHA-256, side, timestamp). While a tombstone is live, the plan
does not report the same deletion again, so a re-sync cannot resurrect a file
or ask the same question twice; restoring the file clears its tombstone. Base
entries for paths that are gone on both sides are dropped during the plan, so a
later file with the same name does not look like a conflict against a stale
ancestor. Tombstones expire with the same 30-day window.

## Revocation

* `lan_unpair` on the client forgets the peer (key and pin).
* `lan_host_unpair` removes the key on the host; the very next request from
  that device is refused, and its stored base hashes are deleted.
* Stopping the host closes the listener and the pairing window.
* Key material is never reused after a pairing is removed; re-pairing issues a
  new 32-byte key.

## Background and platform limits

On Android the host is protected by a `dataSync` foreground service
(`LanShareService.kt`, started through the `LanSharePlugin` bridge in
`src-tauri/src/android_lan.rs`) with an ongoing notification, plus the
multicast lock mDNS needs. The system still owns the lifetime: Android 15
stops `dataSync` services after six hours in a 24-hour window (the service
ends cleanly and the UI says sharing stopped), and a battery optimisation or
a manual force-stop ends it earlier. On desktop the host lives exactly as long
as the process; closing the app stops sharing.

A transfer that is interrupted is not resumed; the client reports the error and
the plan can be run again.

## Discovery

Discovery is **mDNS/DNS-SD** (`crates/synccore/src/lan/mdns.rs`): the host
advertises `_omnioffice._tcp.local.` with a TXT record carrying `device_id`,
`fingerprint` and the protocol version, and a client browses the same type. The
address comes from the resolved A/AAAA record and the SRV port, so one browse
yields everything pairing needs. On Android the plugin holds a
`WifiManager.MulticastLock` while sharing, because Wi-Fi power saving drops
multicast packets otherwise.

The phase-1 UDP broadcast on port 47821 stays as a fallback for networks that
block multicast (some guests and CI sandboxes do); both results are merged by
device id and the mDNS answer wins. No manual IP entry is needed.

## Acceptance

Two layers guard the feature:

- **Automated contract:** `cargo test -p synccore --test lan_two_host` runs two
  hosts with two folders on loopback through pair, list, download, upload, a
  symmetric call, a three-way conflict, a confirmed deletion and an unpair
  refusal. `crates/synccore/src/lan/pairing_tests.rs` adds the attacker cases
  the threat model names: a relaying TLS MITM, a code-guessing device, a
  replayed signed request, a stalled peer and a pre-7.2 client. CI runs both
  with the rest of the Rust suite.
- **Manual two-device pass:** [`docs/lan-sync-acceptance.md`](lan-sync-acceptance.md)
  is the matrix (Windows ↔ Android, Android ↔ Android, plus best-effort rows),
  with the scenarios, the record sheet and what the pass does not cover. Run it
  for every release that touches the LAN code and paste the result into the
  release notes.

## Residual risks (honest list)

1. **Pairing MITM: closed in v7.2.0.** The code is a SPAKE2 password and both
   certificates are bound into the key, so a device that terminates TLS with
   its own certificate and relays the exchange derives two different keys: the
   client pins nothing and the host records nothing (the relay is a regression
   test). What remains: a device that *knows* the code can pair - that is what
   the code is for - an attacker can hold the eight in-flight pairing sessions
   for up to 120 s, so a legitimate pairing may have to wait for a reopened
   window, and six digits against five attempts is a 1-in-200,000 per-window
   guess for an attacker that does not otherwise know the code.
2. **Traffic analysis.** The content is encrypted, but a passive observer sees
   that two devices talk, how much and when.
3. **Discovery metadata.** The broadcast reply carries the device name and
   certificate fingerprint to anyone on the LAN. This is deliberate (it makes
   the UI understandable) and leaks no document data.
4. **Deletion is a user action.** A deletion on one side only propagates after
   the user confirms it; a wrong confirmation is not undoable from the app (the
   peer's copy is gone), so the plan shows the path and the side first.
5. **Background limits are the platform's.** The Android foreground service
   keeps the host alive, but a force-stop, battery optimisation or the
   `dataSync` six-hour cap ends it; transfers are not resumed.
6. **Bounded, not elastic, hosting.** Four worker threads serve connections
   (with up to 16 more queueing); a stalled peer occupies one worker for at
   most the 60 s socket timeout and a connection that arrives while the queue
   is full is dropped instead of waiting. Stopping the share closes the
   listener immediately and does not wait for a worker's in-flight request, so
   a peer that opens a socket and sends nothing cannot hold "stop sharing".
7. **Symlinked folders.** The host resolves paths under the shared root and
   rejects traversal, but a symlink created inside the folder by the user is
   followed by the filesystem. Documents are read, never executed.
8. **mDNS leaks a service name.** The advertised instance name is the device
   name (sanitized); it is visible to the LAN like the broadcast reply was.
