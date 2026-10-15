# DENETİM v5.0.0 — Çalıştırarak Doğrulama Raporu

- **Depo:** `ozanstn1-stack/omnioffice` · **Başlangıç commit'i:** `28df27d` (v5.0.0)
- **Denetim tarihi:** 14 Ekim 2026
- **Ortam:** Windows (win32), Node v24.14.1, Rust/Cargo 1.98.1
- **Yöntem:** Salt okunur tarama bulgularının her biri, gerçek kodu **çalıştırarak** yeniden üretildi.
  Rust bulguları için depo crate'lerine path bağımlılığı veren bir yardımcı crate
  (`audit-v5`) ve `src-tauri` içine geçici birim testleri kullanıldı; TypeScript bulguları
  gerçek modüllerle geçici Vitest testleriyle sınandı. Geçici testler kanıt alındıktan
  sonra geri alındı; depoda yalnız bu rapor ve plan dosyaları kalır.

## Durum özeti

| # | Bulgu | Önem | Durum |
|---|-------|------|-------|
| P0-1 | e-Devlet "Doğrula" sahte siteye gidebilir | P0 | **Doğrulandı** |
| P0-2 | İç içe `message/rfc822` yığın taşması (uygulama kapanır) | P0 | **Doğrulandı** |
| P0-3 | UTF-8 sınırında kesme panikleri (3 yer) | P0 | **Doğrulandı** |
| P0-4 | Calc `1.234` → 1234 ve `%18` → 18 | P0 | **Doğrulandı** |
| P0-5 | İkinci damga ilk damgayı bozuyor | P0 | **Doğrulandı** |
| P0-6 | Form doldurma `/DR /Helv`'i alt kümeyle değiştiriyor | P0 | **Doğrulandı** |
| P0-7 | Belge karşılaştırma tablo/biçim kaybediyor | P0 | **Doğrulandı** |
| P0-8 | Başarısız otomatik kayıt kurtarmayı kalıcı bozuyor | P0 | **Doğrulandı** |
| P0-9 | İzlenen klasör: aynı klasör üzerine yazıyor | P0 | **Doğrulandı (kısmi)** |
| P0-10 | UDF biçimi / `sign.sgn` kaybı | P0 | **Kısmen doğrulandı** |
| P0-11 | `rustls 0.23.45` (≥0.23.46 gerekli) | P0 | **Sürüm doğrulandı** |
| P1-1 | Tarif damga adımı boş gidiyor | P1 | **Doğrulandı** |
| P1-2 | Tarif karartma adımı yüklenemiyor | P1 | **Doğrulandı** |
| P1-3 | Fatura vergi kimliği rozeti hep kırmızı | P1 | **Doğrulandı (kod)** |
| P1-4 | Vault Türkçe katlama kaybı | P1 | **Doğrulandı (kod)** |
| P1-5 | CLI çıktı yönlendirme / çıkış kodu güvenilmez | P1 | **Yanlış alarm** |
| P1-6… | Diğer P1 maddeleri | P1 | **Doğrulanamadı** (aşağıda) |

---

## P0 bulgular (kanıtlı)

### P0-1. e-Devlet doğrulama adresi sahte siteye gidebilir — DOĞRULANDI

- **Dosya:** `src/lib/edevlet.ts:43` (`TURKIYE_URL`), `src-tauri/src/links.rs` (host denetimi yok).
- **Kök neden:** Desen `https?://[^\s"<>()]*turkiye\.gov\.tr[^\s"<>()]*` — (a) `http` kabul eder,
  (b) host'a sabitlenmemiş; adresin **herhangi bir yerinde** (yol/sorgu) dize geçmesi yeter.
- **Yeniden üretme (Vitest, gerçek modül):**
  ```
  hostile url -> { code: 'AB12CD34', url: 'https://evil.example.com/x?u=turkiye.gov.tr' }
  http url    -> { code: 'AB12CD34', url: 'http://turkiye.gov.tr/belge-dogrulama' }
  ```
- **Etki:** Belgeye gömülü kötü niyetli bağlantı, "e-Devlet'te doğrula" düğmesiyle açılır.
  Düğme yalnızca bağlantı açar ve kod gösterir (otomatik gönderim yok), ama kullanıcı resmî
  siteye gittiğini sanır.
