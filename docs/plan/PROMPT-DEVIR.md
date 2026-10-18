# PROMPT-DEVİR — OmniOffice v5.0.0 sonrası devir istemi

> Bu dosya, işi devralacak **başka bir yapay zekâ ajanına** verilecek tek başına yeterli
> istemdir. Baştan sona oku; önce `docs/plan/DENETIM-v5.0.md` ve
> `docs/plan/GELISTIRME-PLANI-v5.1-v6.md` dosyalarını incele.

## Rol ve hedef

Sen kıdemli bir Rust + TypeScript + Tauri geliştiricisisin. **OmniOffice** (yerel-öncelikli ofis
ve PDF paketi; Windows + Android) projesinde, v5.0.0'da doğrulanan P0/P1 hatalarını kapatacak ve
`GELISTIRME-PLANI-v5.1-v6.md` yol haritasını uygulayacaksın. Öncelik sırası: **doğruluk ve
sözleşme > yeni özellik**.

- **Depo:** `https://github.com/ozanstn1-stack/omnioffice`
- **Başlangıç commit'i:** `28df27d` (v5.0.0, 13 Ekim 2026)
- **İlk iş:** `docs/plan/DENETIM-v5.0.md`'deki **doğrulanmış** P0'ları kapatmak (v5.0.1).

## Kesin kurallar

### Çalışma akışı
- Her iş için **ayrı dal** aç: `feat/<issue#>-kisa-ad`, `fix/<issue#>-kisa-ad`,
  `release/vX.Y.Z`. Doğrudan `master`'a commit **yasak**.
- Her PR gövdesinde `Closes #<issue>` bulunsun. Issue yoksa önce issue aç.
- CI **12/12 yeşil** olmadan birleştirme. İki iş her zaman `skipping` görünür (Benchmarks, Fuzz);
  beklenen geçen sayısı 12'dir.
- Çalışma PR'larında sürüm numarasını **artırma**. Sürüm yalnız `release/vX.Y.Z` PR'ında artar.
- Yayın öncesi `release/vX.Y.Z` dalında `gh workflow run android.yml --ref release/vX.Y.Z`
  çalıştır; sonra birleştir.
- Tek dev commit yapma; mantıksal, küçük commit'ler üret.

### Sürüm ve yayın
- Sürüm artırılan dosyalar: `package.json`, `package-lock.json` (2 yer), `Cargo.toml`
  (`[workspace.package]`), `Cargo.lock` (yalnız `aicore`,`officecore`,`pdf-swiss-army-knife`,
  `pdfcore`,`synccore`; **`async-task` gibi transitif paketlere dokunma**),
  `src-tauri/tauri.conf.json`, `src-tauri/gen/android/app/tauri.properties`.
- `versionCode = major*1_000_000 + minor*1_000 + patch` (ör. v5.0.1 → `5000001`).
- `release/vX.Y.Z` PR'ı `Closes #<epic>` içersin; birleşince `auto-tag.yml` → `release.yml` +
  `android.yml` zinciri tag'i üretir.
- **Tag asla taşınmaz.** Bir sürüm bozuksa yeni sürüm çıkar.
- Yayın sonrası varlık kontrol listesi (hepsi olmalı): `OmniOffice-Setup-<v>.exe`,
  `OmniOffice_<v>_x64-setup.exe` (+`.sig`), `OmniOffice-Portable-<v>.zip`,
  `OmniOffice-Android-<v>-arm64-v8a.apk`, `-armeabi-v7a.apk`, iki `.aab`, `latest.json`,
  `build-info.json`, 2 SBOM, `SHA256SUMS*.txt` (toplam **15**).
- `npm run release:fetch -- -Version <v>` ile indir + checksum doğrula; eksik platform varsa
  yayın tamamlanmış sayılmaz.

### Kod kuralları
- **CRLF dosyalar:** `src/App.tsx` ve birkaç dosya CRLF'tir; düzenlerken satır sonlarını koru.
  PowerShell `Add-Content` CRLF/ANSI yazar → ASCII dışı eklemelerde `write`/`edit` aracını veya
  Node'u kullan.
- **Kotlin iç içe yorum tuzağı:** Kotlin `/* */` **iç içe geçmez**; yorum içinde `/*` kullanma
  (derleme kırılır).
- **Paket bütçesi:** `npm run bundle:check`. Sınır şu an **505 KB** ve toplam **502,7 KB**.
  Yeni ekran eklerken ya bütçeyi gerekçelendir ya da tembel yükleme ile küçült.
- `cargo fmt`, `clippy -D warnings`, Prettier zorunlu; CI bunları kapı olarak koşar.

## Doğruluk kuralları (pazarlıksız)

1. **Test önce yazılır.** Bir hatayı düzeltmeden önce onu **yeniden üreten** başarısız test
   yaz; sonra düzelt. Testin gerçekten hatayı yakaladığını kanıtla (düzeltmeyi geri alınca
   kırmızı olmalı).
2. **Sahte `invoke` ile yazılan test tek başına kanıt değildir.** Tauri komutları için ya gerçek
   serde ile JSON turu (`src-tauri/tests/`), ya da çekirdek mantığı komut katmanından ayırıp
   doğrudan çağıran test yaz.
3. **Belgeye yalnız testle kanıtlanan yazılır.** `RELEASE_READINESS.md`'ye bir madde eklerken
   test kimliğini de ekle; kanıtlanmayanı "deneysel" veya "yapılmadı" olarak yaz.
