

---

# v7.22.0 YAYINDA (tag bekleniyor) — sonra PR 5
**#674 birleşti** (`fdd03ae`, 17 başarı + 2 beklenen skip). İzleyici `/tmp/watch_rel22.sh`.

# PR 5: basit font önbelleği + gerçek ölçüm (Opus `e218ac9d`)
## Karar
v7.22.0'daki önbelleği **basit fontlara** (`/Widths`, `/Encoding`, `/Differences`, descriptor) da
uygula **ve** aynı PR'da `examples/nup_bench.rs` ile önbellek açık/kapalı ölçümü yap. İkisi tek
başına eksik: önbellek ölçümsüz hız iddiası, ölçüm yarım kalmış işin ölçümü olurdu.

## Kritik kapı (Opus adım 2)
Önce **ölç**, sonra karar ver: basit font çözümü duvar-saatinin **%5'inden azsa** basit-font
önbelleğini bırak, yalnız ölçümü + kompozit sayıları gönder. Ölçüm tek binary içinde
`budget 0` (önbellek kapalı ≈ eski davranış) ile varsayılan bütçeyi karşılaştırır.

## Adımlar
1. **Ortak fixture** (`crates/pdfcore/tests/common/font_fixtures.rs`): CID font + basit font +
   `/DR` kurucuları; benchmark ve yeni test kullanır. (Önceki üç test dosyasını yeniden yazmıyorum —
   sapma: risk/boyut; kurucular oradan *kopyalanmıyor*, ortak dosyadan alınıyor.)
2. **Gizli giriş noktası**: `#[doc(hidden)] pub fn nup_pdf_bytes_with_budget(input, options, budget)`
   → `build_nup` → `carry_markup(budget)`. Normal giriş 32 MiB ile çağırır.
3. **`examples/nup_bench.rs`** (criterion yok, yalnız `std::time::Instant`): 500 sayfa × 4 not,
   `--fonts simple|composite|mixed`, 5 tur, medyan+min, budget 0 vs varsayılan; iki çıktı
   farklıysa non-zero exit.
4. **Karar**: basit font payı ≥%5 ise basit-font önbelleği (`SimpleFont`, `parse_simple_font`,
   `FontCache::simple`, `simple_parses` sayacı; anahtar **ObjectId**, asla kaynak adı; satır içi
   font sözlükleri önbelleksiz).
5. Testler `tests/nup_simple_font_cache.rs`: `simple_font_metrics_parsed_once_per_document`,
   `same_resource_name_different_objects_not_confused`, `direct_font_dict_resolved_without_cache`,
   `refused_simple_font_cached_as_refusal`, `budget_zero_output_byte_identical`,
   `budget_exhaustion_still_correct`.
6. CHANGELOG + RELEASE_READINESS (ölçüm, komutla birlikte, "VPS ölçümü" etiketiyle).

## Kapsam dışı
CI'da zaman kapısı (üç OS'ta gürültülü), criterion/yeni bağımlılık, çağrılar arası önbellek,
Type3, dört çizim katmanını birleştirme, bellek ölçümü.


## v7.22.0 DOĞRULANDI · PR #675 BİRLEŞTİ · v7.23.0 TRENİ (2026-11-08 06:15 UTC)
- **v7.22.0**: tag `fdd03ae`, **20 asset**, gövde 509 bayt (CHANGELOG ile birebir), `latest.json` +
  imza, `.sig` 444 B, `SHA256SUMS.txt` 8 satır / 8 OK, zarflar 12+5, `coverage` exit 0.
- **#675 `df61102`**: basit font önbelleği + ölçüm; 18 başarı + 2 skip; dal silindi.
  Ölçüm: kompozit **896 → 183 ms**, basit **205 → 178 ms** (2000 not, release, 5 tur medyanı).
- **Yakalanan iki hata**: (1) CHANGELOG'a girdi eklenince türetilmiş fastlane metni bayatladı →
  `fastlane-changelogs` yeniden üretildi; (2) rebase yeni girdiyi **yayınlanmış `[7.22.0]`
  bölümünün içine** düşürmüştü → girdi `[Unreleased]`'a taşındı, çift `### Changed` silindi,
  `changelog-section.mjs v7.22.0` yayınlanan gövdeyle eşleşiyor. Ayrıca RELEASE_READINESS'daki
  7.23.0 bölümü kaybolmuştu (rebase) → yeniden eklendi, `claims:check` 406/563.
- **v7.23.0**: branch `release/v7.23.0`; CHANGELOG `[7.23.0]`, sürüm 7.23.0, versionCode 7023000,
  README, RELEASE_READINESS, fastlane 7023000. Kapılar: `release-guard` OK, `claims:check` 406/563,
  `repro:check` OK. **Sıradaki:** push → PR → CI → merge → tag → yayın doğrulaması.

---

# v7.23.0 YAYINDA — PR 6: kompozit lookup indeksi + bellek kanıtı (Opus `f168682d`)
**Karar:** gerçek CJK boyutunda (≈20 000 `/ToUnicode` girdisi) kompozit yolu **önce ölç**; küçük font
medyanının ≥1.5 katı çıkarsa `CodeIndex`/`AdvanceIndex` (ikili arama) yap, çıkmazsa yalnız bellek
kanıtını gönder. Ayrıca 32 MiB bütçe **tahminle değil ölçülen tahsisle** kanıtlanır (±25%).

## Neden
- 183 ms/178 ms sonucu yalnız fixture'ın küçük haritası için geçerli; gerçek Japonca/Çince
  fontlarda 7k–20k girdi var ve `DocCid::code`/`advance` her karakterde doğrusal yürüyor.
- Bütçe bugün yalnız insert-time tahmin; koleksiyon overhead'i eksik sayılırsa gerçek tepe 32 MiB'ı
  aşabilir — Android'de bu OOM demek.
- Tamamı VPS'te yerel doğrulanabilir (test + clippy + `nup_bench`).

## Adımlar
0) `nup_bench`'e `composite-large` senaryosu (20k `/ToUnicode`, tam `/CIDToGIDMap`, ~4k `/W`) +
   sayan global allocator ile tepe heap; **kapı**: büyük medyan ≥1.5× küçük medyan mı?
1) Ortak test fixture'ı `tests/common/cid_fixture.rs` (üç mevcut test dosyasındaki ~150 satırı
   paylaşıma açar, yeni ölçek testi dördüncü kez kopyalamasın).
2) (Kapı ateşlerse) `appearance/cidfont/index.rs`: `CodeIndex` (sorted `(char, u16)`, ikili arama,
   çift kayıtta **en küçük kod** kuralı korunur) + `AdvanceIndex` (örtüşmeyen run'lar, `/DW` yedeği);
   naif doğrusal aramayla LCG testleri.
3) `DocCid` indeksleri `Arc` ile tutar; `widths.rs`'teki per-CID arama silinir (dosya küçülür).
4) `cache.rs`: `heap_bytes` kapasitelerden; `FontCacheStats`'a `resident_bytes`, `peak_bytes`,
   `refused_over_budget`. `nup/bench.rs`: `impose_with_cache(doc, opts, CacheMode)`.
5) `tests/nup_freetext_cid_scale.rs` (bayt-aynı, haritanın başı/ortası/sonu, `refused == 0`) ve
   `tests/font_cache_memory.rs` (tahmin ±%25, 64 büyük fontta tepe ≤ baseline + 4 MiB×1.1,
   `peak ≤ budget`).

## Kapsam dışı
Basit font indeksi (256 girdi zaten hızlı), 32 MiB'ı değiştirmek/LRU, dört katmanı birleştirme,
Android'de ölçüm, Type3, `/W` fuzz köprüsü.