- **Düzeltme yönü:** `new URL` ile **host** doğrula (`host === "turkiye.gov.tr"` veya `.gov.tr`
  allowlist), yalnız `https`; `links.rs` da aynı politikayı uygulasın.

### P0-2. İç içe `message/rfc822` yığın taşması — DOĞRULANDI

- **Dosya:** `crates/officecore/src/eml.rs:320` (`parse_eml(&decoded)`), `eml.rs:88` (`walk_part(..., 0)`).
- **Kök neden:** `walk_part`'ın `depth > 12` koruması var, ancak `message/rfc822` dalı
  `parse_eml`'i **yeniden** çağırıyor; sayaç 0'dan başlıyor. Koruma her seviyede sıfırlanır.
- **Yeniden üretme (yardımcı crate, gerçek `officecore::eml::parse_eml`):**
  ```
  audit-v5 eml-bomb 20000
  eml-bomb: depth=20000 bytes=1060056
  thread 'main' has overflowed its stack
  exit=-1073741571   (0xC00000FD STATUS_STACK_OVERFLOW)
  ```
- **Etki:** Küçük (~1 MB) bir `.eml` yığını taşırır; `panic=abort` yayın derlemesinde **süreç
  çöker**. Kullanıcı bir e-posta açtığında uygulama kapanır.
- **Düzeltme yönü:** Derinliği `parse_eml` imzasına taşı (`parse_eml_with_depth(bytes, depth)`),
  `walk_part`'tan geçir; ayrıca toplam parça/bayt sınırı koy.

### P0-3. UTF-8 sınırında kesme panikleri — DOĞRULANDI (3 yer)

Geçici birim testleriyle, kaynaktaki **birebir ifadeler** çalıştırıldı:

| Yer | Kaynak | Panik çıktısı |
|-----|--------|---------------|
| Eklenti PDF metni | `src-tauri/src/plugin.rs:629` `text.truncate(MAX_PDF_TEXT_BYTES)` | `assertion failed: self.is_char_boundary(new_len)` |
| Vault snippet | `src-tauri/src/vault.rs:2018` `chars[start..end]` | `range start index 140 out of range for slice of length 106` |
| Vault vektör | `src-tauri/src/vault_index.rs:217` `&self.vectors[start..start + self.dim]` | `range end index 8 out of range for slice of length 4` |

- **vault.rs kök neden:** `folded` (`to_lowercase` sonrası) ile `chars` (özgün) farklı **karakter
  uzaylarında**; `'İ'` küçük harfte 2 karaktere açıldığı için `folded` indeksi `chars`'ta taşırıyor.
- **vault_index.rs kök neden:** `embeddings.bin` kısa/bozuk olduğunda `vectors` `keys.len()*dim`'den
  az kalıyor; `truncate(expected)` yalnız **kısaltır**, eksikse doldurmaz; `search` anahtar
  konumuna göre dilimleyince taşar.
- **Düzeltme yönü:** `truncate` yerine `floor_char_boundary`/`char_indices`; snippet için ortak
  katlama (tek karakter → tek karakter, `fold_char`) kullan; `load`'ta vektör sayısını `keys`'e
  göre doğrula (eksikse indeksi geçersiz say, panikleme).

### P0-4. Calc yerelleştirilmiş sayı: `1.234` → 1234, `%18` → 18 — DOĞRULANDI

- **Dosya:** `src/lib/office-types.ts:1180` (`cellText`), `src/office/calc/cells.ts:67`,
  `src/office/calc/locale.ts:63` (`parseLocalizedNumber`).
- **Yeniden üretme (Vitest, gerçek modüller):**
  ```
  cellText=1.234 reparsed(tr)=1234
  percent -> 18 18
  ```
- **Etki:** `1,234` gösteren bir hücre düzenlenip yeniden ayrıştırıldığında **1234** olur
  (bin katı, sessiz veri bozulması). `%18` yazımı `0,18` değil `18` üretir.
- **Düzeltme yönü:** `cellText` yerel ondalık ayıracıyla serileştirsin (veya düzenleme yolunda
  metin yerine değer korunsun); yüzde işareti değeri 100'e bölsün.

