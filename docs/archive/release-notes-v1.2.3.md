# PDF Swiss Army Knife v1.2.3 — 1M context budget and 384K output tokens

The AI settings now cover the full DeepSeek V4 limits instead of the previous 8,192-token cap.

## Two different limits (both configurable now)

| Limit | DeepSeek V4 | Where it is in the app |
| --- | --- | --- |
| **Context window** (input + output) | **1,000,000 tokens** | Settings → *Context budget (input tokens)* — presets 64K / 128K / 200K / 500K / **1M**, or any value up to 1,000,000 |
| **Maximum output** (one answer) | **384,000 tokens** | Settings → *Max output tokens* — presets 4K / 16K / 32K / 64K / 128K / **384K (API max)** |

- The context budget decides how much document text is sent per request and how the map/reduce chunking is sized. At the 1M setting, a ~2M-character document is summarised in **one** request instead of dozens.
- `max_tokens` is an upper bound, not a target: you are billed for the tokens actually generated. Setting 384K does not cost anything unless the model writes that much — that is why the stored default stays at a sensible 32K and you can raise it per taste.
- Values are clamped to the API limits (256 … 384,000 output; 8,000 … 1,000,000 context), so a typo can never produce a rejected request.

## Also in this release

- **Model names refreshed** for the current DeepSeek lineup: the default is now **`deepseek-flash`** (DeepSeek V4.1 Flash, the canonical name; `deepseek-v4-flash` is still accepted and routed to the same model). Thinking-mode support detection was extended accordingly, so the thinking toggle now works with `deepseek-flash` too.
- Page extraction cap raised from 400 to 2,000 pages (the real limit is now the token budget).
- Q&A retrieval and OCR-text repair also size themselves from the context budget.

## Verified

- **18 `aicore` tests** (2 new): the budget calculation scales correctly (200K tokens → ~400K characters per request, 1M → 2M, clamped at the 1M context) and output tokens are clamped to 384,000.
- End-to-end against a mock endpoint with `contextTokens: 1000000`, `maxTokens: 384000`, effort `max`, model left at its new default: the outgoing request was `model=deepseek-flash thinking=enabled effort=max stream=true` with no errors.
- `cargo test --workspace` → 80 tests passing; installer and portable builds produced for **1.2.3**.

## Your settings were updated

The stored configuration now uses `model: deepseek-flash`, `contextTokens: 1000000` (the 1M window you asked for), `maxTokens: 32768` (raise it in Settings if you want longer single answers), thinking on, effort `high`. Install `PDF-Swiss-Army-Knife-Setup-1.2.3.exe` over your copy — settings and the DPAPI-encrypted API key are preserved.
