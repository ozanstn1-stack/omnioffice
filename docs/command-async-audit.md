# Command async audit (E2.3)

Tauri runs a **non-async** `#[tauri::command]` on its command thread pool and an
**async** one on the async runtime. Neither is the UI thread, so a slow sync
command does not freeze the window — but it does hold a pool slot, and a command
that spawns a process, parses a document or walks a big directory can hold it for
a long time. This audit classifies the surface once, states the rule, and records
the one command that was actually doing heavy work in the wrong place.

## Method

A script walks `src-tauri/src`, finds every `#[tauri::command]` attribute, reads
whether the function is `async`, and looks at the following body for filesystem
or process calls (`std::fs`, `Command::new`, engine binaries). The counts below
come from that pass; a new command that changes them is expected to update this
file.

| Measure | Value |
|---|---|
| Command attributes in the source | 252 |
| Registered commands (`ipc.rs` domains) | 249 |
| `async` commands | 153 |
| Sync commands that touch the filesystem | 50 |

## What the 50 sync filesystem commands are

All of them are bounded, small operations on the app's own configuration state,
not on user documents:

- settings (`load_settings`, `save_settings`), recent files (`add_recent`,
  `clear_recent`), watched folder, dev launch context, frontend log;
- the job store (`jobs_*`), history snapshots (`history_*`), recovery snapshots
  (`recovery_*`), the office/`store_*` state and the AI chat history;
- `output_exists`, `ensure_dir`, `suggest_output`, `file_sizes`,
  `file_fingerprint`, `reveal_document_file` — one stat or one hash of a file the
  user picked;
- plugin source reads, recipe reads, `vault_clear`.

They are kept sync on purpose: they are single small reads/writes, and making
them async would add a thread hop to paths the UI calls in tight loops (the
recovery and history panels call several per render). The rule below is about
what a command does, not about its return type.

## The process-spawning command

`engine_status` probed the local engines — `tesseract --version`, a real process
spawn, tens to hundreds of milliseconds on a cold disk — from a sync command. It
is now `async` and runs the probe on the blocking pool
(`tauri::async_runtime::spawn_blocking`), with `EngineStatus::missing()` as the
answer when the probe cannot run, so the panel shows the install hint instead of
a blank state (`crates/pdfcore/tests/engine_status_test.rs`).

**Correction to the Phase E proposal:** the proposal named `ocr_languages` as
the tesseract caller. It is not: `ocr_languages` lists the `.traineddata` files
in the tessdata directory (one `read_dir`) and stays sync. The probe that spawns
the binary is `engine_status`, which the proposal did not mention.

## The rule for a new command

Make it `async` and put the work on the blocking pool — through the existing
`run_blocking` helper in `commands.rs`, which also takes the heavy-work
concurrency permit — when it:

- spawns a process (an engine, `soffice`, `qpdf`, an installer bridge);
- parses or writes a **user document** (the worker process already covers the
  untrusted parsers; anything not routed there belongs on the blocking pool);
- hashes or copies more than a few megabytes;
- waits on a native dialog or a mobile-plugin callback;
- performs a network request.

Small reads and writes of the app's own config files may stay sync; if one of
them grows into a loop over a directory tree, it moves.

## Limits

- This is a review rule backed by a classification, not a mechanical gate: the
  script counts, it does not decide whether a given call is heavy. A command that
  copies 50 MB through `std::fs::copy` inside a one-line body would still be
  counted as "touches the filesystem" and read as acceptable.
- The counts above are from the E2.3 slice; the check is that a change which adds
  a sync command with process/engine work shows up here.
- `run_blocking` acquires the concurrency permit the heavy-work semaphore owns;
  an async command that calls `spawn_blocking` directly (as the dialog pickers
  do) does not, so it is bounded only by the platform pool.