## v7.23.0 DOĞRULANDI · PR #677 BİRLEŞTİ · v7.24.0 TRENİ (2026-11-08 07:20 UTC)
- **v7.23.0**: tag `a6e0802`, **20 asset**, gövde 760 bayt (CHANGELOG ile birebir), `latest.json`
  7.23.0, `.sig` 444 B, `SHA256SUMS.txt` 8 satır / 8 OK, `coverage` exit 0.
- **#677 `304cc0e`**: bellek kanıtı (18 başarı + 2 skip). Ölçüm: kompozit 808→166 ms,
  kompozit-büyük 13.27 s→223 ms (1.35× küçük harita → indeks YOK), basit 223→179 ms;
  tepe heap sığan önbellekte ~6 MB vs ~21 MB önbelleksiz.
- **v7.24.0**: branch `release/v7.24.0`; CHANGELOG `[7.24.0]`, sürüm 7.24.0, versionCode 7024000,
  README, RELEASE_READINESS, fastlane 7024000. Kapılar: `release-guard` OK, `claims:check`
  409/566, `repro:check` OK, `changelog-section v7.23.0` yayınlanan gövdeyle eşleşiyor.
  **Sıradaki:** push → PR → CI → merge → tag → yayın doğrulaması.

---

# v7.24.0 YAYINDA — Opus `403fc6b3` KAPISI: CJK metin çıkarma
**Karar:** metin çıkarmayı CJK fontlarında ölç, bozuksa çizim için kurulan CMap yığınıyla düzelt.
**Kapı A sonucu: DÜŞTÜ — dilim yapılmayacak.**
- Uygulamanın metin yolu `pdfcore::render::extract_page_text` → **pdfium** (`page.text()`); pdfium
  CJK'yı zaten doğru okuyor.
- `lopdf::Document::extract_text` yalnız **testlerde** kullanılıyor (`tests/common/mod.rs`,
  `stamp_test`, `incremental_mark_test`, `perf_test`); hiçbir Tauri komutu ona gitmiyor
  (`src-tauri` yalnız `render::extract_page_text` çağırıyor: commands.rs:266, ai.rs:718,
  office_tools.rs:170, office.rs:990/1051).
- Opus'un kendi kuralı: "hiçbir kullanıcıya dönük özellik bu koda ulaşmıyorsa dur" → duruldu.
  Bu kapı bir dilimi (ve bir sürüm trenini) boşa gitmekten kurtardı.