### P0-5. İkinci damga ilk damgayı bozuyor — DOĞRULANDI

- **Dosya:** `crates/pdfcore/src/textfont.rs:89` (`ensure_page_font`), `docutil.rs:724` (`set`).
- **Yeniden üretme (yardımcı crate, gerçek `stamp_pdf_incremental`):**
  ```
  after stamp 1: ToUnicode maps 'A' = true
  after stamp 2: ToUnicode maps 'A' = false
  after stamp 2: ToUnicode maps 'B' = true
  --- stamp 2 ToUnicode --- 1 beginbfchar  <0025> <0042>
  ```
- **Etki:** İkinci damga, sayfanın `/Font /OmniTr` kaynağının üzerine **yeni alt küme** yazar.
  İlk damganın içerik akışı hâlâ `/OmniTr`'yi çizer ama glif kimlikleri artık ikinci alt kümeye
  göre çözülür → ilk damga yanlış/boş görünür. (`stamp-double.pdf` üretildi.)
- **Düzeltme yönü:** `ensure_page_font` mevcut `/OmniTr`'yi koruyup **birleştirsin** (glif
  kümesini genişlet), ya da her damga benzersiz bir kaynak adı kullansın.

### P0-6. Form doldurma `/DR /Helv`'i alt kümeyle değiştiriyor — DOĞRULANDI

- **Dosya:** `crates/pdfcore/src/forms.rs:756` (`register_appearance_font`).
- **Yeniden üretme (yardımcı crate, gerçek `forms::fill_fields`):**
  ```
  before fill: /DR /Font /Helv subtype = Type1   (belgenin base-14 Helvetica'sı)
  after fill:  /DR /Font /Helv subtype = Type0   (gömülü alt küme)
  ```
- **Etki:** Belgenin `/Helv`'i, yalnız doldurulan alanın gliflerini içeren bir alt kümeyle
  değiştirilir. Başka alanların `/DA (/Helv …)`'sı ve sonradan yazılan metin, alt kümede
  olmayan glifleri kaybeder.
- **Düzeltme yönü:** `/Helv`'i **ezme**; Türkçe fontu ayrı bir adla (`/OmniTr`) kaydet, `/Helv`
  base-14 kalsın; veya alt kümeyi belgenin tüm form metnini kapsayacak şekilde genişlet.

### P0-7. Belge karşılaştırma veri/biçim kaybediyor — DOĞRULANDI

- **Dosya:** `src/office/writer/compare.ts` (`diffDocuments` satır 100, `diffRuns` şablon satır 58).
- **Yeniden üretme (Vitest, gerçek modül):**
  ```
  diff blocks -> [ 'paragraph', 'paragraph' ]      (temel belgedeki tablo sessizce düştü)
  diff runs   -> her run run[0]'ın stilini alıyor   (bold=false metin bold oluyor)
  ```
- **Etki:** (a) Revize belgede kaldırılan tablo/bloklar **silinmiş olarak gösterilmez**, kaybolur;
  (b) paragraf içi farklı biçimler tek biçime düzleşir; (c) `commonTokens` tam `O(n·m)` tablo
  ayırır (uzun paragrafta bellek şişer).
- **Düzeltme yönü:** Blok hizalamayı LCS'e taşı, tablo farklarını hücre düzeyinde işaretle,
  run'ları biçim sınırlarında böl; LCS'i bantlı (banded) hale getir.

### P0-8. Başarısız otomatik kayıt kurtarmayı kalıcı bozuyor — DOĞRULANDI

- **Dosya:** `src/lib/blob-store.ts:97` (`BlobSendCache.pending`), `src/lib/office-store.ts`
  (`useRecovery.save`, hatayı yutar), `src-tauri/src/office.rs` (`recovery_gc_blobs` hiç çağrılmaz).
- **Yeniden üretme (Vitest, gerçek modül):**
  ```
  first=1 second=0     (IPC başarısız olsa bile blob "gönderildi" sayıldı)
  ```
- **Etki:** Tek bir başarısız `recovery_save_blobs`'tan sonra blob bir daha gönderilmez; Rust
  tarafında olmayan blob `recovery_load`'ta "eksik görsel" hatası verir → **o belgenin kurtarması
  kalıcı bozulur**. Çöp toplama (`recovery_gc_blobs`) hiç çalışmadığı için yetim blob'lar birikir.
