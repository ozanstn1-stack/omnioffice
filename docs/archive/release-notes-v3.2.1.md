# Office Swiss Army Knife v3.2.1

**Comprehensive security hardening, centralized path validation, and job reliability.**

This patch release addresses critical security and correctness audits across the Tauri 2, plugin, and core engines, hardening IPC boundaries and preventing SSRF, untrusted path traversal, and race conditions.

## Highlights

- **Native Folder Dialog for Plugins:** Eliminated trusting raw webview-supplied paths in `plugin_install_from_path`. Plugins are now installed via a Rust-side native folder dialog (`plugin_install_from_dialog`) with strict symlink rejection on source manifests and targets.
- **SSRF & Cloud Metadata Defense:** Hardened `plugin_http_request` with proactive DNS resolution and IP destination filtering. Blocks cloud metadata endpoints (`169.254.169.254`, IPv4-mapped IPv6), link-local, multicast, CGNAT, and private network addresses across both HTTP and HTTPS.
- **Centralized Filesystem Validation:** Refactored all 78 Tauri filesystem commands to use strongly typed wrappers (`ValidatedInputFile`, `ValidatedOutputFile`, `ValidatedDirectory`). Rejects path traversal (`..`), NUL bytes, ASCII control characters, Windows reserved device names (`CON`, `PRN`, `AUX`, `NUL`, `COM1-9`, `LPT1-9`), UNC paths (`\\server\share`), and extended paths (`\\?\`).
- **Trinary PDF Redaction Verification:** Redaction verification upgraded from a binary boolean to a trinary contract (`Removed`, `PossiblyPresent`, `NotVerified`). Partial redactions or remaining text matches immediately surface as warnings to prevent false security assurances.
- **Digital Signature Gate:** `pdf_sign` now requires immediate post-sign cryptographic and ByteRange verification before returning success, ensuring malformed signatures or altered digests are never reported as valid.
- **Job Lifecycle RAII Guards:** Integrated `JobFinishGuard` across asynchronous operations to guarantee jobs cleanly and deterministically transition to terminal states (`success`, `failure`, `cancelled`, `interrupted`) even on early error returns.
- **Dynamic AI Model Discovery:** Added `discover_models` for OpenAI-compatible and Ollama endpoints with credential-safe error messages.
- **CI Quality Scripts:** Added `npm run check:fast` and `npm run check:all` scripts for rapid and comprehensive validation.

## Verification

- `cargo test --workspace` (0 errors across `pdfcore`, `officecore`, `aicore`, `synccore`, `pdf_sak_lib`).
- `npm run check:fast` (ESLint 0 errors, i18n 1505/1505 parity, `tsc --noEmit` 0 errors, Vitest 29 files / 587 tests passed).