- **Sıradaki adaylar** (hepsi kullanıcıya dönük değil ya da araç kısıtlı): (i) fixture dedupe,
  (ii) `/W` fuzz köprüsü, (iii) Type3, (iv)/(v) editör/Android (VPS'te derlenemiyor).

## v7.24.0 DOĞRULANDI · PR #679 BİRLEŞTİ · v7.25.0 TRENİ (2026-11-08 08:40 UTC)
- **v7.24.0**: tag `84c2258`, **20 asset**, gövde 688 bayt (CHANGELOG ile birebir), `latest.json`
  7.24.0, `.sig` 444 B, `SHA256SUMS.txt` 8 satır / 8 OK, `coverage` exit 0.
- **#679 `ab9824c`**: resize sonrası açıklama konumları + crop orijini (18 başarı + 2 skip).
- **v7.25.0**: branch `release/v7.25.0`; CHANGELOG `[7.25.0]`, sürüm 7.25.0, versionCode 7025000,
  README, RELEASE_READINESS 7.25.0 bölümü (3 `test:` işareti), fastlane 7025000.
  Kapılar: `release-guard` OK, `claims:check` 412/569, `repro:check` OK, `changelog-section
  v7.24.0` yayınlanan gövdeyle eşleşiyor.
- **Ders (üçüncü kez)**: `str.replace` sessizce başarısız oluyor (anchor metni tutmazsa). Her
  RELEASE_READINESS/CHANGELOG düzenlemesinden sonra `grep` ile doğrula — bu kez 7.25.0 bölümü
  yazılmamıştı, yakalandı ve eklendi.

## KESİNTİ ÇÖZÜLDÜ · v7.25.0 etiketlendi · #681 birleşti · v7.26.0 treni (2026-11-08 11:25 UTC)
- Kesinti ~90 dk sürdü (pdfium/qpdf/tesseract 504). Çözülünce master'ın kırmızı işleri `rerun-failed-jobs`
  ile temizlendi → Desktop yeşil → **Auto release tag → v7.25.0 = `78cd033`** → Release + Android koşuyor.
- **#681 birleşti** (`59d30ad`): flatten gizli açıklamayı artık basmıyor (prob buldu).
- **v7.26.0**: sürüm 7.26.0, versionCode **7026000**, CHANGELOG `[7.26.0]`, README, RELEASE_READINESS
  bölümü 7.26.0'a taşındı, fastlane 7026000. Kapılar: `release-guard` OK, `claims:check` **415/575**,
  `repro:check` OK.
- **Sıradaki**: v7.25.0 yayın doğrulaması (20 asset) + v7.26.0 push → PR → CI → merge → tag → doğrulama.
- **Ders**: `str.replace` anchor'ları bu turda `assert` ile korundu (üç kez sessiz düşme olmuştu).

## v7.26.0 ETİKETLENDİ · #683 birleşti (2026-11-08 12:30 UTC)
- **v7.26.0** tag `491f5ad`; Release workflow yayınlıyor (Windows installer sürüyor), Android yine
  `ANDROID_KEYSTORE_BASE64` yüzünden düştü (kullanıcı adımı).
- **#683 birleşti** (`a206fc1`): repair probu, **10/10 yeşil, kusur yok** → sürüm çıkarılmadı.
  Oracle hatası düzeltildi (miras alınan `/MediaBox`, 7.7.3.4).
- **Sıradaki**: v7.26.0 desktop doğrulaması (14 asset) → sonra `inspect` probu (Opus yedeği:
  outline round-trip + `evaluate_findings` yanlış "her şey yolunda").

## v7.26.0 DOĞRULANDI (desktop) — 2026-11-08 13:05 UTC
- Release workflow **success**; **14 asset** (Linux 2 + Windows 2 + 3 SHA256SUMS + build-info +
  latest.json + 2 SBOM + `.sig` 444 B + desktop zarfı). Gövde == CHANGELOG (389 bayt),
  `SHA256SUMS.txt` **8/8 OK**, `coverage` → *"every published asset is covered by 1 envelope"*.
- **Eksik: 5 Android asset + android zarfı** → `ANDROID_KEYSTORE_BASE64` yok (kullanıcı adımı).
- Disk temizlendi.
- **Sıradaki**: `inspect` probu (Opus `85f53da8` yedeği) — outline round-trip + `evaluate_findings`.

## PR 8: inspect probu (Opus `57f42463`) — 2026-11-08 14:20 UTC
- Branch `feat/inspect-probe`, **PR #684**. Prob 8 vaka; **bir gerçek hata**:
  `a11y.missing-title` ekranın gösterdiği başlığı (dosya adına düşen) test ediyordu → `/Title`'ı
  olmayan belge "sorun yok" diyordu. Düzeltme: bulgu `title_override`'a bakıyor; gösterim geri
  düşüşünü koruyor. Mevcut test (`a_file_name_stands_in_for_a_missing_title`) çelişkide ISO'ya
  (PDF/UA 7.1) göre ikiye ayrıldı.
- İlk koşudaki 3 kırmızının **ikisi fixture hatasıydı** (pdfa bulgusu gerçekti; outline nesneleri hiç
  kurulmamıştı) — sınıflandırma PR gövdesinde.
- Yerel: prob **8/8**, lib **344**, clippy temiz, `inspect.rs` 1078 → **1077** (bütçe içeriden).
- **Sıradaki**: CI yeşilse squash merge → **v7.27.0 sürüm treni** (kullanıcıya dönük düzeltme var).

## #686 birleşti · v7.28.0 sürüm PR'ı açıldı — 2026-11-08 17:25 UTC
- **#686** `c60bc9f`: annotate koordinat düzeltmeleri (20 kontrol, 18 başarı + 2 skip).
- **v7.28.0**: sürüm 7.28.0, versionCode **7028000**, CHANGELOG, README, RELEASE_READINESS,
  fastlane. Kapılar: `release-guard` OK, `claims:check` **422/589**, `repro:check` OK.
- **Sıradaki**: CI yeşilse merge → tag → **20 asset** doğrulaması (Android keystore mevcut).

## v7.28.0 birleşti (`f0d2387`) — 2026-11-08 17:52 UTC
- #687 squash merge; 19 kontrol (17 başarı + 2 skip). Master Desktop → auto-tag bekleniyor.
- **Sıradaki**: `v7.28.0` tag → Release + Android → **20 asset** doğrulaması; sonra yeni dilim (Opus).

## PR 9: açıklama görünen kutuya oturur (Opus `cfc40e22`) — 2026-11-08 17:2x UTC
- **Opus planı `cfc40e22`** (kod okumadan yazıldı; Adım 0 kapısı bana bırakıldı).
- **Adım 0 sonucu: K2.** Ön uç `pointToDisplay` ile **gösterilen** sayfaya göre nokta üretiyor
  (`src/screens/annotate/geometry.ts`), boyut `geometry.display_width_pt` = pdfium
  `page.width()`/`height()` → **CropBox ∩ MediaBox**, `/Rotate` uygulanmış
  (`src/screens/Annotate.tsx:401`). Yani sözleşme "görünen kutu" uzayı; arka uç yalnız
  `/MediaBox` kullanıyor.
- **Bulunan iki gerçek hata:**
  1. `apply_annotations` (bake: `annotate_pdf`, `annotate_pdf_incremental`) kutu **orijinini**
     hiç eklemiyor → `[100 200 695 1042]` sayfada işaret (100,200) kayıyor; ters `/MediaBox`
     negatif boy üretiyor (v7.28.0'ın düzelttiği hatanın bake yolundaki eşi).
  2. `/CropBox` yok sayılıyor: kırpılmış sayfada tuval kırpılmış bölgeyi gösterdiği hâlde işaret
     `(crop.x0 − media.x0, crop.y0 − media.y0)` kayıyor.
- **Dilim**: `annotate/geometry_util.rs`ine görünen-kutu yardımcısı (CropBox ∩ MediaBox, normalize,
  miras, ters kutu) + iki yol da onu kullanır; prob `annotate_box_probe.rs`.
- **Sıradaki**: prob kırmızı → düzelt → lib testleri + clippy + `check-file-sizes` → PR.

## v7.28.0 DOĞRULANDI (20 asset) — 2026-11-08 19:55 UTC
- Release + **Android** ikisi de **success** (Android keystore artık mevcut).
- **20 asset**: Linux 2 + Windows 3 (Setup, Portable, NSIS `x64-setup`) + `.sig` **444 B** +
  SHA256SUMS 4 dosya + build-info + latest.json + 2 SBOM + 2 Android zarfı + 4 Android çıktısı
  (arm64/v7a × apk+aab).
- Gövde **883 bayt** ve `node scripts/changelog-section.mjs v7.28.0` ile **birebir** (diff boş).
- `latest.json` → **7.28.0**. `SHA256SUMS.txt` **8 satır**, 8/8 GitHub'ın asset `digest` alanıyla
  eşleşiyor (indirme gerekmedi: API `sha256:` özeti veriyor).
- `provenance-subjects.mjs coverage` → *"every published asset is covered by 2 envelope(s)"*, exit 0.
- `cargo clean`: 16.8 GiB silindi; disk **%61** (39 GB boş).

## PR 9: açıklama görünen kutuya oturur — **#688** açıldı, 2026-11-08 20:05 UTC
- Dal `feat/annotate-box` (`efc8c21`), **Opus planı `cfc40e22`** (kod okumadan yazıldı; Adım 0 bana kaldı).
- **Adım 0 = K2**: ön uç gösterilen-kutu uzayı gönderiyor (`src/screens/annotate/geometry.ts` +
  `Annotate.tsx:401`, boyut pdfium `page.width()` → CropBox ∩ MediaBox, `/Rotate` uygulanmış).
- **İki gerçek hata**: (1) bake yolu kutu orijinini hiç eklemiyordu → kaydırılmış/ters kutuda işaret
  kayıyor veya sayfa dışına çıkıyordu; (2) iki yol da `/CropBox`'ı yok sayıyordu → kırpılmış sayfada
  işaret kırpma ofseti kadar kayıyordu.
- Prob `tests/annotate_box_probe.rs` **9 vaka**: düzeltme öncesi **7/9 kırmızı**, sonrası **9/9**;
  düzeltmeden önce de yeşil olan 2 vaka (düz sayfa, crop==media) kayma olmadığını kanıtlıyor.
- Düzeltme `annotate/geometry_util.rs`: `visible_box(media, crop)` (normalize + kesişim + boş/kopuk
  crop'ta media'ya dönüş); iki yol boyutu **ve** orijini oradan alıyor; bake `translate(x0,y0)` ekliyor.
  Sayfa-ağacı yürüyücüleri de oraya taşındı → `annotate.rs` **1475 → 1438**.
- Kapılar: prob 9/9, eski prob 5/5, lib **344/344**, clippy temiz, `check-file-sizes` OK.
  `batch_test::batch_watermark_and_compress_all_files` bu VPS'te pdfium yokluğu yüzünden düşüyor
  (raster stratejisi) — bilinen Gate B sınırı, değişiklikle ilgisiz.
- **Sıradaki**: #688 CI yeşilse squash merge → **v7.29.0** sürüm treni (kullanıcıya dönük yerleşim düzeltmesi).

## #688 birleşti (`eee0033`) · v7.29.0 sürüm PR'ı — 2026-11-08 20:50 UTC
- **#688** squash merge (`eee0033`), dal silindi. Master Desktop → auto-tag **no-op** (sürüm dosyası
  hâlâ 7.28.0), beklenen davranış.
- **Ders**: yeni test dosyası README istatistik bloğunu bayatlatıyor → Lint işinin "Project statistics"
  adımı düştü (Rust tests 1822 → 1831). **Push'tan önce `npm run docs:stats`**; bu bir sürüm dilimi
  değil, her yeni test dosyasında geçerli.
- **v7.29.0**: sürüm 7.29.0, versionCode **7029000**, CHANGELOG `[7.29.0]` bölümü + `[Unreleased]`
  korunuyor, README (sürüm, "What's new (6.2 to 7.29.0)", indirme bağlantıları), RELEASE_READINESS
  başlığı + `(this cycle, 7.29.0)` bölümü, fastlane 7029000 (en-US üretildi, tr-TR elle),
  `cargo update --workspace --offline`, `files:update`, `docs:stats`.
- Kapılar: `release-guard` OK (7.29.0 / 7029000), `claims:check` **424 iddia / 593 işaret**, 97 döngü
  bölümü, güncel döngü 7.29.0.
- **Sıradaki**: PR → CI → squash merge → tag `v7.29.0` → Release + Android → **20 asset** doğrulaması.

## PR 10: filigran, damga ve evidence görünen kutuya oturur (Opus `001b1e05`) — 2026-11-08 21:15 UTC
- **Opus planı `001b1e05`** (kod okumadan yazıldı; Adım 0 bana kaldı, sonuçlar aşağıda).
- **Adım 0 sonuçları (doğrulandı):**
  - `watermark.rs::paint_page` (208-211): boyut `page_mediabox`'tan, `display_to_page(rot, w, h)` —
    **kutu orijini yok, `/CropBox` yok**. Yerleşim `position_origin(display_w, display_h, …)` ile
    görünen boyuta göre seçiliyor ama sayfaya yerleştirme orijinsiz → kaydırılmış/kırpılmış sayfada
    filigran kayıyor, "center" kırpılmış bölgenin ortasına değil medya kutusunun ortasına düşüyor.
  - `stamp.rs::PageCanvas::new` (145-151): aynı desen; tip **private**, uygulama crate'i kullanmıyor.
    Metin `Td` ile `canvas.to_page` uzayında yazılıyor → aynı kayma.
  - `evidence.rs::stamp_page` (376-389): orijini ekliyor (`translate(media[0], media[1])`,
    kompozisyon sırası doğru) ama **`/CropBox` yok**.
  - `page_mediabox` normalize **etmiyor** (ham dizi) → ters kutu negatif boy.
  - Bütçeler: watermark 332, stamp 490, evidence 643, `docutil` 1137 (dolu), `annotate.rs` 1438/1475,
    `annotate/geometry_util.rs` 98 (taban 200 altı → ratchet dışı).
- **Yardımcı yeri**: Opus `pagegeom.rs` (yeni dosya, ratchet dışı) önerdi; ben `annotate/geometry_util.rs`
  içindeki `visible_box`'ı **`docutil.rs`'e taşımadan**, `pagegeom.rs`'e taşıyıp `annotate` tarafında
  yeniden dışa aktaracağım — böylece annotate bağımlılığı tersine dönmez.
- **Prob**: `tests/watermark_box_probe.rs` (+ stamp/evidence vakaları aynı dosyada), içerik akışı
  yürüyücüsü (`q`/`Q`/`cm` yığını, `Do` → birim kare, `Td` → nokta). İki tür doğrulama:
  (1) **diferansiyel** — aynı seçeneklerle F1 (kutu 0,0) ile F2 (kutu 100,200) arasında matris
  yalnız `(100,200)` kaymalı; (2) **değişmez** — "center" filigranın merkezi görünen kutunun merkezi,
  damganın sağ-alt çapası görünen kutunun sağ-alt köşesi eksi kenar boşluğu.
- **Sıradaki**: prob kırmızı → yardımcı + üç çağrı noktası → kapılar → PR.

## v7.29.0 DOĞRULANDI · #690 birleşti (`ce03a50`) — 2026-11-08 22:30 UTC
- **v7.29.0** tag `8c2650e`; Release + Android **success**, **20 asset** (14 desktop + 6 Android).
- Gövde **647 bayt** == `node scripts/changelog-section.mjs v7.29.0` (diff boş). `latest.json` **7.29.0**,
  `.sig` **444 B**, `SHA256SUMS.txt` **8/8** API `digest` eşleşmesi, `coverage` → *"every published
  asset is covered by 2 envelope(s)"*.
- **#690** squash merge (`ce03a50`): filigran, sayfa numarası ve kanıt etiketi görünen kutuya oturuyor;
  kural `pagegeom.rs` (`visible_box`, `page_visible_box`, `display_frame`). Prob 8 vaka: önce **7/8
  kırmızı** (kaydırılmış sayfada `(300,400)` yerine `(400,600)`, ters kutuda `(-300,-400)`, kırpılmış
  sayfada medya kutusunun merkezi), sonra **8/8 yeşil**.
- **Flake notu**: macOS Rust işi `synccore` LAN testinde `Address already in use (os error 48)` ile
  düştü; `jobs/<id>/rerun` ile yeşile döndü — kodla ilgisiz, tekrarında rerun yeterli.
- **Sıradaki**: yeni dilim (Opus'a sorulacak). Adaylar: evidence kendi testleri, N-up markup,
  inspect'in kalan kuralları (link hedefi, şifreleme).

## PR 11: evidence etiketi probu (Opus `7767eb27`) — 2026-11-08 22:55 UTC
- **Opus planı `7767eb27`**: evidence bundle için E/G/C/V/M gruplarında ~60 vaka önerdi. **Kapsam
  daraltıldı**: bu oturumda yalnız **G grubu** (etiket geometrisi) + çok kanıtlı numaralandırma
  yapıldı; V/M/E grupları bir sonraki dilime kaldı (gerekçe: süre + sessiz saat 01:00 UTC).
- Yeni prob `crates/pdfcore/tests/evidence_label_probe.rs` (4 vaka, hepsi **yeşil**):
  kutu orijini kaydırılmış sayfa, `/CropBox` içeride, `/Rotate 90`, iki kanıtlı bundle.
- **Kusur bulunamadı** → sürüm çıkarılmadı (repair probu deseni). Prob, son dilimde değiştirdiğim
  `evidence.rs::stamp_page` çağrısını (tek test edilmeyen çağrı noktasıydı) sabitliyor.
- Prob yazarken iki **fikstür hatası** düzelttim: (1) `cm` kompozisyon sırası ters (CTM·M olmalı),
  (2) R90 için "merkeze uzaklık korunur" varsayımı yanlış (etiket sağ-üst köşeye hizalı olduğu için
  dönüşte uzaklık değişir) — beklenen değer matristen elle hesaplandı.
- **Sıradaki**: PR aç → CI → merge; sonra vaka seti V/M/E gruplarına genişletilebilir.

## PR 12: evidence verify/match probu (Opus `7767eb27`, V+M grupları) — 2026-11-08 23:40 UTC
- `crates/pdfcore/tests/evidence_verify_probe.rs` **8 vaka, hepsi yeşil** (kusur yok → sürüm yok).
- Kapsam: taze bundle doğrular; **sayfa içeriği düzenlenirse** `content_matches` false; **kayıtlı özet
  bozulursa** false; 200 bayt kesilmiş dosya `Err` (panik yok); manifest'siz PDF `present=false`;
  aynı orijinal → `Some(1)`; farklı/kısaltılmış/yeniden sıralanmış belge → `None`; boş liste/boş dosya `Err`.
- Prob yazarken üç fikstür hatası düzelttim: `/Contents` **dizi** olabiliyor (7.8.2); manifest
  **pretty-printed** olduğu için özet araması `"sha256": "` + `rposition` ile; 10 bayt kesme lopdf'te
  hâlâ ayrıştığı için 200 bayt kullandım.
- **Kapı dersi**: yeni test dosyası `git add` edilmeden `check-format.mjs rust --all-changed` "değişen
  dosya yok" diyor → **önce `cargo fmt -p pdfcore`, sonra `git add`, sonra format kapısı**.
- **Sıradaki**: PR aç → CI → merge. Sonra E grubu (numaralandırma/bozuk girdi) veya N-up/inspect dilimi.

## PR 13: evidence sıra/numaralandırma probu (Opus `7767eb27`, E grubu) — 2026-11-08 23:55 UTC
- `crates/pdfcore/tests/evidence_order_probe.rs` **4 vaka, hepsi yeşil** (kusur yok → sürüm yok).
- Elle hesaplanan beklentiler **birebir tuttu**: 1 kapak + 1 dizin (15 satır) + 5 kanıt sayfası = **7**;
  A→`3..5`, B→`6..7`. Girdi sırası ters verilince sıra korunuyor (B→`3..4`, A→`5..7`). İç içe sayfa
  ağacında nesne numarası değil **ağaç sırası** okunuyor (7.7.3.2). 0 baytlık kanıt `Err`.
- **Sıradaki**: PR aç (00:45'ten önce), CI'yı sessiz saat sonrası (04:00 UTC) kontrol et ve merge et.

## PR 14: font adı çakışması — GERÇEK HATA (Opus `a03f0e81` C grubu) — 2026-11-09 05:20 UTC
- **Bulunan gerçek hata**: damga/sayfa numarası/kanıt etiketi fontunu sayfaya `/OmniTr` adıyla
  yazıyordu. Bu uygulamanın daha önce damgaladığı her dosyada o ad zaten var → **üzerine yazılıyordu**;
  eski damga metni yeni subset fontla çizilip bozuk görünüyordu. Prob kırmızıydı, düzeltme olmadan
  kırmızı kaldığı stash'li ağaçta doğrulandı.
- **Düzeltme**: `textfont::register_page_font` — sayfada kullanılmayan adı seçer (`OmniTr`, `OmniTr2`…),
  kaydeder ve **kullanılan adı döndürür**; çizilen metin o adı kullanır. `evidence.rs`, `stamp.rs`
  (private kopyası silindi) ve `numbering.rs` aynı yardımcıya geçti.
- Prob `crates/pdfcore/tests/evidence_content_probe.rs` **4 vaka**: miras `/Resources`, paylaşılan
  `/Contents`, Flate içerik, font adı çakışması (kırmızıydı → yeşil).
- Bütçeler: `stamp.rs` 490 → **452**, `evidence.rs` 640 → **641** (sınır 643), `numbering.rs` 241 (sınır),
  `textfont.rs` 87 → 136 (taban altı, izlenmiyor).
- Kapılar: lib **344**, evidence probları 4+4+4+8, clippy temiz, `docs:stats` güncel, `files` OK.
- **Sürüm gerekli**: kullanıcıya dönük düzeltme → **v7.29.1** (sıradaki pencerede 10:00 UTC sonrası).

## #694 birleşti (`5d47f53`) · v7.29.1 sürüm PR'ı — 2026-11-09 10:10 UTC
- **#694** squash merge: ikinci damga ilkinin fontunu koruyor (`register_page_font`).
- **v7.29.1**: sürüm 7.29.1, **versionCode 7029001** (formül: major×1e6 + minor×1e3 + patch —
  7029100 yanlıştı, `release-guard` yakaladı), CHANGELOG `[7.29.1]` + `[Unreleased]` korunuyor,
  README (sürüm, "What's new (6.2 to 7.29.1)", indirme bağlantıları), RELEASE_READINESS başlığı +
  `(this cycle, 7.29.1)` bölümü, fastlane 7029001 (en-US üretildi, tr-TR elle), `cargo update --offline`,
  `files:update`, `docs:stats`.
- Kapılar: `release-guard` 7.29.1 (7029001) OK; `claims:check` **426 iddia / 597 işaret**, 98 döngü
  bölümü; `fastlane-changelogs --check` OK; `files` OK (363 dosya).
- **Sıradaki**: PR → CI → squash merge → tag `v7.29.1` → Release + Android → **20 asset** doğrulaması.

## PR 15: sayfa kaynak kaydı bütünlüğü (Opus `85b34163`) — 2026-11-09 10:30 UTC
- **Opus planı `85b34163`**: son düzeltmenin (`/OmniTr` çakışması) kök nedenini komşu yollarda sına:
  aynı sayfaya iki damga (header + Bates), paylaşılan `/Resources`, miras alınan `/Font`, sayfa 2'ye
  dokunmama, orijinal içerikten sızan CTM.
- **Planın "prob gücü kontrolü"** şartı uygulanacak: `register_page_font` geçici olarak sabit ada
  döndürülüp probun kırmızıya döndüğü gösterilecek (dönmezse prob zayıftır).
- Kapsam: **V2–V5** (kaynak kaydı) + **V6** (sızan CTM) — V1 (ExtGState) elendi: kodda sabit
  ExtGState/XObject adı yok, hepsi nesne kimliğinden türetiliyor (`GS{id}`, `WM{id}`, `AN{id}`).
- **Prob gücü kontrolü**: `register_page_font` geçici olarak sabit `/OmniTr`'ye döndürüldü → 3 vaka hâlâ
  yeşil kaldı (**prob zayıftı**), bu yüzden gerçek senaryo eklendi: *bu uygulamanın damgaladığı dosyaya
  tekrar damga* → sabit adla kırmızı, düzeltmeyle yeşil. Opus'un şartı böylece sağlandı.
- Sonuç: **4 vaka yeşil, yeni kusur yok** → sürüm yok (font çakışması v7.29.1'de düzeltilmişti; bu prob
  onu ve komşularını kilitliyor).

## v7.29.1 etiketlendi · #696 birleşti (`6fbb7d0`) — 2026-11-09 11:00 UTC
- **Tag gecikmesinin sebebi**: master Desktop `ceabb2a` koşusu `Desktop E2E (tauri-driver)` →
  "Secret store (Xvfb)" adımında düştü (`ai-key-deepseek.bin does not hold the saved key` — anahtar
  deposu flake'i). Auto-tag **doğru davrandı** (kırmızı CI'ya tag yok). `rerun-failed-jobs` → yeşil →
  **v7.29.1 tag `ceabb2a`** → Release + Android koşuyor.
- **#696** squash merge: sayfa kaynak sözlüğü probu (4 vaka). **Prob gücü kontrolü** dersi: ilk sürüm
  sabit ada döndürülünce yeşil kalıyordu (zayıf prob) → gerçek senaryo eklendi (damgalanmış dosyaya
  tekrar damga), sabit adla kırmızı / düzeltmeyle yeşil.
- **Sıradaki**: v7.29.1 asset doğrulaması (20 asset); sonra Opus'a sor.

## PR 16: sağlık raporu bağlantıları + belgenin kendi kısıtları (Opus `e3c4b597`) — 2026-11-09 11:20 UTC
- **K1 sonucu**: `DocumentInspection`'a yeni **alan** eklemek UI sözleşme testini kırar (`src/screens/contract.test.ts`
  her alanın `Inspect.tsx`'te kullanılmasını şart koşuyor) ve bindings üretimi CI'da. Bu yüzden dilim
  **bulgu (finding)** olarak tasarlandı: bulgu listesi UI'da jenerik çiziliyor → yeni alan/binding/UI yok.
- **K2 sonucu**: lopdf şifreli dosyayı yüklerken **çözüyor ve trailer'daki `/Encrypt`'i düşürüyor**, bu yüzden
  `/P` yalnız **ham baytlardan** okunabiliyor (`declared_permissions(path)`: `/Encrypt N G R` → nesne → `/P`).
- Yeni modül `crates/pdfcore/src/inspect/links.rs` (277 satır): `count_links` (URI/GoTo/kırık/diğer, `/A` > `/Dest`
  12.6.2, isim ağacı çözümü 12.3.2.4, döngü koruması), `declared_permissions` (7.6.4.2), `findings`.
  `inspect.rs` **1077 → 1064** (bütçe içinde, `destination_page` modüle taşındı).
- Prob `tests/inspect_links_probe.rs` **6 vaka yeşil** + **mutasyon kontrolü** (bit indeksi kaydır, `/A`
  önceliğini boz, kırık sayımını kaldır → her biri en az bir vakayı kırmızıya döndürdü).
- **Sürüm**: yeni kullanıcıya dönük özellik → **v7.30.0** (minor) sıradaki pencerede.

## v7.29.1 DOĞRULANDI · #697 birleşti (`20e2c8f`) · v7.30.0 sürüm PR'ı — 2026-11-09 11:50 UTC
- **v7.29.1**: **20 asset** (14 desktop + 6 Android), gövde 759 bayt == `changelog-section v7.29.1`,
  `latest.json` 7.29.1, `.sig` 444 B, `SHA256SUMS` **8/8**, provenance → "every published asset is
  covered by 2 envelope(s)". (Tag gecikmesinin sebebi E2E "Secret store (Xvfb)" flake'iydi; rerun yeşil.)
- **#697** squash merge: sağlık raporu kırık linkleri (`link.broken-internal`) ve belgenin beyan ettiği
  `/P` kısıtlarını (`security.permissions`) bulgu olarak veriyor. `inspect.rs` **1077 → 1064**.
- **v7.30.0**: sürüm 7.30.0, versionCode **7030000**, CHANGELOG `[7.30.0]`, README, RELEASE_READINESS
  (`(this cycle, 7.30.0)`), fastlane 7030000 (en-US üretildi, tr-TR elle), `cargo update --offline`,
  `files:update`, `docs:stats`.
- Kapılar: `release-guard` 7.30.0 (7030000) OK; `claims:check` **429 iddia / 599 işaret**, 99 döngü
  bölümü (bir iddiaya `(no test: …)` gerekçesi eklendi — kapı yakaladı).
- **Sıradaki**: PR → CI → merge → tag `v7.30.0` → Release + Android → 20 asset doğrulaması.

## PR 17: izin bayrakları probu (Opus `89eb7e02`) — GERÇEK HATA — 2026-11-09 12:20 UTC
- Prob `crates/pdfcore/tests/permissions_probe.rs`: `/P` değerini **ham baytlardan** okuyup sabit ISO
  tamsayılarıyla karşılaştırıyor (yazıcı ile okuyucu aynı hatayı paylaşmasın diye).
- **Doğru çıkanlar**: hepsi açık → `-4`, hepsi kapalı → `-3392`, tek tek kapalı → `-20` (kopyalama),
  `-2056` (yazdırma), `-1036` (düzenleme), `-292` (yorum; bit 9 form doldurma da bu bayrağa bağlı —
  seçeneklerde ayrı bayrak yok, ürün kararı olarak belgelendi).
- **BULUNAN GERÇEK HATA (`#[ignore = "BUG: …"]`)**: `owner_password == user_password` olduğunda
  `protect_pdf` kısıtları yazıyor ama **açan kişi owner parolasını da doğrular** (ISO 32000-1 Algoritma 2.A),
  yani "kopyalanamaz" belge kopyalanabilir kalıyor. **UI bunu varsayılan yapıyor**:
  `Security.tsx:76` → `ownerPassword: ownerPassword || userPassword`.
- **Sıradaki dilim (v7.30.1 adayı)**: `protect_pdf` kısıt istendiğinde owner boş/eşitse ya reddetsin ya da
  rastgele owner üretsin; UI'da owner alanı boş bırakılabilsin + i18n mesajı; sürüm notunda "daha önce
  korunmuş belgeler kendiliğinden düzelmez, yeniden korunmalı" uyarısı.

## PR 18: kısıtları bağlayan sahip parolası (Opus `9089f72c`) — GERÇEK HATA DÜZELTMESİ — 2026-11-09 13:10 UTC
- **Opus kararı `9089f72c`**: seçenek 1 — en az bir izin kapalıyken owner boş veya user'a eşitse
  `protect_pdf` **`InvalidInput` döndürür**; tüm izinler açıksa eşit parola kabul edilir (bağlanacak
  bir kısıt yok). Gerekçe: reddetme hiçbir şey üretmez ve hemen düzeltilebilir; rastgele owner üretmek
  kullanıcının kendi kısıtlarını kaldırmasını engellerdi.
- **Backend**: `security.rs` (+11 satır, bütçe dışı dosya) — kısıt + (owner boş | owner == user) → hata.
- **UI**: `Security.tsx` artık `ownerPassword || userPassword` göndermiyor; kontrol saf yardımcıya alındı
  (`src/screens/security/protection-guard.ts`) ve **4 vitest** ile sınandı. `Security.tsx` 241 → **254**
  (bütçe `--allow-raise` ile bir kez yükseltildi; gerekçe: sessiz güvenlik yanlış beyanını engelleyen
  kontrol). i18n: `security.ownerRequiredForRestrictions`, `security.ownerMustDiffer` (tr+en).
- Prob: `#[ignore]` kaldırıldı; vaka artık üç durumu sınıyor (eşit/boş + kısıt → `Err` ve dosya yok;
  hepsi açık + eşit → `Ok`).
- Kapılar: prob 5/5, lib testleri yeşil (ortam kaynaklı 4 kırmızı değişmedi), clippy temiz,
  `i18n-audit` yeni sabit metin yok, `contract.test.ts` 5/5, guard testi 4/4, `claims:check` 429/599.
- **Sürüm**: **v7.30.1** — sürüm notunda "daha önce korunmuş belgeler kendiliğinden düzelmez, farklı
  owner parolasıyla yeniden korunmalı" uyarısı.

## v7.30.0 DOĞRULANDI (20 asset) — 2026-11-09 13:10 UTC
- **20 asset** (14 desktop + 6 Android), gövde 883 bayt == `changelog-section v7.30.0`, `latest.json`
  7.30.0, `.sig` 444 B, `SHA256SUMS` **8/8** (API `digest`), provenance → "every published asset is
  covered by 2 envelope(s)". Target temizlendi (16.1 GiB), disk %61.

## #700 birleşti (`a9b61c6`) · v7.30.1 sürüm PR'ı — 2026-11-09 14:45 UTC
- **#700**: kısıtlar artık bağlıyor — `protect_pdf` owner boş/eşitse ve bir izin kapalıysa reddediyor;
  UI kontrolü `protection-guard.ts` (4 vitest); i18n tr+en. İlk CI'da iki kırmızı vardı, ikisi de
  düzeltildi: (1) `tools-screens.test.tsx` **eski hatayı sabitliyordu**, (2) `docs:stats` bayat.
- **v7.30.1**: sürüm 7.30.1, **versionCode 7030001**, CHANGELOG `[7.30.1]` (+ eski belgeler için uyarı),
  README, RELEASE_READINESS `(this cycle, 7.30.1)`, fastlane 7030001 (en-US üretildi, tr-TR elle),
  `cargo update --offline`, `files:update`, `docs:stats`.
- Kapılar: `release-guard` 7.30.1 (7030001) OK; `claims:check` **432 iddia / 603 işaret**, 100 döngü
  bölümü.
- **İki kapı dersi** (plana): yeni test dosyası → `docs:stats` **dosyalar kesinleştikten sonra**;
  yeni TS dosyası → `check-format.mjs frontend --base=origin/master` (`--all-changed` yenisini görmüyor).

## #701 birleşti (`a9fc3c2`) — v7.30.1 tag bekleniyor — 2026-11-09 15:30 UTC
- **#701** squash merge. CI'da tek kırmızı "Release claims"ti: `claims-audit --junit` işareti **JUnit
  test-case adıyla** eşleştiriyor; `#protectionProblem` bir `describe` başlığıydı → gerçek `it` adına
  çevrildi (`#refuses an owner password that matches the user password`).
- **Kapı dersleri (bu oturum, üçü de plana yazıldı)**:
  1. Yeni test dosyası → `docs:stats` **dosyalar kesinleştikten sonra**.
  2. Yeni TS dosyası → `check-format.mjs frontend --base=origin/master` (`--all-changed` yenisini görmüyor).
  3. TS iddia işareti **`it(...)` adı** olmalı, `describe` değil (`--junit` eşleşmesi).
- **Sıradaki**: master Desktop → auto-tag `v7.30.1` → Release + Android → **20 asset** doğrulaması.

## v7.30.1 DOĞRULANDI (20 asset) — 2026-11-09 16:20 UTC
- **20 asset** (14 desktop + 6 Android), gövde 854 bayt == `changelog-section v7.30.1`, `latest.json`
  7.30.1, `.sig` 444 B, `SHA256SUMS` **8/8** (API `digest`), provenance → "every published asset is
  covered by 2 envelope(s)". Disk %61.
- **Kapanış**: bu oturumda 20 PR; son üç sürüm (7.29.0 → 7.30.1) ve iki gerçek hata (font çakışması,
  bağlamayan izin kısıtı) kapandı.
- **Sıradaki dilim**: Opus'a sorulacak — öneri: güvenlik hattı (`/Perms` şifreli özeti) veya organize
  sonrası link/outline hedeflerinin yeniden eşlenmesi.

## PR 19: silinen sayfaya giden başvurular temizlenir (Opus `a0a45dbd`) — GERÇEK HATA — 2026-11-09 16:55 UTC
- **Opus kararı `a0a45dbd`**: (b) — sayfa silmede hedef temizliği. Ön koşul sağlandı:
  `pdfcore::organize::delete_pages` VPS'te çalışıyor.
- **Prob `tests/organize_dest_probe.rs` (5 vaka) düzeltmeden önce 5/5 kırmızıydı**: bir sayfa
  silinince A) link hâlâ silinen sayfayı gösteriyordu, B) `/OpenAction` kalıyordu, C) outline girdisi
  silinen sayfaya atlıyordu, D) isim ağacı girdisi ölü hedef taşıyordu, E) silinen sayfanın **içeriği
  dosyada erişilebilir kalıyordu** (gizlilik).
