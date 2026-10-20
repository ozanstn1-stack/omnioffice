# i18n dictionaries

`en.json` and `tr.json` are the translation source of truth. The app
imports them with `?url` and fetches them at runtime (a JSON asset, not a
JS chunk), so the bundle budget measures behaviour, not strings; tests
import the same files directly.

Key parity and usage are checked by `npm run i18n:audit -- --check`; the
mojibake guard is `src/lib/i18n-encoding.test.ts`.
