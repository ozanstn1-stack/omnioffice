# PDF Swiss Army Knife v1.2.0 — AI assistant (optional, local-first by default)

This release adds an **optional AI assistant** for summaries, translation, document Q&A, OCR text repair and metadata suggestions — built around a clear privacy contract: nothing is sent anywhere until you add your own API key and confirm the data notice for the document.

## New: AI assistant

- **Summaries** — short / medium / detailed, paragraph / bullet / executive-brief styles, output language and an optional focus ("payment terms and dates"). Documents longer than the model context are summarized with a **map/reduce** pass so nothing is silently dropped.
- **Translation** — page-by-page Markdown translation (optionally bilingual) into any target language, streamed into the output pane.
- **Ask the document** — grounded Q&A with page citations; a keyword retriever selects the relevant pages first, so even long documents answer quickly.
- **Repair OCR text** — fixes broken words, hyphenation and spacing in scanned documents without summarizing (pairs well with the OCR tool).
- **Metadata ideas** — suggests title/author/subject/keywords and can apply them to a new PDF with one click.
- **Live streaming** — answers appear while they are generated, with a Stop button and per-stage progress (extract → chunk → generate).
- **Save or copy** the result as Markdown/plain text.

### Privacy contract

| Rule | Implementation |
| --- | --- |
| Off by default | No request is possible without an API key **and** a per-document consent checkbox |
| You see what is sent | Page count, character count and estimated words are shown before running |
| Text only | Only the extracted text of the selected document is transmitted — never the file, passwords or unrelated metadata |
| Key handling | Stored with **Windows DPAPI** (`CryptProtectData`), masked in the UI, never logged, removable with one click |
| Errors | Friendly codes for rejected key, rate limit, insufficient balance, network, missing text layer and oversized context |

Point the base URL at a local OpenAI-compatible server (Ollama, LM Studio, llama.cpp) and the AI features run **entirely offline**.

## Everything else from v1.1.0 / v1.0.0
Reading mode (continuous scroll, zoom, full-text search), merge, organize, split, extract/delete/rotate, compression (lossless + strong re-render with real estimates), offline OCR (8 languages), watermarks, page numbering, metadata editor, image conversion, batch processing — plus the offline Chrome extension (separate release).

## Downloads

| File | Description |
| --- | --- |
| `PDF-Swiss-Army-Knife-Setup-1.2.0.exe` | Windows installer (per-user or all-users, uninstall supported) |
| `PDF-Swiss-Army-Knife-Portable-1.2.0.zip` | Portable build — extract and run |
| `SHA256SUMS.txt` | SHA-256 checksums |

Upgrade note: install over an existing 1.0/1.1 installation — settings, recent files and preferences are kept.

## Validation status

- **75 Rust tests** pass (`cargo test --workspace`), including **12 new `aicore` tests** that run the real DeepSeek client against a local mock HTTP server: SSE streaming assembly, error mapping (401 → invalid key, 402 → balance, 429 → rate limit), cancellation mid-stream, prompt/chunk construction, keyword retrieval and metadata JSON parsing.
- The full desktop flow was verified end-to-end against a **mock DeepSeek endpoint** (configurable base URL): settings load, DPAPI/plain key storage, preview ("3 pages · 220 characters will be sent"), consent, streaming into the UI, and a successful streaming request logged by the mock server.
- Live calls to the real api.deepseek.com require a personal API key, so they were not executed in this environment; the request/response shapes follow the documented DeepSeek API and are covered by the mock-server tests.

## Known limitations
AI works on extracted text only (no layout translation/rewriting), needs internet unless you self-host an endpoint, caps requests at 400 pages, and is not part of the Chrome extension or the batch queue yet.