- **Düzeltme**: yeni `crates/pdfcore/src/dest_prune.rs` (`prune_references_to`) — link `/Annots`'tan
  çıkarılır, outline girdisi başlığını koruyup hedefini kaybeder, `/OpenAction` ve ölü isim ağacı
  girdileri silinir; isimler önce çözülür (12.3.2.3). `delete_pages` bunu ağaç yeniden kurulduktan
  sonra çağırır. **Mutasyon kontrolü**: bağlantı geri alınınca 5/5 kırmızı, açıkken 5/5 yeşil.
- Kapılar: lib testleri yeşil (ortam kaynaklı 4 kırmızı değişmedi), clippy temiz, `files` OK
  (`dest_prune.rs` 338, test 316 — ratchet'e eklendi), `claims` 432/603, `docs:stats` güncel.
- **Sürüm**: **v7.30.2** (yama; V5 gizlilik yönü nedeniyle sürüm notunda ayrıca vurgulanacak).
- **Kapsam dışı**: `/StructTreeRoot`, `/AcroForm` widget'ları, `/Threads` — sonraki aday.

## PR 20: yapı ağacı, form ve thread temizliği (Opus `9eec3e2b`) — GERÇEK HATA — 2026-11-09 17:20 UTC
- **Opus kararı `9eec3e2b`**: #702'nin kapsam dışı bıraktığı üç başvuru türü. Prob
  `crates/pdfcore/tests/organize_pageref_probe.rs` **5 vaka**: yapı ağacı, form alanı, makale ipliği,
  yeniden sıralama (kontrol), çıkarma.
- **Düzeltme öncesi 4/5 kırmızı** (kontrol vakası yeşildi): silinen sayfanın içeriği `/StructTreeRoot`
  ve `/AcroForm /V` üzerinden **dosyada kalıyordu**; thread boncukları ölü sayfayı gösteriyordu;
  `extract_pages` kalan belgede ölü link bırakıyordu.
- **Düzeltme**: yeni `prune_struct.rs` (165) ve `prune_forms_threads.rs` (194); temizlik
  `apply_page_plan`'a taşındı → **silme, çıkarma ve plan** hepsi kapsanıyor. `delete_pages` artık
  plana delege ediyor. **Mutasyon kontrolü**: çağrılar geri alınınca 5/5 kırmızı, açıkken 5/5 yeşil.
- Kapılar: lib testleri yeşil (ortam kaynaklı 4 kırmızı değişmedi), clippy temiz, `files` OK,
  `claims` 432/603, `docs:stats` güncel.
- **Sürüm**: **v7.30.2** — #702 ile aynı yamaya katılıyor (henüz yayımlanmadı).

## #703 birleşti (`8d19bfb`) · v7.30.2 sürüm PR'ı — 2026-11-09 17:45 UTC
- **#703**: yapı ağacı, form alanları ve makale iplikleri de temizleniyor; temizlik
  `apply_page_plan`'a taşındı (silme + çıkarma + her plan). Prob 5/5, mutasyon kontrolü geçti.
- **v7.30.2**: sürüm 7.30.2, **versionCode 7030002**, CHANGELOG (gizlilik yönü + "eski dosyalar
  kendiliğinden düzelmez, yeniden silin/çıkarın" uyarısı), README, RELEASE_READINESS (iki bölüm
  `(this cycle, 7.30.2)`), fastlane 7030002 (en-US üretildi, tr-TR elle), `cargo update --offline`,
  `files:update`, `docs:stats`.
- Kapılar: `release-guard` 7.30.2 (7030002) OK; `claims:check` **441 iddia / 612 işaret**, 102 döngü
  bölümü; `fastlane --check` OK; `files` OK.
- **Sıradaki**: PR → CI → merge → tag `v7.30.2` → Release + Android → 20 asset doğrulaması.

## PR 21: eylem zincirleri + sayfa etiketleri (Opus `0625bf63`) — GERÇEK HATA — 2026-11-09 19:40 UTC
- **Opus kararı `0625bf63`**: (b) daraltılmış — `/AA`, `/Next`, `/SE`, `/PageLabels`. Prob
  `tests/probe_page_plan_residue.rs` 5 vaka; **düzeltme öncesi 5/5 kırmızıydı** (ikisi yalnız nesne
  numarası yeniden atandığı için; probe'u gezinmeyle düzelttim).
- **Gerçek kusurlar**: (1) korunan widget'ın `/AA` eylemi silinen sayfaya atlıyor ve **içeriği dosyada
  tutuyor**, (2) `/Next` zinciri aynı şekilde, (3) outline `/SE` ölü yapı elemanını gösteriyor,
  (4) `/PageLabels` indeksleri kayıyor (i, ii, 1, 2, 3 → i, 1, 2, 3 olması gerekirken i, ii, 1, 2).
- **Düzeltme**: yeni `prune_actions.rs` (275) — eylem adım adım budanır (`/D` ölüyse adım gider,
  `/Next` özyineli + döngü koruması, `/AA` olayları tek tek), `/SE` temizliği; yeni `page_labels.rs`
  (105) — aralık başlangıçları hayatta kalan ilk sayfaya kayar, `/St` kaydırılır, ardışık eşit
  aralıklar birleşir. OBJR vakası `prune_struct` tarafından zaten karşılanıyordu (prob artık sabitliyor).
- **Mutasyon kontrolü**: çağrılar geri alınınca 5/5 kırmızı (biri hariç — OBJR), açıkken 5/5 yeşil.
- Kapılar: lib testleri yeşil (ortam kaynaklı 3-4 kırmızı değişmedi), clippy temiz, `files` OK
  (yeni dosyalar ratchet'e eklendi), `claims` 441/612, `docs:stats` güncel.
- **Sürüm**: **v7.30.3** (yama; `/AA` ve `/Next` gizlilik yolu).

## #705 birleşti (`373d426`) · v7.30.3 sürüm PR'ı — 2026-11-09 19:55 UTC
- **#705**: `/AA` eylemleri, `/Next` zincirleri, outline `/SE` ve `/PageLabels` planla birlikte
  temizleniyor; prob 5/5, mutasyon kontrolü 4/5 kırmızı.
- **v7.30.3**: sürüm 7.30.3, **versionCode 7030003**, CHANGELOG (gizlilik notu + "eski dosyalar
  kendiliğinden düzelmez"), README, RELEASE_READINESS `(this cycle, 7.30.3)`, fastlane 7030003
  (en-US üretildi, tr-TR elle), `cargo update --offline`, `files:update`, `docs:stats`.
- Kapılar: `release-guard` 7.30.3 (7030003) OK; `claims:check` **446 iddia / 617 işaret**, 103 döngü
  bölümü; fastlane + files + docs:stats OK.
- **Sıradaki**: PR → CI → merge → tag `v7.30.3` → Release + Android → 20 asset doğrulaması.

## PR 22: buton durumu sessizce bozulmuyor (Opus `308f37ca`) — GERÇEK HATA — 2026-11-09 22:05 UTC
- **Opus kararı `308f37ca`**: (a) form durumu. Prob `tests/button_state_probe.rs` 4 vaka;
  **düzeltmeden önce 1 kırmızı**: belgenin bildirmediği bir durum adı istenince kod sessizce ilk
  bildirilen duruma düşüp **uyarı** veriyordu → `/V` ile `/AS` çelişebiliyor ya da hiç görünümü
  olmayan bir durum yazılıyordu (ISO 32000-1 12.5.5 Tablo 168).
- **Düzeltme** `forms.rs`: onay kutusu ve radyo için bilinmeyen durum artık `InvalidInput` döndürüyor,
  hiçbir şey yazılmıyor. `fill_checkbox`/`fill_radio` artık kullanılmayan `report` parametresini aldı →
  satır bütçesi korundu (`forms.rs` 2752 → **2756**, +4; `--allow-raise` yerine küçültmeyle).
- **Yeşil kalan üç vaka** (kayıtlı): bildirilen açık durumu (`/Ja`) hem `/V` hem `/AS`'ye yazma,
  `Off`'a çekme, `/Yes` kullanan belgede `/Yes` — yani mevcut davranış zaten doğruydu.
- **Kapsam dışı**: radyo `/Opt` etiketleri ve çocuk başına `/AS` — sonraki dilim.
- Kapılar: lib testleri yeşil (ortam kaynaklı 3 kırmızı değişmedi), forms_test 14/14, clippy temiz,
  `files` OK, `claims` 446/617, `docs:stats` güncel.
- **Sürüm**: **v7.30.4** (yama).

## #708 birleşti (`9871a3e`) · v7.30.4 sürüm PR'ı — 2026-11-09 22:45 UTC
- **#708**: bilinmeyen buton durumu artık `InvalidInput`; prob 4 vaka (3'ü zaten doğruydu).
  Bindings kırmızısı: doc comment değişikliği `specta.ts`'e yansıdı, üretilen iki satır elle uygulandı
  (`8a234c1`).
- **v7.30.4**: sürüm 7.30.4, **versionCode 7030004**, CHANGELOG, README, RELEASE_READINESS
  `(this cycle, 7.30.4)`, fastlane 7030004 (en-US + tr-TR), `cargo update --offline`, `files:update`,
  `docs:stats`.
- Kapılar: `release-guard` 7.30.4 (7030004) OK; `claims:check` **450 iddia / 620 işaret**; fastlane OK.
- **Sıradaki**: PR → CI → merge → tag → Release + Android → 20 asset; sessiz saat 01:00'den önce
  bitmezse yayın 04:00 sonrasına kalır.

## PR 23: radyo grubu `/Opt` adıyla doldurulabiliyor (Opus `308f37ca` kalan vakalar) — 2026-11-10 04:20 UTC
- Prob `crates/pdfcore/tests/radio_opt_probe.rs` 3 vaka; **1 kırmızı**: `/Opt [(Elma)(Armut)]`
  varken "Armut" ile doldurmak `InvalidInput` veriyordu (durumlar `/0 /1`). Diğer ikisi (değer sahada,
  `/AS` her çocukta; temizleme hepsini Off yapar) zaten doğruydu.
- **Düzeltme** `forms.rs::fill_radio`: yazdırılan ad, grubun kendi `radio_options` listesinden duruma
  çözülüyor (12.7.4.2.4, Tablo 227). `forms.rs` 2756 → **2761** (+5; bu dilimde tek bütçe artışı,
  `--allow-raise` ile belgelendi — alternatifi tek bir arama için ikinci modül açmaktı).
- Kapsam: radyo `/Opt` + çocuk `/AS`; Opus `308f37ca`'nın kalan iki vakası böylece kapandı.
- Kapılar: prob 3/3, button_state 4/4, forms_test 14/14, lib testleri yeşil (ortam kaynaklı 3 kırmızı
  değişmedi), clippy temiz, `files` OK, `claims` 450/620, `docs:stats` güncel.
- **Sürüm**: **v7.30.5** (yama).

## #710 birleşti (`7bd9d50`) · v7.30.5 sürüm PR'ı — 2026-11-10 04:45 UTC
- **#710**: radyo grubu `/Opt` yazdırılan adıyla doldurulabiliyor; prob 3/3.
- **v7.30.5**: sürüm 7.30.5, **versionCode 7030005**, CHANGELOG, README, RELEASE_READINESS, fastlane
  7030005 (en-US + tr-TR), `cargo update --offline`, `files:update`, `docs:stats`.
- **Belge kayması düzeltildi**: `RELEASE_READINESS.md` başlığı v7.30.1–7.30.4 trenlerinde
  güncellenmemiş (7.30.0'da kalmış) ve bir "(unreleased)" bölümü v7.29.0'dan beri öyle duruyordu;
  ikisi de bu PR'da düzeltildi. **Ders**: sürüm trenlerinde başlığı `assert` yerine `grep`+değiştir,
  yoksa yarım kalan script onu sessizce atlar.
- Kapılar: `release-guard` 7.30.5 (7030005) OK; `claims:check` **456 iddia / 627 işaret**; fastlane OK.
- **Sıradaki**: PR → CI. **Sessiz saat 06:00–10:00 UTC**: 05:50'den sonra push/merge yok; PR CI'da
  kalacak, 10:00'daki hatırlatmayla merge → tag → Release + Android → 20 asset.

## v7.31.0 sürüm PR'ı — yeni arayüz (Windows + Android) — 2026-11-10
- **Tasarım**: Claude Design tuvalinde onaylanan arayüz uygulandı: yeni renk/yüzey belirteçleri
  (tek çivit vurgu, gradyan yok, belge türü renkleri), kenar çubuğu açılır-kapanır merkezlere
  bölündü (`components/sidebar.tsx`), Yeni menüsü + arama + sabitlenenler, ana sayfada selamlama ve
  Oluştur satırı (`components/create-row.tsx`), telefon alt çubuğu Ana sayfa/Ofis/Tara/Asistan/Diğer.
- `App.tsx` kenar çubuğunu bileşene taşıyarak küçüldü (1155 → ~1090); `Home.tsx` 607 → 600.
- **v7.31.0**: sürüm 7.31.0, **versionCode 7031000**, CHANGELOG, README, RELEASE_READINESS, fastlane
  7031000 (en-US + tr-TR).
