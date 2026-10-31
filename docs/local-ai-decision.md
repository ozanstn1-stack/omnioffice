# Local AI v2 — the model bundling decision (D5)

C7 shipped four Turkish task packs on an **optional local provider** (Ollama or
LM Studio running on the same machine); no model is bundled with the
installer. D5 asked for a recorded decision about whether a tiny model should
be bundled in v7.0.0. This note records the trade-offs, the numbers behind
them, and the decision.

**Decision: keep the model out of the installer for v7.0.0.** Ollama/LM Studio
stays the default local path, the evaluation harness (below) becomes the gate
for any future bundled or downloaded model, and a bundled or one-click
downloaded model is revisited only when the criteria at the end are met.

## What "bundled" would cost

Candidate small instruction models and their quantized sizes:

| Model | Licence | Q4_K_M GGUF | RAM at 4k context | Notes |
|---|---|---|---|---|
| SmolLM2-360M-Instruct | Apache-2.0 | ≈ 0.22 GB | ≈ 0.6 GB | Smallest; weakest Turkish; fine for extraction-shaped tasks. |
| Qwen2.5-0.5B-Instruct | Apache-2.0 | ≈ 0.40 GB | ≈ 0.8 GB | Strong Turkish for its size; best size/quality ratio. |
| Qwen2.5-1.5B-Instruct | Apache-2.0 | ≈ 1.0 GB | ≈ 1.5 GB | Better reasoning; slower on phones. |
| Llama 3.2 1B-Instruct | Llama 3.2 Community Licence | ≈ 0.8 GB | ≈ 1.3 GB | Licence requires notice and acceptable-use terms; extra review for F-Droid. |

A model has to be shipped for **each packaging target**; a universal Android
APK would grow by roughly the model size plus the runtime, and the F-Droid /
IzzyOnDroid listings have practical size limits. The current Android APK is
about 34–36 MB per ABI and the desktop Setup about 53 MB, so a 0.4 GB model is
a 8–10× jump for the smallest useful option and closer to 20× for 1.5B.

The runtime is the other half of the story. `llama.cpp`-class inference added
to the Tauri app means: a native dependency in every build (including
reproducible F-Droid builds), a second CPU feature matrix, thermal budgets on
phones, and an update cadence tangled with the app's own updates. Ollama
already solves all of that, is a one-time install the user opts into, and keeps
the document text on the machine.

## What we get instead

- **Provider-agnostic packs:** C7's four packs and D5's three new ones
  (e-invoice, KEP notice, court petition) are data + a verifier. They work with
  Ollama, LM Studio or a cloud provider the user deliberately configures.
- **A measured gate:** `src/lib/ai-task-pack-eval.ts` scores every pack against
  golden answers and deliberately bad ones (broken arithmetic, wrong dates,
  missing fields). A model that loses JSON discipline or arithmetic is caught
  by a failing score instead of by a user. The suite floor is 90 % per pack and
  overall; today's golden run scores 100 %.
- **Strict JSON with one repair turn:** when an answer does not verify, the
  runner sends one repair question with the concrete problems. Two attempts is
  the budget; the answer the user sees is always the verified one when one
  exists.
- **Isolation is testable:** the runner takes the provider call as a parameter,
  so a test can prove neither the harness nor the runner touches `fetch` or
  telemetry; the only network user is the provider the user configured, and the
  local presets are a localhost server.

## Revisit criteria

Bundling (or a one-click "download the model" inside the app) is worth
revisiting when **all** of these hold:

1. A 0.5B-class model reaches ≥ 90 % on the eval suite in Turkish, for every
   pack, on a mid-range phone — measured, not promised.
2. The extra installer size is accepted for each channel (APK/AAB, Setup,
   Portable, deb/AppImage) and F-Droid reproducibility is still possible.
3. The licence review (notices in About, acceptable-use for Llama) is done.
4. A bundled model has a clear update story that does not force the app's own
   release cadence (separate download, checksum, retry, disk cleanup).
5. Thermal and battery measurements on a phone show a bounded session cost.

Until then, the local badge, the Ollama preset and this record stay the honest
answer. Nothing in this decision changes the promise that **no document text
leaves the machine in local mode**.

## Türkçe özet

- **Karar:** v7.0.0 için model installera gömülmeyecek. Ollama/LM Studio yerel
  yol olarak kalır.
- **Neden:** en küçük kullanışlı model (Qwen2.5-0.5B Q4) bile ~0.4 GB; APK/AAB
  ve Setup boyutunu 8–20× büyütür, her platformda ayrı paket, F-Droid'de
  yeniden üretilebilirlik ve lisans/atıf yükü getirir. llama.cpp sınıfı bir
  çalışma zamanı eklemek ısı/pil ve güncelleme zincirini uygulamaya bağlar.
- **Yerine:** sağlayıcıdan bağımsız 7 görev paketi, **ölçülen** değerlendirme
  koşum takımı (altın + kasıtlı hatalı yanıtlar; paket ve genel eşik %90),
  tek onarım turlu katı JSON sözleşmesi, hiçbir ağ/telemetri çağrısı yapmadığı
  testle kanıtlanan izolasyon.
- **Yeniden değerlendirme:** 0.5B sınıfı bir model Türkçede her pakette
  ≥ %90'a ulaşırsa, ek boyut her kanalda kabul edilirse, lisans/atıf ve
  bağımsız model güncellemesi çözülürse, telefon ısı/pil ölçümleri sınırlıysa.
