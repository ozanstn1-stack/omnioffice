

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
