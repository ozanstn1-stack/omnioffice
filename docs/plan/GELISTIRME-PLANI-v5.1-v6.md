# GELİŞTİRME PLANI — v5.0.1 → v6.0

- **Temel:** `DENETIM-v5.0.md` (bu klasördeki doğrulanmış rapor) ve v5.0.0 durumu.
- **İlke:** Önce **doğruluk ve sözleşme**, sonra yeni özellik. Her madde testle kanıtlanır;
  belgeye yalnız kanıtlanan yazılır.
- **Sürümleme:** Her sürüm kendi issue'ları + `release/vX.Y.Z` PR'ı + tag ile yayınlanır
  (mevcut `auto-tag` zinciri). `versionCode = major*1_000_000 + minor*1_000 + patch`.

---

## v5.0.1 — "Kanama durdurma" (P0 kapanışı)

Amaç: veri bozan, çökerten ve güvenlik açan P0'ların tamamını kapatmak. Yeni özellik yok.

### 1. Panik güvenliği altyapısı
- **Neden:** Üç UTF-8 panik yeri (P0-3) ve `panic=abort` yayın profili, tek bozuk girdinin
  uygulamayı kapatmasına yol açıyor.
- **Dosyalar:** `src-tauri/src/{plugin,vault,vault_index}.rs`, `Cargo.toml` (workspace lints),
  `src-tauri/src/lib.rs` (komut sarmalayıcıları).
- **Yapılacaklar:**
  - `truncate` → `floor_char_boundary`/`char_indices` tabanlı yardımcı.
  - `chunk_snippet` → tek-karakter-1:1 katlama (`fold_char`) ile konum hizası.
  - `VectorStore::load` → vektör sayısı `keys.len()*dim` ile uyuşmuyorsa indeksi geçersiz say
    (panik yerine yeniden kur).
  - Workspace clippy lintleri: riskli modüllerde `clippy::string_slice`, `clippy::indexing_slicing`
    (`warn` → kritik dosyalarda `deny`).
  - Tauri komut sarmalayıcılarında `catch_unwind` (son çare ağ) + `crash_log`.
- **Kabul kriterleri:** Üç panik yeri regresyon testiyle kilitli; bozuk `embeddings.bin`, `İ`'li
  uzun metin ve UTF-8 sınırına denk gelen eklenti PDF metni panik üretmiyor.
- **Testler:** Raporun üç yeniden üretmesi, tersine çevrilmiş (önce başarısız → düzeltme sonrası
  geçen) birim testleri olarak `src-tauri` içine.
- **Riskler:** Lint genişletmesi mevcut kodda çok uyarı üretebilir; kapsam dosya listesiyle sınırlı
  tutulmalı.

### 2. `.eml` yığın taşması
- **Neden:** P0-2; küçük bir dosya uygulamayı çökertiyor.
- **Dosyalar:** `crates/officecore/src/eml.rs`.
- **Yapılacaklar:** `parse_eml_with_depth(bytes, depth)`; `message/rfc822` dalı derinliği taşısın;
  toplam parça ve toplam bayt sınırı; aşımda `warnings` + kısmi sonuç.
- **Kabul kriterleri:** 20.000 derinlikli bomb `Ok` döner (uyarı ile), süreç çökmez.
- **Testler:** Derinlik 1/12/13/50/20000; her biri panik/çökme olmadan.
- **Riskler:** Meşru derin iletiler kırpılabilir; eşik (ör. 32) ve uyarı metni net olmalı.

### 3. e-Devlet adres doğrulaması
- **Neden:** P0-1; oltalama vektörü.
- **Dosyalar:** `src/lib/edevlet.ts`, `src-tauri/src/links.rs`.
- **Yapılacaklar:** `new URL` ile host kontrolü (`turkiye.gov.tr` ve alt alanları), yalnız `https`;
  bağlantı açan Rust tarafında da aynı allowlist; `www.turkiye.gov.tr` kabul.
- **Kabul kriterleri:** `evil.example.com/...turkiye.gov.tr` reddedilir; `http://` reddedilir;
  `https://www.turkiye.gov.tr/belge-dogrulama` kabul.
- **Testler:** TS birim testleri + Rust `links` testi.
- **Riskler:** Alt alan adı politikası fazla katı olursa meşru `*.turkiye.gov.tr` linkleri düşer;
  allowlist açık yazılmalı.