- **Düzeltme yönü:** `pending`'i yalnız **başarılı** IPC sonrası işaretle (veya başarısızlıkta
  geri al); oturum sonunda/başlangıçta `recovery_gc_blobs` çağır.

### P0-9. İzlenen klasör — DOĞRULANDI (kısmi)

- **Dosya:** `src-tauri/src/recipes.rs` (`scan_once`, `watched_folder_start/stop`, `watched_folder_set`).
- **Aynı girdi/çıktı klasörü (RUNTIME):**
  ```
  same-dir: results=1 ok=Some(true) input_changed=true
  ```
  `folder == output_dir` iken çıktı yolu girdiyle aynı; tarif **girdi dosyasının üzerine yazıyor**
  (veri kaybı).
- **İki döngü (kod):** `WATCH_RUNNING` bayrağı `stop` → `start` arasında yarışır; `stop`'tan sonra
  bir sonraki uyku aralığında `start` çağrılırsa iki görev birden döner. Ayrıca arayüz
  `watched_folder_scan`'i arka plan döngüsüyle eşzamanlı çağırabilir.
- **Parola (kod):** `WatchedFolder.password` düz metin olarak `watched-folder.json`'a yazılır.
- **Düzeltme yönü:** `folder == output_dir` reddi; nesil (generation) sayacıyla tek döngü;
  parolayı OS anahtarlığında sakla veya çalışma anında sor.

### P0-10. UDF biçimi ve `sign.sgn` — KISMEN DOĞRULANDI

- **Dosya:** `crates/officecore/src/udf.rs` (`read_udf`/`write_udf`).
- **Çalıştırma:**
  ```
  write_udf bytes=531
  read_udf ok: blocks=1
  contains 'sign.sgn' hint: false
  ```
- **Durum:** Kendi yazdığımız UDF kendi okuyucumuzla tur atıyor, ama yazıcı `sign.sgn`
  üretmiyor (imza dosyası kaybolur). **Gerçek bir UYAP dosyasıyla dış uyumluluk bu ortamda
  doğrulanamadı** (üçüncü taraf örnek yok); biçim modeli uydurma olabilir. Bulgu "doğrulanamadı"
  değil, "iç tutarlılık doğrulandı, dış uyumluluk kanıtlanamadı".

### P0-11. `rustls 0.23.45` — SÜRÜM DOĞRULANDI

- **Kanıt:** `Cargo.lock:4599-4600` → `name = "rustls"` / `version = "0.23.45"`.
- **Not:** `cargo audit` bu ortamda kurulu değil; RUSTSEC uyarı numaraları bağımsız olarak
  numaralandırılamadı. Sürümün ≥0.23.46 gerektirdiği bilgisiyle **güncelleme gerekli**.

---

## P1 bulgular (doğrulananlar)

### P1-1 & P1-2. Tarif adımları — DOĞRULANDI

- **Dosya:** `src-tauri/src/recipes.rs:24-45` (`RecipeStep`), `src/screens/Recipes.tsx:30`.
- **Yeniden üretme (geçici birim testi, gerçek serde):**
  ```
  stamp step -> Stamp { header_footer: None, bates: None }   (arayüz headerFooter gönderiyor)
  redact options={} ok=false                                  (RedactionOptions serde default'suz)
  ```
- **Etki:** (a) UI'nin `headerFooter` alanı serde'de `header_footer` beklendiğinden yok sayılır →
  **damga adımı boş/no-op** çalışır. (b) `RedactionOptions` `Default` uygular ama alan/struct
  düzeyinde `#[serde(default)]` yok → `{ kind: "redact", options: {} }` **ayrıştırılamaz**;
  karartma adımlı tarife kaydedilemez/yüklenemez.
- **Düzeltme yönü:** `#[serde(rename_all_fields = "camelCase")]` (veya alan adlarını eşle);
  `RedactionOptions`'a `#[serde(default)]` ekle. Kalıcı çözüm: **tip üretimi** (aşağıdaki plan).

### P1-3. Fatura vergi kimliği rozeti — DOĞRULANDI (kod)

