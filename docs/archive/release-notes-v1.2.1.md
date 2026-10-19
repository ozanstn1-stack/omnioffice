# PDF Swiss Army Knife v1.2.1 — AI model defaults to DeepSeek V4 Flash

Small follow-up release for the AI assistant.

## What changed

- **Default model is now `deepseek-v4-flash`** (DeepSeek V4 Flash). When you add your API key and run an AI action, requests go to the Flash model without any extra configuration. Note that DeepSeek's `deepseek-v4-flash` alias currently serves **DeepSeek-V4-Flash-0731**.
- **Model picker in Settings** instead of a plain text field:
  - `deepseek-v4-flash` — DeepSeek V4 Flash (fast, recommended, default)
  - `deepseek-v4-flash-vision-exp` — experimental model that also accepts images
  - `deepseek-v4-pro` — highest quality, slower
  - `deepseek-chat` / `deepseek-reasoner` — legacy aliases for accounts that still use them
  - **Custom model id…** — free text for any other model (including local OpenAI-compatible servers)
- The settings panel explains the default, and "Test connection" reports which model answered, so you can confirm the routing before using AI on real documents.

## Verification

- Request routing verified against a local mock endpoint: the captured outgoing request used `model=deepseek-v4-flash` with `stream=true`.
- `cargo test -p aicore` (12 tests) still passes; the desktop build, installer and portable package are built for **1.2.1**.

## Privacy reminder (unchanged)

AI stays off until you add your own key and confirm the per-document data notice. Only extracted text is sent; the key is DPAPI-encrypted on disk; every other tool in the app remains fully offline.