### 4. Calc yerelleştirilmiş sayı turu
- **Neden:** P0-4; sessiz 1000× veri bozulması.
- **Dosyalar:** `src/lib/office-types.ts`, `src/office/calc/{cells,locale}.ts`.
- **Yapılacaklar:** `cellText` yerel ondalık ayıracıyla yazsın; düzenleme yolunda metin→değer
  turu değeri koruyacak biçimde (biçim ayrıştırması `numberFormat`'a göre); `%` değeri 100'e bölsün.
- **Kabul kriterleri:** `1,234` düzenle→kaydet değeri `1.234` kalır; `%18` → `0.18`; `1.234` (tr,
  binlik) → `1234` (girdi niyeti korunur).
- **Testler:** `locale.test.ts` genişletmesi + düzenleme turu entegrasyon testi.
- **Riskler:** Eski belgelerdeki `General` biçimli değerler; geriye dönük uyum testi şart.

### 5. Damga ve form font bütünlüğü
- **Neden:** P0-5, P0-6; ikinci işlem birinciyi bozuyor.
- **Dosyalar:** `crates/pdfcore/src/{textfont,stamp,forms}.rs`, `docutil.rs`.
- **Yapılacaklar:**
  - `/OmniTr` kaynağı **birleştirici** olsun: mevcut font varsa glif kümesi genişletilir, üzerine
    yazılmaz.
  - `forms::register_appearance_font` belgenin `/Helv`'ini **ezmesin**; Türkçe font `/OmniTr`
    adıyla ayrı kalsın, `/Helv` base-14 korunsun (gerekirse `/DA` yalnız yeni alanlar için).
- **Kabul kriterleri:** İki ardışık damgadan sonra ilk damganın tüm glifleri hâlâ çözülür; form
  doldurma sonrası `/DR /Font /Helv` subtype'ı değişmez.
- **Testler:** `stamp-double` ve `form-fill` yeniden üretmeleri regresyon testi olarak;
  `pdftotext` yoksa pdfium metin çıkarımıyla doğrulama.
- **Riskler:** Font birleştirme karmaşık (alt küme + ToUnicode); kapsam "glif ekle" ile sınırlı
  tutulup yeni alt küme yazımı yeniden kullanılmalı.

### 6. Karşılaştırma veri kaybı
- **Neden:** P0-7.
- **Dosyalar:** `src/office/writer/compare.ts`.
- **Yapılacaklar:** Blok hizalamayı LCS'e taşı; kaldırılan tablo/blokları silinmiş olarak göster;
  run'ları biçim sınırlarında böl (stil korunur); LCS'i bantlı yap (bellek).
- **Kabul kriterleri:** Tablo ekle/sil farkları görünür; farklı biçimli run'lar biçimini korur;
  1000×1000 kelimede bellek makul.
- **Testler:** Raporun iki yeniden üretmesi + büyük paragraf performans testi.
- **Riskler:** Tablo hücresi hizalaması zor; v5.0.1'de "tablo düzeyinde işaret" ile başlanabilir.

### 7. Otomatik kayıt bütünlüğü
- **Neden:** P0-8.
- **Dosyalar:** `src/lib/blob-store.ts`, `src/lib/office-store.ts`, `src-tauri/src/office.rs`.
- **Yapılacaklar:** `pending` yalnız başarılı IPC sonrası işaretlensin (başarısızlıkta geri al);
  oturum başında/sonunda `recovery_gc_blobs`; `recovery_load` eksik blob'da net hata (zaten var).
- **Kabul kriterleri:** Başarısız kayıttan sonraki kayıt blob'u yeniden gönderir; GC yetim blob
  bırakmaz.
- **Testler:** Başarısız-sonra-başarılı senaryosu (mock IPC); GC birim testi.
- **Riskler:** Çok büyük blob'larda yeniden gönderim maliyeti; yalnız başarısızlıkta tekrar
  gönderilecek şekilde tasarla.

### 8. İzlenen klasör güvenliği
- **Neden:** P0-9.
- **Dosyalar:** `src-tauri/src/recipes.rs`.
- **Yapılacaklar:** `folder == output_dir` (ve iç içe) reddi; nesil sayacıyla tek döngü;
  parolayı OS anahtarlığında sakla (Windows Credential Manager / Android Keystore) veya çalışma
  anında sor.
- **Kabul kriterleri:** Aynı klasör yapılandırması reddedilir; iki `start` tek döngü üretir;
  parola diskte düz metin görünmez.
- **Testler:** `scan_once` aynı-klasör testi (şu an `input_changed=true` → reddet'e döner);
  `start/stop/start` yarış testi.
- **Riskler:** Anahtarlık entegrasyonu platforma özel; v5.0.1'de "çalışma anında sor" ile
  başlanabilir.

### 9. Bağımlılık ve araç
- **Neden:** P0-11 + `cargo audit` yokluğu.
- **Dosyalar:** `Cargo.toml`, `Cargo.lock`, `.github/workflows/desktop.yml`.
- **Yapılacaklar:** `rustls` ≥0.23.46; CI'ya `cargo-deny`/`cargo-audit` adımı; `OmniOffice.exe`
  bayat artefakt temizliği/`.gitignore`.
- **Kabul kriterleri:** `cargo audit` temiz; CI kırılganlıkta uyarır.
- **Riskler:** Transitif sürüm çakışmaları; `cargo update -p rustls` ile sınırlı tut.

---

## v5.1 — "Sözleşme ve doğruluk" (tekrarı önleyen mühendislik)

Amaç: P1-1/P1-2/P1-3 gibi **alan adı uyuşmazlığı** sınıfını kökten bitirmek.

### 1. Rust → TS tip üretimi
- **Neden:** `header_footer`/`headerFooter`, `tax_id_valid`/`taxIdValid` hataları tip
  sisteminden kaçtı; `src/screens/contract.test.ts` yalnız `DocumentInspection`'ı koruyor.
- **Dosyalar:** Yeni `crates/ts-export` veya `specta`/`ts-rs` entegrasyonu; `src/lib/api.ts`,
  `src/lib/office-api.ts`, `src/lib/types.ts`.
- **Yapılacaklar:** Serileştirilen tüm DTO'lar için TS tipleri **üretilsin**; elle yazılan
  kopyalar üretilenle değiştirilsin; `contract.test.ts` genişletilip **tüm komutları** kapsasın.
- **Kabul kriterleri:** `cargo run -p ts-export` çıktısı ile `src/lib/types.ts` farkı sıfır
  (CI kapısı); bir Rust alan adı değişince TS derlemesi kırılır.
- **Testler:** Üretim-uyum testi; örnek DTO'lar için serde↔TS tur.
- **Riskler:** Tauri `State`/`AppHandle` içeren imzalar üretilemez; DTO katmanı ayrıştırılmalı.

### 2. Sözleşme testleri (sahte `invoke` olmadan)
- **Neden:** Sahte `invoke` ile yazılan testler serde'yi atlar; hata sınıfı görünmez.
- **Dosyalar:** `src-tauri/tests/contract_*.rs`, mevcut ekran testleri.
- **Yapılacaklar:** Her komut için gerçek serde ile JSON turu; örnek payload'lar ekranlardan
  **kopyalanarak** değil, üretilen tiplerden.
- **Kabul kriterleri:** Her komut için en az bir gerçek-serde sözleşme testi.
- **Riskler:** Bazı komutlar `AppHandle` ister; çekirdek mantık ayrı fonksiyonlara ayrılmalı.

### 3. "Belgede iddia = test" CI kuralı
- **Neden:** `RELEASE_READINESS.md` iddiaları test kimliğine bağlı değil; sığ özellikler
  "yapıldı" görünebiliyor.
- **Dosyalar:** `RELEASE_READINESS.md`, yeni `scripts/claim-audit.mjs`, `desktop.yml`.
- **Yapılacaklar:** Her iddia satırına `test:<id>` etiketi; script testin var olduğunu ve
  geçtiğini doğrulasın; etiketsiz iddia CI'yı kırsın.
- **Kabul kriterleri:** `npm run claims:check` yeşil; eksik testli iddia build'i durdurur.
- **Riskler:** Etiket bakımı; script test adlarını `cargo test -- --list` + Vitest listesinden
  toplamalı.

### 4. Özellik bayrakları
- **Neden:** Yarım özellikler (tarayıcı, eklenti `batch-step`) tamamlanmadan görünür.
- **Dosyalar:** `src/lib/flags.ts`, `src/App.tsx` (nav), ilgili ekranlar.
- **Yapılacaklar:** `Scanner`, eklenti `batch-step` gibi yarım özellikler bayrak arkasına;
  varsayılan kapalı, "deneysel" rozeti.
- **Kabul kriterleri:** Kapalıyken rota görünmez; açıkken mevcut davranış korunur.
- **Testler:** Bayrak kapalı/açık nav testleri.
- **Riskler:** Kullanıcı beklentisi; sürüm notlarında açıkça yazılmalı.

---

## v5.2 — "Gerçek dosya laboratuvarı"

### 1. Lisansı temiz dosya korpusu
- **Neden:** UDF/UBL/OOXML doğrulaması gerçek örnek olmadan yapılamıyor (P0-10).
- **Dosyalar:** `tests/corpus/` (Git LFS), `docs/corpus.md`.
- **Yapılacaklar:** Kamuya açık/lisansı uygun UYAP `.udf`, UBL-TR fatura, DOCX/XLSX/PPTX/ODF
  örnekleri; kaynak ve lisans kaydı.
- **Kabul kriterleri:** En az 5 biçim × 3 örnek; her örnek içe aktarılır.
- **Riskler:** Kişisel veri; örnekler anonim/üretilmiş olmalı.

### 2. Uyumluluk puanı (gece CI)
- **Neden:** "İçe aktarıldı" ≠ "doğru göründü".
- **Dosyalar:** `.github/workflows/nightly.yml`, `scripts/compat-score.mjs`, Uyumluluk Merkezi.
- **Yapılacaklar:** İçe aktar → PDF'e çevir → LibreOffice ile referans → sayfa görüntüsü
  karşılaştırması → biçim başına puan; ekranda göster.
- **Kabul kriterleri:** Puan gece raporlanır; %90 altı biçim "inceleme" listesine düşer.
- **Riskler:** LibreOffice sürüm farkı; puan eşikleri gerekçeli ayarlanmalı.

### 3. Fuzz genişletmesi
- **Neden:** PDF ayrıştırıcı fuzz'ı var; içe aktarıcılar yok.
- **Dosyalar:** `fuzz/fuzz_targets/{docx,xlsx,pptx,udf,ubl}.rs`.
- **Yapılacaklar:** Her importçıya hedef + korpus tohumu; CI fuzz işine ekle.
- **Kabul kriterleri:** Hedefler derlenir ve 60 sn koşar; bulunan her çökme regresyon testi olur.
- **Riskler:** ZIP/XML katmanları paylaşıldığı için mükerrer kapsam; ortak tohum kullan.

---

## v5.3 — "Türkçe yazım ve resmî yazı denetimi"

### 1. Çevrimdışı Türkçe yazım denetimi
- **Neden:** Türkçe kullanıcı için ayırt edici; ağ gerektirmemeli.
- **Dosyalar:** Yeni `crates/turkish` (kural + morfoloji), `src/office/writer/spell.ts`.
- **Yapılacaklar:** Lisansı uygun kök sözlük + ek çözümleyici; Writer'da altını çizme + öneri.
- **Kabul kriterleri:** Yaygın hataların ≥%90'ı yakalanır; yanlış pozitif <%2.
- **Riskler:** Sözlük lisansı; kaynak ve lisans açıkça belgelenmeli.

### 2. Resmî Yazışma Yönetmeliği denetleyicisi
- **Neden:** Kamu kurumu kullanıcısı için yüksek değer.
- **Dosyalar:** Yeni `src/lib/official-letter.ts`, Writer eklentisi.
- **Yapılacaklar:** Kenar boşlukları, sayı/konu satırı, tarih biçimi, imza bloğu denetimi + tek
  tıkla düzeltme.
- **Kabul kriterleri:** Örnek resmî yazıda tüm kurallar raporlanır ve düzeltilir.
- **Riskler:** Yönetmelik sürümü; kurallar yapılandırılabilir olmalı.

### 3. Dilekçe sihirbazı
- **Neden:** UYAP/UDF ile birleşince güçlü.
- **Dosyalar:** `src/screens/PetitionWizard.tsx`, `crates/officecore/src/udf.rs`.
- **Yapılacaklar:** Adım adım dilekçe; çıktı UDF + PDF.
- **Kabul kriterleri:** Üretilen UDF gerçek örnekle uyumlu (v5.2 korpusu).
- **Riskler:** Biçim uydurma riski; korpus olmadan başlanmamalı.

---

## v5.4 — "Belge sağlığı" ve KVKK paketi

### 1. Tek tık belge raporu
- **Neden:** Dağınık denetimleri (kişisel veri, meta, font, erişilebilirlik, imza) birleştirir.
- **Dosyalar:** `src/screens/DocumentHealth.tsx`, mevcut `inspect`/`redact`/`sign` motorları.
- **Yapılacaklar:** Risk listesi + her satırda düzeltme düğmesi.
- **Kabul kriterleri:** Rapordaki her bulgu tek tıkla düzeltilebilir veya "elle" işaretlenir.
- **Riskler:** Düzeltmelerin belgeyi bozmaması; her düzeltme geri alınabilir olmalı.

### 2. Karartma sertifikası
- **Neden:** KVKK uyumu için kanıt.
- **Dosyalar:** `crates/pdfcore/src/redact.rs`, yeni ek.
- **Yapılacaklar:** Neyin, ne zaman, hangi kurala göre karartıldığını gösteren PDF eki.
- **Kabul kriterleri:** Ek, karartılan alanların listesini ve hash'lerini içerir.
- **Riskler:** Ek, karartılan bilgiyi sızdırmamalı (yalnız kural + konum).

### 3. KVKK aydınlatma şablonları
- **Neden:** Şablonlar kullanıcıyı hızlandırır.
- **Dosyalar:** `src/office/templates.ts`.
- **Yapılacaklar:** Aydınlatma/açık rıza/veri işleme şablonları (tr).
- **Kabul kriterleri:** Şablonlar `{{}}` yer tutucularıyla gelir.
- **Riskler:** Hukuki metin; "örnek, avukat incelemesi gerekir" notu.

---

## v5.5 — "Muhasebe köprüsü"

### 1. UBL → muhasebe içe aktarma biçimleri
- **Neden:** Fatura ekranındaki gerçek iş akışı.
- **Dosyalar:** `crates/officecore/src/ubl.rs`, yeni `src/lib/accounting-export.ts`.
- **Yapılacaklar:** Yaygın CSV/XLSX şablonları; alan eşleme arayüzü.
- **Kabul kriterleri:** Örnek UBL'ler şablona hatasız aktarılır.
- **Riskler:** Program başına biçim; kapsam 2–3 programla sınırlı tutulmalı.

### 2. KDV beyan yardımcı tablosu
- **Neden:** Aylık iş yükünü azaltır.
- **Dosyalar:** Yeni Calc şablonu + hesaplayıcı.
- **Yapılacaklar:** Oran bazında toplamlar, ÖTV ayrımı.
- **Kabul kriterleri:** Örnek fatura setiyle toplamlar doğru.
- **Riskler:** %0/iskonto/ÖTV kuralları (P1 doğrulanamayan madde) önce düzeltilmeli.

### 3. Mükerrer fatura (aynı ETTN) tespiti
- **Neden:** Muhasebede sık hata.
- **Dosyalar:** `src/screens/Invoices.tsx`, arşiv deposu.
- **Yapılacaklar:** ETTN hash'i ile arşivde arama; uyarı.
- **Kabul kriterleri:** Aynı ETTN ikinci kez yüklenince uyarı.
- **Riskler:** Arşiv ölçeği; indeks gerekebilir.

---

## v6.0 — "Gerçek mobil"

### 1. Kamera tarayıcı
- **Neden:** Tarayıcı şu an iskelet; Android'de en değerli özellik.
- **Dosyalar:** `src/screens/Scanner.tsx`, `src-tauri/gen/android/...`, Kotlin köprüsü.
- **Yapılacaklar:** ML Kit Document Scanner (yoksa CameraX + mevcut perspektif düzeltme);
  çoklu sayfa; PDF'e ekle.
- **Kabul kriterleri:** Emülatörde/gerçek cihazda sayfa tarama → PDF uçtan uca.
- **Riskler:** ML Kit bağımlılığı/lisans; CameraX yedeği şart.

### 2. Kısayollar ve paylaşım
- **Neden:** P1 doğrulanamayanlar (kısayollar boş, düz metin paylaşımı kayboluyor).
- **Dosyalar:** `res/xml/shortcuts.xml`, Android intent köprüsü, `src-tauri/src/android_intent.rs`.
- **Yapılacaklar:** Kısayolları gerçek işlevlere bağla; `ACTION_SEND` düz metni karşıla; dosya
  ilişkilendirmeleri (`.eml`, `.xml`).
- **Kabul kriterleri:** Kısayol doğru ekranı açar; paylaşılan metin ilgili araca düşer.
- **Riskler:** Android sürüm farkları; minSdk politikası.

### 3. SAF uyumluluğu + emülatör duman testi
- **Neden:** Android'de dosya erişimi masaüstünden farklı.
- **Dosyalar:** Tüm ekranların dosya seçicileri.
- **Yapılacaklar:** Her ekranı SAF ile gözden geçir; emülatörde ekran ekran duman testi.
- **Kabul kriterleri:** Tüm ekranlar SAF ile dosya açıp kaydedebilir.
- **Riskler:** Emülatör yavaşlığı; CI'da `android-launch-smoke.sh` genişletilmeli.

---

## Yatay (tüm sürümler)

- **Paket bütçesi:** 505 KB sınırına 2,3 KB kaldı. v5.1'de ya ölçüm gerekçelendirilip yükseltilmeli
  ya da parçalar küçültülmeli (ör. Impress/Calc ağır kısımlarını tembel yükleme).
- **i18n:** Eksik anahtarlar (`library.kind.chat`, `ai.cap*`) tamamlanmalı; `i18n:audit` zaten
  kapı.
- **Doğruluk kuralı:** Her PR'da "bu değişiklik gerçekten çalıştırıldı mı?" kanıtı (komut + çıktı).