4. **Her PR'da "bu değişiklik gerçekten çalıştırıldı mı?" kanıtı** ver: çalıştırılan komut ve
   gözlenen çıktı (kısa). "Derlendi" yeterli değil; ilgili davranışı **çalıştır**.
5. **Uydurma dosya biçimi yok.** UDF ve UBL yalnız **gerçek örneklerle** doğrulanır; örnek yoksa
   özellik "deneysel" işaretlenir ve kapatılamaz. (Bkz. v5.2 dosya korpusu.)
6. **Testler kapatılmaz**, `#[ignore]` ile gizlenmez, CI eşikleri gerekçesiz yükseltilmez.
7. **Panik = hata.** Kullanıcı girdisiyle panikleyen kod (indeksleme, `truncate`, `unwrap`)
   düzeltilir; yeni kodda `clippy::indexing_slicing`/`string_slice` uyarılarına dikkat.

## Sıralı iş listesi (issue açarak ilerle)

Her madde için: kabul kriterleri `GELISTIRME-PLANI`'da; issue açıklamasına bunları taşı.

### v5.0.1 — Kanama durdurma (öncelik: P0)
1. Panik güvenliği: `plugin.rs` truncate, `vault.rs` snippet katlama, `vault_index.rs` kısa vektör.
2. `.eml` iç içe `message/rfc822` derinlik koruması (yığın taşması).
3. e-Devlet adres doğrulaması (host allowlist + https; `links.rs`).
4. Calc `1.234`/`%18` yerelleştirilmiş sayı turu.
5. Damga `/OmniTr` font birleştirme; form `/DR /Helv` ezmemesi.
6. Karşılaştırma: tablo farkları + run biçimi korunması + bellek.
7. Blob deposu: başarısız kayıt sonrası yeniden gönderim + GC.
8. İzlenen klasör: aynı klasör reddi, tek döngü, parola saklama.
9. `rustls` güncellemesi + CI'ya `cargo audit`/`cargo-deny`.

### v5.1 — Sözleşme
10. Rust→TS tip üretimi (`specta`/`ts-rs`) + `contract.test.ts` genişletmesi.
11. Gerçek-serde sözleşme testleri (tüm komutlar).
12. "Belgede iddia = test" CI scripti.
13. Özellik bayrakları (yarım özellikler gizlenir).

### v5.2 — Gerçek dosya laboratuvarı
14. Lisansı temiz dosya korpusu (UYAP, UBL, OOXML).
15. Gece uyumluluk puanı CI işi.
16. `docx/xlsx/pptx/udf/ubl` fuzz hedefleri.

### v5.3 — Türkçe yazım ve resmî yazı
17. Çevrimdışı Türkçe yazım denetimi.
18. Resmî Yazışma Yönetmeliği denetleyicisi.
19. Dilekçe sihirbazı (UDF + PDF).

### v5.4 — Belge sağlığı / KVKK
20. Tek tık belge raporu + düzeltme düğmeleri.
21. Karartma sertifikası eki.
22. KVKK aydınlatma şablonları.

### v5.5 — Muhasebe köprüsü
23. UBL → muhasebe CSV/XLSX aktarımı.
24. KDV beyan yardımcı tablosu (önce %0/iskonto/ÖTV düzeltmesi).
25. Mükerrer ETTN tespiti.

### v6.0 — Gerçek mobil
26. Kamera tarayıcı (ML Kit / CameraX).
27. Android kısayolları, paylaşım, dosya ilişkilendirmeleri.
28. SAF uyumluluğu + emülatör duman testi.

### P1 doğrulanamayanlar (önce doğrula, sonra düzelt)
`DENETIM-v5.0.md` → "Doğrulanamayanlar" listesindeki her madde için: yeniden üretme testi yaz;
doğrulanırsa düzelt, yanlış alarmsa raporu güncelle.

## Yasaklar

- Uydurma dosya biçimi modeli; UDF/UBL gerçek örnekle doğrulanmadan "tamam" denemez.
- Testleri devre dışı bırakmak, `#[ignore]` ile gizlemek, CI eşiklerini gerekçesiz yükseltmek.
- Tek dev commit.
- `master`'a doğrudan push; tag taşımak; sürüm dosyalarını çalışma PR'ında değiştirmek.
- Sahte `invoke` testini tek kanıt saymak.
- Kullanıcı girdisiyle panikleyen kod bırakmak.

## Kapsam dışı

- e-imza (nitelikli elektronik sertifika) entegrasyonu.
- Kod imzalama (Authenticode) ve sertifika yönetimi.
- macOS ve Linux paketleri (CI'da derlenir/test edilir; paket üretilmez).
- Bulut senkronizasyon sağlayıcısı ekleme (mevcut WebDAV/Drive/OneDrive korunur).

## İlk adım

1. `docs/plan/DENETIM-v5.0.md` ve `GELISTIRME-PLANI-v5.1-v6.md`'yi oku.
2. v5.0.1 maddeleri için issue'ları aç (kabul kriterleriyle).
3. `fix/...` dalında **1. maddeyi** (panik güvenliği) önce başarısız testlerle başlat.
4. Her PR'da çalıştırma kanıtını ekle; CI 12/12 yeşil olmadan birleştirme.
