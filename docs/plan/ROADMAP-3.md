# OmniOffice Yol Haritası 3: v4.7.1 → v5.0 (başka bir yapay zekâ için uygulama planı)

> Depo: https://github.com/ozanstn1-stack/omnioffice

## Bağlam
- **Güncel durum:** master `b7c4e5c`, v4.7.0 (11 Ekim 2026). Bu oturumdan sonra GitHub'da dört sürüm
  daha çıktı:
  - **v4.5.0:** Writer ve Impress.
  - **v4.6.0:** PDF araçları.
  - **v4.6.1 / v4.6.2:** imzalı uygulama içi güncelleme.
  - **v4.7.0:** Belge Asistanı 2.0.

  Yerel diskteki eski Faz 7 çalışma ağaçları artık geçersiz. Hiçbiri gönderilmeyecek, silinecek.
- **Kaynak:** bu plan v4.7.0 kodunun GitHub'dan okunan hâline dayanıyor. Üç tarama yapıldı:
  açıklar ve borçlar, yapay zekâ ve güvenlik, özgün ürün fırsatları. İki kritik bulgu kaynak
  koddan ayrıca teyit edildi.
- **Ana sonuç:** v4.7.0'da belgelerin söylediğiyle kodun yaptığı arasında ciddi fark var.
  - Fatura çıkarma alan kaybediyor.
  - "Hibrit arama" arayüzden çağrılmıyor.
  - BM25 kodda yok.
  - Yeni ekranın 41 metni Türkçe değil.
  - v4.7.0 tag'i yayından sonra taşındı.

  Ayrıca Türkçe karakterler PDF damgalarında bozuk çıkıyor. Türkiye'ye özel ihtiyaçlar ise hiç
  karşılanmıyor: UYAP UDF, e-Fatura UBL-TR, KVKK karartma ve Türkçe sayı girişi.
- **Kullanıcının istekleri:**
  - Plan ayrıntılı ve özgün olsun; uygulamayı başka bir yapay zekâ yapacak.
  - Tüm iş GitHub üzerinden yürüsün.
  - Her sürümde EXE ve APK GitHub Release'e yüklensin.
  - Plan GitHub'a epic issue, alt issue ve plan dosyası olarak konsun.
  - **e-imza kapsam dışı.**

---

## Adım 0: Bu oturumun yapacakları (onaydan sonra)
1. Eski çalışma ağaçlarını (`.claude/worktrees/agent-*`) ve yerel `worktree-agent-*` dallarını sil.
   Hepsi v4.5.0'da başka yoldan yapıldı.
2. `claude/laughing-pasteur-tbmm3z` dalını `origin/master`'dan yeniden kur.
3. Depoya iki dosya ekle:
   - `docs/plan/ROADMAP-3.md`: bu planın tam metni.
   - `docs/plan/AI-HANDOFF.md`: aşağıdaki "Çalışma kuralları".

   PR aç, CI yeşil olunca birleştir. Sürüm değişmediği için yayın tetiklenmez.
4. GitHub'da issue'ları aç:
   - Her sürüm için bir **epic issue**: başlık `[vX.Y.Z] <ad>`, etiket `release:vX.Y.Z`.
   - Her iş için bir **alt issue**, `sub_issue_write` ile epic'e bağlı. Gövde şablonu: Neden,
     Dosyalar, Yapılacaklar, Kabul kriterleri, Testler, Riskler. Bu plandaki maddeler birebir
     aktarılır.
   - Etiketler: `prio:P0` / `prio:P1` / `prio:P2` ve `area:ai|pdf|office|android|ci|i18n|tr`.
   - GitHub araçları milestone oluşturamıyor; sürüm gruplaması epic issue ve etiketle yapılır.
5. Kullanıcıya epic issue bağlantılarını ve diğer yapay zekâya verilecek kısa başlatma metnini ver.

---

## Çalışma kuralları (`docs/plan/AI-HANDOFF.md`, diğer yapay zekâ için zorunlu)