- **Dosya:** `src-tauri/src/office.rs:1439-1447` (`InvoiceQr`), `src/screens/Invoices.tsx:237`.
- **Kök neden:** Rust struct `#[serde(rename_all = "camelCase")]` **taşımıyor**; `tax_id_valid`
  olarak serileşir. Arayüz `qr.taxIdValid` okur → `undefined` → rozet **hep kırmızı**.
- **Düzeltme yönü:** Struct'a `rename_all = "camelCase"` ekle (veya TS alanını eşle); sözleşme
  testiyle kilitle.

### P1-4. Vault Türkçe katlama — DOĞRULANDI (kod)

- **Dosya:** `src-tauri/src/vault_index.rs:61` (`tokenize` → `to_lowercase`),
  `src-tauri/src/vault.rs:1448` (`fold_char`, Türkçe i/İ/ı/I → i) — indeks bunu **kullanmıyor**.
- **Etki:** "İstanbul" tokenı `i̇stanbul` (birleşik noktalı) olur; `istanbul` sorgusu eşleşmez.
- **Düzeltme yönü:** `tokenize` ve sorgu normalizasyonunda `fold_char`'ı kullan (1:1 eşleme,
  konum hizası korunur).

---

## Yanlış alarm / doğrulanamayanlar

### P1-5. "CLI çıktı yönlendirme çalışmıyor, çıkış kodu güvenilmez" — YANLIŞ ALARM

v5.0 ikilisi (`target/debug/pdf-swiss-army-knife.exe`, 14 Eki 2026) ile:

| Komut | Beklenen | Gözlenen |
|-------|----------|----------|
| `--cli merge a.pdf b.pdf -o merged.pdf` | 0 | **exit=0**, `merged.pdf` oluştu |
| `--cli compress a.pdf -o c.pdf` | 0 | **exit=0**, `c.pdf` oluştu |
| `--cli merge missing.pdf …` | 1 | **exit=1**, stderr: `error: file not found: …` |
| `--cli nonsense` | 2 | **exit=2**, kullanım metni |

