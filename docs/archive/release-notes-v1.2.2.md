# PDF Swiss Army Knife v1.2.2 — thinking mode support and a fix for "unexpected response"

## Fixed: "unexpected response from the AI service"

DeepSeek V4 models run in **thinking mode by default**: the model streams its reasoning (`reasoning_content`) before the answer (`content`). The previous version only read `content`, so when the token budget ran out during the thinking phase — or when the answer arrived only as reasoning — the request ended with:

> unexpected response from the AI service: unexpected response from DeepSeek

Now:

- **The thinking trace is parsed.** Deltas are streamed to the UI with a kind marker and shown in a collapsible "Thinking (model reasoning)" block, separate from the answer.
- **A reasoning-only response is no longer an error.** If the model produced only reasoning (for example because `max_tokens` was exhausted while thinking), that trace is returned as the result and clearly labelled — instead of failing.
- Non-streaming calls (metadata suggestions, OCR text repair) use the same fallback.

## New: thinking mode and reasoning effort in Settings

- **Thinking mode** toggle — on (default, matching DeepSeek) or off for the fastest answers.
- **Reasoning effort** — `low` / `high` (default) / `max`.
- Both are **only sent to `deepseek-v4-*` models** (the API would reject the fields on legacy aliases), and the panel warns when the selected model does not support them.
- **Tip:** if you disable thinking and still see truncated answers, raise *Max tokens*; when thinking is on, give it at least ~2000 tokens so the answer phase has room.

## Verified

- `cargo test -p aicore` → **16 tests** (4 new): reasoning-only streams return the trace, reasoning and answer deltas are reported separately, and the `thinking` / `reasoning_effort` fields are sent for V4 models while being omitted for legacy models.
- End-to-end against a mock endpoint: `thinking=enabled effort=high` when the toggle is on, `thinking=disabled effort=absent` when off, and a reasoning-only stream completes **without any error** (this was the reported failure).
- `cargo test --workspace` → 78 tests passing; installer and portable builds produced for **1.2.2**.

## Upgrade

Install `PDF-Swiss-Army-Knife-Setup-1.2.2.exe` over your existing installation; settings, recent files and the DPAPI-encrypted API key are preserved.
