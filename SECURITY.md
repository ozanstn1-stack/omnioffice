# Security policy

## Reporting a vulnerability

Use GitHub's private vulnerability reporting on this repository - **Security ->
Report a vulnerability**. It opens a private advisory that only the maintainers
can read; please do not open a public issue for a suspected vulnerability.

Include what you can: the affected version and platform, the steps to reproduce,
the impact you expect, and a proof of concept if you have one. We will confirm
the report, keep you posted while it is fixed, and credit you in the advisory
unless you ask us not to.

## Supported versions

| Version | Supported |
| --- | --- |
| 7.x | Yes - fixes ship in the newest patch or minor release |
| 6.x and older | No - upgrade to the current release |

Releases are built from tags on `master`; every published binary and its
`SHA256SUMS*.txt` are on the release page.

## What the application promises

OmniOffice runs on your machine. Scanning, OCR, document conversion, the office
editors and the AI prompt pipeline run locally, and the app makes no network
request unless you use a feature that needs one: cloud sync, the updater, or AI
with your own API key. `RELEASE_READINESS.md` lists each claim with the test
that backs it.

## Known, deliberate limits

These are decisions recorded in the repository, not open bugs. Please still
report it if one of them turns out to be worse than described here.

- **The Windows installers are unsigned** (`docs/signing.md`, D10 decision), so
  SmartScreen shows "unknown publisher". That is expected. Verify a download by
  comparing its SHA-256 against `SHA256SUMS-windows.txt`.
- **The command ACL and the path policy separate the webview from the backend,
  not users from each other.** A local process running as the same user reaches
  the same files directly; that is what the path policy and the secret store
  cover (`docs/command-acl.md`).
- **Engine binaries are third-party artifacts.** pdfium, qpdf, tesseract and the
  fonts are downloaded at build time and pinned by SHA-256 in
  `scripts/engines.lock.json`; they are not rebuilt from source here.

## Scope

In scope: the desktop and Android applications in this repository, the release
workflows, and the code they build. Out of scope: vulnerabilities in third-party
engines that are already public and unfixed upstream (report those upstream),
and anything that requires an already-compromised machine or account.