Tek nüans: **başarı** durumunda normal stdout bu konsol ana bilgisayarında yakalanmadı (uygulama
`CONOUT$`'a yazar); bu bir işlev hatası değil, kabuk/GUI alt sistemi ayrıntısıdır. Çıkış kodları
ve dosya üretimi doğru.

**Dikkat:** `target/debug/OmniOffice.exe` **eski (v3.8) bir artefakttır**; `--cli` ile
çağrıldığında GUI açılır ve asılı kalır. Doğru ikili `pdf-swiss-army-knife.exe`'dir (paket adı).
Bu karışıklık, "çalıştırılamıyor" sanılan birçok bulgunun kaynağı olabilir.

### Doğrulanamayanlar (bu ortamda)

Aşağıdakiler salt-okunur taramada işaretlendi; bu ortamda (LibreOffice, poppler, Android
emülatörü, gerçek UYAP/UBL örnekleri yok) **çalıştırarak doğrulanamadı**. `file:line` ile
listelenir; bir sonraki ajanın öncelikli doğrulama listesidir:

- TCKN doğrulayıcı negatif mod / taşma — `src/lib/invoice-qr.ts`, `crates/pdfcore/src/gibqr.rs`.
- WORKDAY "TR" tatilleri — `src/office/calc/functions/*`.
- Heceleme hiç devreye girmiyor (dil bilgisi set edilmiyor) — `src/office/writer/*`, importçular.
- İzlenen değişiklikte yazar (belge sahibi öne geçiyor) — `src/office/writer/revisions.ts`.
- Vault: arama seçenekleri/sayfalama; her sorgunun onaysız embedding'e gitmesi; vektör indeksinin
  hiç kurulmaması (`vault_index_build` çağrılmıyor) — `src-tauri/src/vault*.rs`, `src/screens/Vault.tsx`.
- Atıfların yanlış sayfa/belge açması — `src/lib/citation-target.ts`, `crates/aicore/src/citations.rs`.
- Onay denetimi yalnız iki komutta; görsel girdi kapısı sağlayıcı düzeyinde — `src-tauri/src/ai.rs`.
- Tarayıcı iskeleti (kamera yok) — `src/screens/Scanner.tsx`, Android.
- Android: başlatıcı kısayolları, paylaşılan düz metin, `.eml`/`.xml` ilişkilendirmesi.
- e-Fatura doğrulaması (%0 oran, iskonto, ÖTV, ZIP'te ilk fatura) — `crates/officecore/src/ubl.rs`.
- KVKK algılayıcı (noktalama bitişik sayılar, `1234567890`/`15W40` yanlış pozitifi) — ilgili algılayıcı.
- Etiketli PDF (satır başına paragraf; Türkçe `Alt`/`Title` bozuk) — `crates/officecore/src/pdfcanvas.rs`.
- Eklenti API v2 `pdf`/`batch-step` fiilen kullanılamıyor — `src/lib/plugins.ts`.
- Mail merge BOM/CP1254 ve çok-run yer tutucular — `crates/officecore/src/mailmerge.rs`.
- Eksik i18n anahtarları (`library.kind.chat`, `ai.cap*`).
- Paket bütçesi: **502,7 / 505 KB** (2,3 KB pay).

## Çalıştırarak eklenen gözlemler

- **YENİ (CI): `Dependency audit` işi kırmızı — neden `cargo audit` / `rustls 0.23.45`.**
  İş `npm audit`'i **geçti** (3 moderate, dev bağımlılığı `@vitest/mocker` ≤4.1.10,
  GHSA-82fw-gwwq-j7x9; bloklamıyor) ve **`cargo audit` adımında durdu**: `Crate: rustls`,
  `Version: 0.23.45`, **4 vulnerabilities found** (RUSTSEC-2026-0347, RUSTSEC-2026-0348 + 2),
  `Solution: Upgrade to >=0.23.46`. Bu, P0-11'i **CI kanıtıyla** doğrular. `deny.toml` yalnız
  RUSTSEC-2023-0071'i (rsa) muaf tutuyor; rustls güncellenmeden **hiçbir PR 12/12 yeşil olamaz**.
- **Paket bütçesi marjı çok dar:** toplam 502,7 KB (sınır 505). Her yeni ekran risklidir;
  v5.0.1'de bütçe ya düşürülmeli ya da ölçüm gerekçelendirilmeli.
- **`OmniOffice.exe` (debug) bayat:** depoda eski sürümden kalan bir ikili, CLI testlerini
  yanıltıyor. `.gitignore`/temizlik kontrolü önerilir.
- **`cargo audit` yerel ortamda yok:** güvenlik açığı taraması CI'ya bağlı (`cargo-deny`/`cargo-audit`).

## Kalite kapıları (başlangıç durumu, hepsi yeşil)

| Kapı | Sonuç |
|------|-------|
| `npm run lint` | 0 |
| `npx tsc --noEmit` | 0 |
| `npx vitest run` | tümü geçti |
| `npm run i18n:audit` | en=2377 tr=2377, eksik=0 |
| `npm run build && npm run bundle:check` | 502,7/505 KB |
| `cargo fmt --check` | 0 |
| `cargo clippy -D warnings` | 0 |
| `cargo test --workspace` | tümü geçti |
| `cargo check --locked` | 0 |
| `cargo audit` | **kurulu değil** |

## Ortam kısıtları (dürüst)

- **LibreOffice, poppler (`pdftotext`), sistem `qpdf`/`tesseract` yok.** Gömülü motorlar
  (`src-tauri/resources/engines/`: pdfium, qpdf, tesseract) mevcut ve Rust testleri onları
  kullanıyor; ancak DOCX/XLSX/PPTX/UDF dönüşüm doğrulaması ve `pdftotext` ile metin okuma
  yapılamadı.
- **Android emülatörü yok:** Android'e özgü bulgular (kısayol, paylaşım, SAF) doğrulanamadı.
- **`msedgedriver` yok:** `tauri-driver` ile otomatik WebDriver turu (42 rota × 2 dil) koşulamadı.
  Bunun yerine: tüm birim/entegrasyon testleri + CLI uçtan uca + GUI açılış duman testi
  (uygulama 10 sn ayakta, ~34 MB) yapıldı.
- **Ağ:** npm registry erişilebilir; GitHub API bazı isteklerde 403 döndü (arayüz/limit).