### Akış
- **Sıra:** issue'lar epic sırasına göre alınır. Her issue kendi dalında yapılır:
  `feat/<issue#>-kısa-ad` veya `fix/<issue#>-kısa-ad`.
- **PR:** her PR `master`'a açılır, gövdesinde `Closes #N` olur ve CI yeşil olmadan birleşmez.
- **Sürüm:** iş PR'larında sürüm yükseltilmez. Bir epic'in tüm alt issue'ları kapanınca ayrı bir
  `release/vX.Y.Z` PR'ı açılır. Bu PR şunları içerir:
  - **Sürüm numarası:** `package.json` (satır 4), `package-lock.json` (iki yer), `Cargo.toml`
    (`version`), `Cargo.lock` (yalnızca `aicore`, `officecore`, `pdf-swiss-army-knife`,
    `pdfcore`, `synccore`; `objc2-encode`'a dokunma), `src-tauri/tauri.conf.json`,
    `src-tauri/gen/android/app/tauri.properties`.
  - **Android sürüm kodu:** `versionName` ve `versionCode = major*1_000_000 + minor*1_000 + patch`.
  - **Belgeler:**
    - `CHANGELOG.md`.
    - README "What's new".
    - `RELEASE_READINESS.md`, **mutlaka "Not done / Known limitations" bölümüyle**.

### Yayın
- **Otomatik zincir:** sürüm yükselten PR birleşince `auto-tag.yml` tag'i atar. Ardından
  `release.yml` (Windows) ve `android.yml` başlar.
- **Birleşmeden önce:** `android.yml` dalda elle başlatılır (`workflow_dispatch`, ref = dal).
  Kotlin derlemesi ve emülatör testleri yalnızca orada koşuyor.
- **Yayın sonrası kontrol:** Release sayfasında şunlar bulunmalı:
  - `OmniOffice-Setup-X.Y.Z.exe`
  - `OmniOffice-Portable-X.Y.Z.zip`
  - `OmniOffice-Android-X.Y.Z-arm64-v8a.apk` ve `...-armeabi-v7a.apk`
  - iki AAB
  - `latest.json` ve `.sig`
  - `SHA256SUMS*`

  Biri eksikse sürüm tamamlanmış sayılmaz.
- **Tag asla taşınmaz:** v4.7.0'da tag yayından sonra taşındı. Hata çıkarsa yeni bir yama sürümü
  (X.Y.Z+1) yayınlanır.

### Kalite kapıları
- **Yerelde koşulacaklar:**
  - `npm run lint`
  - `npx tsc --noEmit`
  - `npx vitest run`
  - `npm run i18n:audit`
  - `npm run build && npm run bundle:check`
  - `cargo fmt --check`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo test --workspace`
  - `cargo check --locked`
- **Windows clippy:** CI daha yeni bir clippy kullanıyor. Platforma özel kod `cfg` ile
  korunmalı; ölü kod bırakılmaz.
- **Test önce:** her hata için önce kırmızı test, sonra düzeltme. Her biçim işi için gidiş-dönüş
  testi; LibreOffice (`soffice --headless --convert-to`) ile iki yönlü kontrol.
- **Dürüstlük:** CHANGELOG, README ve RELEASE_READINESS'ta test edilmeyen hiçbir şey
  "yapıldı/test edildi" diye yazılmaz. 4.7.0'daki yanlış iddialar bir daha tekrarlanmaz.
- **i18n:** her yeni metin `src/lib/i18n.ts` içinde **en ve tr** olarak, ilgili anahtarın yanına
  eklenir.
- **Paket bütçesi:** şu an 485 KB sınırına karşı yaklaşık 471 KB. Sınır gerekçesiz yükseltilmez;
  yeni ağır ekranlar `React.lazy` ile ayrı parçaya alınır.
- **Satır sonları:** CRLF dosyalar korunur (`src/App.tsx`, `src/styles.css`,
  `src/office/CalcEditor.tsx`, `src/office/OfficeWorkspace.tsx`). `git diff --stat` ile tüm
  dosyanın yeniden yazılmadığı kontrol edilir.
- **Kotlin:** blok yorumlar iç içe geçer. Yorum içine `/*` yazılmaz; v4.6.1'in Android yayınını
  bu bozdu.
- **Güvenlik:**
  - Belge içeriği her zaman "veri"dir, talimat değildir.
  - Dış bağlantı yalnızca `open_external_link` allow-list'i üzerinden açılır.
  - Anahtarlar `secret.rs` dışında saklanmaz.

---

## v4.7.1: Onarım ve dürüstlük (P0, küçük)
1. **Fatura çıkarma alan kaybı** (`crates/aicore/src/prompts.rs`). `InvoiceTableRow` camelCase
   bekliyor, komut ise `invoice_number`, `line_items`, `custom_fields` istiyor; `page` alanı
   zorunlu.
   - Serde `alias` ekle; böylece iki yazım biçimi de kabul edilir.
   - `page` alanına varsayılan değer ver.
   - Satırları tek tek ve hoşgörülü ayrıştır: `"1.250,00"`, `"1250 TL"` ve string sayılar kabul
     edilsin.
   - Ayrıştırma başarısız olursa ham yanıtla birlikte hata döndür. Bugün sessizce 0 satır
     dönüyor.
   - Testler: snake_case, camelCase, eksik page, string tutar ve bozuk JSON.
2. **Eksik Türkçe metinler** (`src/screens/Ai.tsx`, `src/lib/i18n.ts`). `label("ai.…", fallback)`
   ile kullanılan 41 anahtar her iki dilde de eksik.
   - Anahtarları ekle.
   - `scripts/i18n-audit.mjs` kaynakta kullanılan anahtarları da taramalı: `t("…")`,
     `label("…")` ve `tr("…")` desenleri. Tabloda olmayan anahtar CI'ı kırar.
3. **Yayın hattı kapıları**:
   - `release.yml` ve `android.yml`: sürüm tutarlılık işleri derleme ve yükleme işlerinin
     `needs:` listesine girsin.
   - `auto-tag.yml`: `tauri.properties` da sürüm kontrolüne girsin.
   - `release.yml`: derlemeden önce aynı commit için `desktop.yml`'ın başarılı olduğunu kontrol
     etsin, ya da lint, clippy, i18n ve bundle adımlarını kendisi koşsun.
   - `desktop.yml`: `src-tauri/gen/android/**` değişince Gradle `compileDebugKotlin` ve birim
     testleri koşsun.
   - Release gövdesi tüm CHANGELOG değil, yalnızca o sürümün bölümü olsun.
4. **Güncelleme güvenliği** (`src/lib/update.ts`, `src-tauri/src/update.rs`,
   `src-tauri/src/android_update.rs`):
   - İmza doğrulaması başarısız olursa tarayıcıya sessizce düşülmesin; kullanıcıya "dosya
     doğrulanamadı" denilsin.
   - URL allow-list'i ham önek karşılaştırması yerine ayrıştırılmış ve normalize edilmiş URL ile
     yapılsın: `..` ve `%2e` reddedilsin.
   - Android'de digest yoksa kurulum yapılmasın.
   - Ön sürüm etiketleri karşılaştırmada dikkate alınsın.
   - Testler eklensin.
5. **Toplu yapay zekâ**:
   - Android'de dosya seçimi SAF yolundan geçsin: `pickFileBytes` / `src/lib/mobile.ts`.
   - İptal edilen veya tümüyle başarısız olan toplu iş "tamamlandı" görünmesin
     (`src-tauri/src/ai.rs`, `job_guard`).
6. **CSV dışa aktarma** (`Ai.tsx`):
   - Formül enjeksiyonuna karşı `=`, `+`, `-`, `@`, tab ve CR ile başlayan hücrelerin başına `'`
     eklensin.
   - Sütun eşlemesi alt dize ile değil, tam alan adıyla yapılsın.
7. **Belgelerde dürüstlük**. Yanlış iddialar düzeltilsin:
   - "BM25 yok"
   - "sıralı işleniyor"
   - gerçek fatura alanları
   - gerçek test listesi
   - olay adı `ai:batch_progress`

   Ayrıca:
   - 4.6.2 ve 4.7.0 için RELEASE_READINESS bölümleri ve "Not done" listeleri.
   - README'deki eski Android anahtar notu.
   - `AUDIT_REPORT.md` için 4.3–4.7 durum notu.
8. **Dependabot temizliği**:
   - #57 (actions) birleştirilsin; Node 20 uyarılarını giderir.
   - #66 ve #61 birlikte güncellenip birleştirilsin.
   - `pkcs8` ve `hmac` büyük sürümleri `.github/dependabot.yml` ignore listesine alınsın;
     #43 ve #44 kapatılsın.
   - #8 ve #5: eklenti için ayrı bir P2 issue açılsın.

---

## v4.8.0: "Türkçe Doğru" (P0, yerel doğruluk)
1. **PDF'te Türkçe metin**. Bugün damga, Bates ve form görünümleri WinAnsi Helvetica ile
   yazıldığı için ş, ğ, ı, İ ve Ş bozuk çıkıyor.
   - Dosyalar: `crates/pdfcore/src/stamp.rs` ve `docutil.rs`.
   - Gömülü bir TTF (ikilide hazır duran PT Sans veya Liberation) Type0/CID + ToUnicode olarak
     alt kümelenip gömülsün. Mevcut `fontembed.rs` ve `ttfsubset.rs` yeniden kullanılsın.
   - Aynı yol PDF form görünümleri için de uygulansın (`forms.rs`).
   - Test: pdfium'un metin çıkarması "Şişli Güneş İĞDIR ı" metnini birebir geri vermeli
     (Windows CI).
2. **Calc Türkçe**:
   - **Harf dönüşümü:** `UPPER`, `LOWER` ve `PROPER` `tr-TR` kurallarıyla çalışsın: i↔İ, ı↔I
     (`src/office/calc/functions/text.ts`). Kitap dili tr ise bu davranış varsayılan olsun.
   - **Sayı ve tarih girişi:** `src/office/calc/cells.ts`. Belge veya ayar düzeyinde "sayı
     yerel ayarı" eklensin. `1.234,56`, `₺1.250`, `1.250 TL`, `%18` ve `11.10.2026` gerçek sayı
     veya tarih olarak okunsun. `1.234` tr ayarında 1234 olsun.
   - **Biçimleyici:** yerel ayara göre yazsın.
   - **Testler:** iki dil ayarında tablo-temelli testler.
3. **Türkçeye özel Calc fonksiyonları**:
   - **`YAZIYLA(sayı; [para_birimi])`:** "Yalnız bin iki yüz elli Türk Lirası on beş Kuruş"
     üretir; fatura ve çek için.
   - **Doğrulayıcılar:** `TCKNDOGRULA`, `VKNDOGRULA`, `IBANDOGRULA` (mod-97).
   - **Tatil listesi:** `WORKDAY` ve `NETWORKDAYS` için isteğe bağlı "TR resmî tatilleri"
     (sabit günler ile Ramazan ve Kurban bayramları).
     - Bayram tarihleri statik bir tabloda tutulur (2020–2035) ve kaynağı dosya başında yazılır.
   - **XLSX ve ODS dışa aktarım:** bu fonksiyonlar Excel'de yok. Kaydederken önbelleğe alınmış
     değer yazılsın ve uyumluluk raporunda belirtilsin.
4. **İlk açılış ve arama**:
   - İşletim sistemi dili Türkçe ise arayüz `tr` ve OCR dili `tur+eng` başlasın
     (`src/lib/types.ts`, `store.ts`).
   - Home, araç arama ve Vault'ta ı/I/İ/i, ş/s, ğ/g, ç/c, ö/o, ü/u katlaması yapılsın
     (`vault.rs` `fold`).
5. **KVKK karartma** (`crates/pdfcore/src/redact.rs`). Otomatik algılayıcıya şunlar eklensin:
   - TCKN (sağlama algoritması)
   - VKN
   - araç plakası
   - TR telefon biçimleri
   - TR IBAN doğrulaması

   Toplu karartma Batch ekranına eklensin. Yanlış pozitifleri ölçen test kümesi hazırlansın.
6. **Türkçe heceleme**: Türkçe hece kuralları belirlidir, sözlük gerekmez.
   - Kural tabanlı bir heceleme modülü yazılsın: `crates/officecore/src/hyphen_tr.rs` ve TS ikizi
     `src/office/writer/hyphen-tr.ts`.
   - İki yanlı hizalanmış Writer PDF dışa aktarımı (`layout.rs`) ve editör görünümü bunu
     kullansın.
   - Test: 500 kelimelik altın liste.
7. **Kullanıcı adı**:
   - Ayarlara "Adınız" alanı eklensin.
   - Writer yorumlarındaki sabit `"Me"` ve izlenen değişikliklerin yazarı bu adı kullansın
     (`WriterEditor.tsx`).
8. **Şablonlar** (`src/office/templates.ts`):
   - Resmî Yazışma Yönetmeliği'ne uygun resmî yazı
   - ihtarname
   - kira sözleşmesi
   - izin dilekçesi
   - istifa dilekçesi
   - bordro
   - irsaliye

   Her şablonun alanları yer tutucudur, gerçek kişi bilgisi içermez.

---

## v4.9.0: "Resmî Belgeler" (P0–P1, özgün ve Türkiye'ye özel)
1. **UYAP `.udf` desteği** (yeni modül `crates/officecore/src/udf.rs`). UDF, içinde
   `content.xml` bulunan bir ZIP dosyası: metin ve uzaklık tabanlı stiller.
   - **Okuma:** Writer'da açılsın; paragraf, kalın/italik, hizalama ve tablo desteklensin.
   - **Yazma:** UDF olarak kaydedilebilsin. DOCX ve PDF'e dönüştürülebilsin.
   - **Dosya ilişkilendirme:**
     - Windows: `tauri.conf.json` `fileAssociations`.
     - Android: `AndroidManifest.xml` filtreleri ve `IncomingFiles.kt` açılabilir uzantılar.
   - **Test verisi:**
     - Uygulamanın ürettiği UDF dosyaları ve lisansı açık örnekler kullanılsın.
     - Biçim yalnızca örneklerden çözülür; başka projelerin kodu kopyalanmaz.
     - Bilinmeyen öğeler uyarı ile korunsun.
2. **e-Fatura / e-Arşiv UBL-TR** (yeni modül `crates/officecore/src/ubl.rs`, `quick-xml`):
   - **Okuma:** XML ve ZIP içindeki faturalar okunsun:
     - satıcı ve alıcı, VKN/TCKN
     - ETTN
     - kalemler
     - KDV oranları ve matrahlar
     - tevkifat
     - ödenecek tutar
   - **Görüntüleme:**
     - Varsayılan: uygulamanın kendi deterministik şablonu.
     - Fatura içindeki XSLT, yalnızca WebView'da XSLTProcessor varsa ve kullanıcı isterse
       çalıştırılsın.
   - **PDF:** Writer modeline çevrilip mevcut PDF dışa aktarımıyla basılsın.
   - **Toplu rapor:** bir klasör veya ZIP faturadan Calc'ta "KDV özeti" (oran, satıcı ve ay
     bazında) ve XLSX üretilsin.
   - **Doğrulama:**
     - Satır toplamları
     - KDV hesabı
     - Güncel oranlar (%1, %10, %20) tablo halinde, tarihli
   - **Yapay zekâ gerekmez.**
3. **GİB karekodu ve deterministik fatura doğrulama**:
   - PDF'teki veya taranmış faturadaki karekod saf Rust ile okunsun (`rxing` crate).
   - Karekoddaki alanlar (VKN, tarih, ETTN, tutarlar) yapay zekâ çıkarımını doğrulasın;
     uyuşmazlık kırmızı gösterilsin.
   - VKN/TCKN sağlaması yapılsın.
   - Sayı ve tarih normalizasyonu v4.8.0'daki modülü kullansın.
4. **e-Devlet barkodlu belge doğrulama**:
   - PDF metninde "doğrulama/barkod" kalıbı ve kodu algılanınca Reader'da "Belgeyi doğrula"
     düğmesi çıksın.
   - Düğme resmî doğrulama sayfasını açar: `turkiye.gov.tr` `open_external_link` allow-list'ine
     eklenir.
   - Hiçbir veri uygulamadan otomatik gönderilmez.
5. **KEP `.eml` görüntüleyici** (P2): başlıklar, gövde ve ekler gösterilsin; ekler Reader'da
   açılsın. Salt okunur.

---

## v4.10.0: "Belge Asistanı 2.1: Gerçek arama, doğrulanmış atıf" (P0–P1)
1. **Kalıcı vektör ve BM25 dizini** (`src-tauri/src/vault.rs` ve yeni `vault_index/`):
   - **Vektörler:** parça başına embedding saklansın; anahtar `model + içerik özeti`. Dosyalar
     ikili (`embeddings.bin` ve indeks JSON).
   - **BM25:** ters indeks, IDF ve uzunluk normalizasyonu ile gerçek BM25.
   - **Birleştirme:** iki sıralama Reciprocal Rank Fusion ile birleşsin.
   - **Toplu embedding:** destekleyen sağlayıcılarda istekler gruplanarak gönderilsin; iptal
     edilebilsin ve ilerleme olayı göndersin.
   - **Ölçek testi:** 5.000 belgede sorgu < 300 ms.
2. **Arayüze bağlama**:
   - `vault_search_hybrid` `src/lib/api.ts` ve `Vault.tsx`'e bağlansın.
   - Sohbetin "vault" kapsamı bu aramayı kullansın.
   - Atıf hedefi `(belge, konum)` olsun ve tıklayınca doğru dosya doğru sayfada açılsın.
     Bugün mevcut belgenin 1–5. sayfası açılıyor.
3. **Doğrulanmış atıf**:
   - Model her atıfta kısa bir alıntı vermeli.
   - Alıntı ilgili sayfa metninde bulanık eşleşmeyle doğrulansın; doğrulanamazsa "doğrulanmadı"
     rozeti gösterilsin.
   - Getirilmemiş ya da var olmayan sayfa numaraları reddedilsin.
   - `[sayfa N]`, `[pages 3-4]` ve `(p. 4)` biçimleri de tanınsın.
   - DOCX, XLSX ve PPTX atıfları doğru editör konumuna gitsin.
4. **İstem enjeksiyonuna karşı sertleştirme**:
   - Belge metni açık sınırlayıcılar içinde verilsin ve "veridir" kuralı sistem istemine yazılsın.
   - Arka uçta mesaj rolleri doğrulansın; geçmişte `system` rolü kabul edilmesin.
   - Geçmiş bağlam bütçesine göre kırpılsın.
5. **Toplu iş kuyruğu**:
   - Eşzamanlılık 2–3 ile sınırlı olsun.
   - 429 hatasında `Retry-After` ile beklenip yeniden denensin.
   - Akışsız çağrılar da iptal edilebilsin.
   - Öğe başına parola girilebilsin.
   - Token kullanımı ve tahmini maliyet gösterilsin.
6. **Görsel girdi (vision)**:
   - Sağlayıcı bu yeteneği desteklemiyorsa seçenek kapalı olsun.
   - Görüntü boyutu ve bayt sınırı olsun.
   - Görüntü oluşturma hatası kullanıcıya gösterilsin.
7. **Onay ve gizlilik**:
   - Vault kapsamı, embedding ve sayfa görüntüsü için ayrı onaylar alınsın.
   - LM Studio yerel sağlayıcı sayılsın.
   - Onay arka uçta da kontrol edilsin.
   - Sohbet yükü `jobs.json`'a yazılmasın.
   - Ayarlara "Yapay zekâ verilerini temizle" (sohbetler, indeks) ve saklama süresi eklensin.
   - Testler: her madde için Rust ve vitest testleri; üç sağlayıcının görüntü biçimi için sözleşme
     testleri.

---

## v4.11.0: "Cepte Ofis" (P1, mobil)
1. **Android belge tarayıcı**:
   - **Yol:** kamera → kenar algılama → perspektif düzeltme → iyileştirme → çok sayfalı PDF ve
     Türkçe OCR.
   - **Uygulama:** Play hizmetleri varsa ML Kit Document Scanner kullanılsın. Yoksa (örneğin
     Huawei telefonlar) CameraX ile çekim ve elle 4 köşe seçimi yapılsın; perspektif düzeltmesi
     Rust tarafında yapılsın (`imageproc`).
   - **İzin:** CAMERA izni çalışma anında istensin.
   - **Ön işleme:** mevcut OCR ön işleme (deskew, binarize) yeniden kullanılsın.
2. **Paylaşım ve kısayollar**:
   - Görsel, txt ve csv paylaşım filtreleri `AndroidManifest.xml`'e eklensin. `IncomingFiles`
     bunları zaten kabul ediyor.
   - Uygulama kısayolları: Tara, Yeni belge, Son dosyalar.
3. **Paraf ve imza kütüphanesi** (masaüstü ve mobil, `Annotate`):
   - Kayıtlı imza ve paraf saklanabilsin.
   - "Her sayfaya paraf" seçeneği olsun.
   - Tarihli hazır damgalar: "ASLI GİBİDİR", "ONAYLANDI", "GÖRÜLDÜ". Bunlar `/Annots` olarak
     yazılsın.
4. **Ana ekran**:
   - Bırakılan dosya türüne göre önerilen işlemler. Bugün hep aynı 8 PDF aracı öneriliyor.
   - Araç sabitleme ve sık kullanılanlar.
   - Kenar çubuğundaki tekrarlanan grup etiketleri düzeltilsin (`App.tsx` `navGroups`).

---

## v4.12.0: "Otomasyon" (P1–P2)
1. **Toplu iş tarifleri**:
   - Adımlar zincirlenebilsin: OCR → damga → PDF/A → karartma → sıkıştırma.
   - Tarif kaydedilip yüklenebilsin.
   - Masaüstünde "izlenen klasör" olsun; `JobRegistry` üzerinden çalışsın.
2. **Komut satırı (CLI) ve "Gönder"**:
   - Örnek: `OmniOffice.exe --cli merge a.pdf b.pdf -o out.pdf` (`launch.rs`, Windows'ta konsola
     bağlanma).
   - NSIS ile Gezgin'e "Gönder → OmniOffice" eklensin.
3. **Adres mektup birleştirme**:
   - Calc veya CSV verisi Writer şablonuna (`{{alan}}`) aktarılsın.
   - Çıktı toplu PDF veya DOCX olsun.
4. **Belge karşılaştırma**:
   - DOCX ve ODT karşılaştırması, izlenen değişiklik içeren bir belge üretsin. Mevcut revizyon
     motoru `src/office/writer/revisions` kullanılsın.
   - Sürüm geçmişinde iki sürüm arası fark gösterilsin.
5. **Eklenti API v2** (P2): PDF işlemleri yeteneği, toplu iş adımı kancası, API belgeleri ve iki
   örnek eklenti.

---

## v5.0.0: "Erişilebilir ve hızlı" (P1–P2)
1. **Etiketli PDF dışa aktarımı**: Writer ve Impress PDF'lerinde `StructTreeRoot`, `/Lang tr-TR`
   ve alternatif metin olsun. Mevcut `pdfcore/inspect.rs` PDF/UA notuyla doğrulansın.
2. **Reader**:
   - Seçilebilir metin katmanı.
   - Sesli okuma (Web Speech API, tr-TR).
3. **Performans**:
   - Ofis görselleri model JSON'undan çıkarılıp özet anahtarlı bir blob deposuna taşınsın.
     Otomatik kayıtta tüm görseller IPC'den geçmesin.
   - Etkin olmayan dil dosyası tembel yüklensin; giriş parçası küçülsün.
4. **Güvenlik denetimi**:
   - 4.3–4.7 için bağımsız denetim.
   - Yarım kalan PDF saldırgan-girdi denetimi (fuzz hedefleri genişletilerek).
   - `AUDIT_REPORT.md`'deki L2, L3, L4 ve L5 kapatılsın.
   - Yapay zekâ sayfa çıkarma sınırlı iş havuzuna alınsın (H5).

---

## Kritik dosyalar
- **Yapay zekâ:** `crates/aicore/src/{prompts,lib,anthropic}.rs`, `src-tauri/src/ai.rs`,
  `src/screens/Ai.tsx`, `src/lib/ai-providers.ts`
- **Vault:** `src-tauri/src/vault.rs`, `src/screens/Vault.tsx`, `src/lib/api.ts`
- **PDF:** `crates/pdfcore/src/{stamp,docutil,forms,fontembed,ttfsubset,redact,annotate}.rs`
- **Ofis:**
  - `crates/officecore/src/{layout,docx,odf}.rs` ve yeni `udf.rs`, `ubl.rs`, `hyphen_tr.rs`
  - `src/office/calc/{cells,functions/text}.ts`, `src/office/templates.ts`
- **Güncelleme:** `src-tauri/src/{update,android_update}.rs`, `src/lib/update.ts`
- **CI:** `.github/workflows/{release,android,auto-tag,desktop}.yml`,
  `scripts/{i18n-audit,check-bundle-budget,make-latest-json}.mjs`, `.github/dependabot.yml`
- **Android:** `src-tauri/gen/android/app/src/main/AndroidManifest.xml`,
  `.../IncomingFiles.kt`, `MainActivity.kt`

## Yeniden kullanılacaklar
- **PDF yazı tipleri:** `fontembed.rs` ve `ttfsubset.rs` (Type0/CID ve ToUnicode alt kümeleme).
- **Mobil dosya işlemleri:** `pickFileBytes`, `saveFileBytes` ve `publishOutputs`
  (`src/lib/mobile.ts`).
- **Uzun işler:** `JobRegistry` (`src-tauri/src/jobs.rs`) ve `operation_with_progress`.
- **Güvenli regex:**
  - TS tarafı: `regex-probe.ts`.
  - Rust tarafı: `regex` crate (redact).
- **Uyumluluk ve link güvenliği:**
  - `compat.rs` ve `readme_contract_test.rs`.
  - `safe_link_target` ve `open_external_link`.
- **Writer PDF yolu:** Writer modelinden PDF üretme yolu, UBL ve UDF çıktıları için kullanılır.

## Doğrulama (her sürüm)
- Yukarıdaki yerel kapılar ve CI yeşil olmalı. `android.yml` sürüm dalında elle başlatılmalı.
- Yayın sonrası Release sayfasında EXE, ZIP, iki APK, iki AAB, `latest.json`, `.sig` ve
  `SHA256SUMS*` bulunmalı.
- v4.6.2 kurulu bir cihazda uygulama içi güncellemenin yeni sürümü bulduğu (Windows) ve APK'yı
  doğrulayıp kurduğu (Android) kontrol edilmeli. Kontrol listesi `RELEASE_READINESS.md`'ye yazılır.
- Türkçe kabul testleri:
  - "Şişli Güneş İĞDIR ı" damgası pdfium'da birebir okunmalı.
  - Calc'ta `1.234,56` sayı olarak girilmeli ve `YAZIYLA(1250,15)` doğru metni vermeli.
  - Örnek UDF ve UBL dosyaları açılmalı, PDF'e dönüştürülmeli; LibreOffice ve kendi okuyucu ile
    gidiş-dönüş yapılmalı.

## Kapsam dışı
- e-imza (akıllı kart / mobil imza / qualified PAdES): kullanıcı istemedi.
- Kod imzalama sertifikası, macOS/Linux paketleri, mağaza dağıtımı.
- Canlı Google Drive/OneDrive OAuth testi: kullanıcının client ID'leri gerekiyor.
