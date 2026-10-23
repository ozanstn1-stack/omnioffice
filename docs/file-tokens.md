# Capability tokens for user files

The webview never receives a filesystem path for a user document unless a
command needs one internally. A native picker (open, save, or folder) registers
the chosen path in the Rust token registry (`src-tauri/src/files.rs`) and
returns an opaque UUID token plus a display name. Commands resolve the token
through `paths::input_file` / `paths::output_file` / `paths::directory`, and
revoke it after use where the flow allows.

- Tokens are random per pick, scoped by access (read or read-write), and die
  with the process.
- Pick tokens never carry a path; save tokens may carry the display path so the
  UI can show the target, but commands only accept the token.
- The frontend keeps a display-name cache (`src/lib/file-token-names.ts`) so
  `fileBaseName` can show the real name for a token.
- The office in-place save flow, PDF studio, watermark, annotate, output
  targets, multi-output folders and the shared drop zone use tokens.

## Documented exceptions

Settings that persist a folder across sessions keep a real path, because a
token cannot outlive the process and the settings store must survive restarts:

- Vault root folders (`src/screens/Vault.tsx`).
- AI settings and AI library folder (`src/components/ai-settings.tsx`,
  `src/screens/AiLibrary.tsx`).
- MailMerge picks a template, data file and output folder with tokens; the
  editable fields show the picked display name and keep accepting a manual
  path only in debug/Android builds (release desktop requires a pick).

Android keeps its SAF path bridge (`src/lib/mobile.ts`) for the same reason:
the platform hands out cache copies, not durable document paths.

Strict mode (B3 slice 4d-d) rejects raw paths in desktop release builds for the
commands that accept tokens; the exceptions above are audited there.
