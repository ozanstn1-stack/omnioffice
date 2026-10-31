# Platform breadth decisions (D12)

D12 asks each extra platform to be **either shipped with the same verification
as Windows/Linux/Android, or explicitly declined with the reasons**. This page
records the three decisions taken for the v7.1.0 train, the evidence behind
them, and the trigger that would reopen each one. Nothing here is a promise of
future work; it is a dated decision record.

Current shipped platforms: **Windows** (Setup/Portable, Authenticode-gated),
**Linux** (`.deb` + `.AppImage`, SHA-256), **Android** (arm64-v8a and
armeabi-v7a APK/AAB, release keystore). macOS is compiled and tested in CI on
every push, but not packaged. The Chrome extension is a separate web artifact.

## Decision table

| Platform | Decision for v7.1.0 | Reason | Revisit trigger |
|---|---|---|---|
| macOS (.dmg, notarized) | **Decline packaging** (CI build/test continues) | Apple Developer Program cost (~$99/year), notarization requires a macOS signing identity and a notarization round-trip on a macOS runner; no user report asking for a mac build yet, and the release train has no mac hardware to verify the result. Shipping an unnotarized `.dmg` would be worse than not shipping one: Gatekeeper blocks it by default. | A mac user explicitly asks for a build **and** the Apple account + a macOS verification machine are available. Then: sign + notarize, verify with `spctl --assess --type execute`, and add the same asset/SHA verification as Windows before calling it shipped. |
| Flatpak (Flathub or side-loaded) | **Decline for v7.1** | The sandbox model conflicts with two core flows: the document vault indexes user-picked folders across the filesystem, and the PDF engines are prebuilt binaries outside the app bundle. A Flatpak would need portal-based file access for every flow plus engine packaging (or a degraded flavour), which is a multi-week effort with no requested user story. `.deb` + `.AppImage` already ship with SHA-256 and the same CI verification. | A Linux user asks for Flatpak **and** the engine binaries can be packaged as a runtime extension without breaking the size budget. Then validate open/save through portals for vault, PDF tools and office documents before listing. |
| Windows ARM64 | **Defer** | Cross-compiling Tauri + NSIS for ARM64 is supported but the NSIS ARM64 toolchain and the WebView2 ARM64 runtime story need a real device test; the CI matrix would add a runner class with no measured demand (the app collects no telemetry, so demand is only user reports). | A user reports running Windows on ARM and needs the app natively, or a CI ARM64 Windows runner becomes available. Then build, sign with the same Authenticode provider, and verify on a device before listing. |

## What this means in practice

- The supported release matrix does not change in v7.1.0. No "coming soon"
  claims are added to the README or the listing metadata.
- macOS keeps its CI job (`cargo build`/`cargo test` on `macos-latest`), so the
  code stays portable; only packaging/notarization is declined.
- The decisions are reversible per platform without touching the other two;
  each revisit trigger names the verification that must pass before "shipped".

## Türkçe özet

- **macOS paketleme: hayır** (CI'da derleme/test sürüyor). Gerekçe: Apple
  geliştirici ücreti (~99 $/yıl), notarization için macOS kimliği ve doğrulama
  makinesi, henüz kullanıcı talebi yok; imzasız `.dmg` Gatekeeper'a takılır.
- **Flatpak: v7.1 için hayır.** Kasa tarama akışı ve paket dışı motor ikilileri
  sandbox/portal modeliyle çok haftalık iş; `.deb` + `.AppImage` zaten SHA-256
  ile yayınlanıyor.
- **Windows ARM64: ertelendi.** NSIS/WebView2 ARM64'ün gerçek cihazda testi ve
  CI koşucu maliyeti; talep yok (telemetri toplamıyoruz).
- Her maddenin yeniden açılma koşulu ve o zaman geçmesi gereken doğrulama
  yazılı. Desteklenen matris v7.1.0'da değişmiyor.
