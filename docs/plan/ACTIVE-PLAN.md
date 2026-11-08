

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
