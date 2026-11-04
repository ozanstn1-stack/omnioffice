# ACTIVE PLAN — v7.5.0

> Bu dosya hibrit protokolün plan adımıdır: plan **Opus 5.5**'ten alınır
> (`subagent_claude_opus`, yalnız plan), uygulama DeepSeek tarafında yapılır.

## Geçmiş
- 2026-11-02: `subagent_claude_opus` ilk çağrıda düştü (Claude Code 2.1.263,
  Opus 5.5 desteklemiyor; API 400). O kesintide planı ben yazdım ve **Rust
  kapsam ratchet'i** teslim edildi (PR #584 → `b935b54`).
- 2026-11-02 (akşam): binary 2.1.316'ya yükseltildi, SDK testi
  `INIT model: claude-opus-5-5` + `end_turn` verdi. **Opus planı çalıştı** ve
  aşağıdaki plan alındı (`subagent 5eb71edc`).

---

# Seçilen iş kalemi: JSX prop i18n taraması + ilk çeviri dilimi (E4.5)

Opus'un aday karşılaştırması (özet): Scanner E2E yüksek değer ama yalnız Windows
CI'da koşuyor ve her deneme bir tur; Android ABI matrisi ve motor önbelleği
`workflow_dispatch` oturumu istiyor; Scorecard/Renovate karar PR'ı; Rust kapsam
tabanı bir sonraki ölçümde yükseltilir. **Seçim: `label` prop taraması**, çünkü
(a) VPS'te tamamen doğrulanır, (b) kullanıcının gördüğü en büyük ölçülmemiş alan,
(c) mevcut AST/ratchet kalıbını genişletiyor, (d) arkasından ölçülebilir bir
çeviri kuyruğu açıyor. Ayrıca **`attr={"…"}` ve şablon dizesi açığı** (mevcut
tarama yalnız `attr="…"` görüyor) aynı PR'da kapatılıyor.

## Adım 0 — Ön kontrol (okuma)
- Bekleyen `6735615 docs(plan)` commit'i korunur; dal onun üstünden açılır.
- Dosya boyutu ratchet'i `scripts/` kapsıyor mu? Kapsıyorsa `i18n-audit.mjs`
  büyüyemez → yeni mantık ayrı modüle.
- `i18n-audit --check` hangi CI işinde, `ci-workflow.test.ts` pinliyor mu?
- Var olan audit testi yok; `coverage-ratchet.test.ts` kalıbı (alt süreç) örnek.
- `label` prop'u alan bileşenler ve `description`/`hint` gibi kardeş prop'lar.

## Adım 1 — Tarama modülü
- **Yeni `scripts/lib/i18n-jsx.mjs`** (≤400 satır): saf fonksiyon; kaynak + dosya
  adı girer, metin düğümü / öznitelik / prop sayıları ve örnekler çıkar.
- Sayılan biçimler: `label="…"`, `label={"…"}`, `` label={`…`} `` — aynısı
  `alt/aria-label/title/placeholder` için de.
- Sayılmayanlar: `label={t("…")}`, değişken, koşullu ifade, `PROSE` desenine
  uymayan değerler. Koşullu ifade dalları **bilinen açık**.
- `scripts/i18n-audit.mjs` JSX bölümü bu modülü çağıran birkaç satıra iner;
  `--update` "grown" iletisindeki `known[file] ?? knownAttributes[file]` hatası
  düzeltilir; özete `hardcoded-props=N` eklenir; **`--props-report`** bayrağı
  `dosya:satır<TAB>prop<TAB>metin` basar (depoya yazılmaz).

## Adım 2 — Taban şeması v3
- `scripts/i18n-baseline.json` → `{ version: 3, files, attributes, props }`.
- v2 geriye uyumlu okunur; `props` yoksa açık ileti: "props tabanı kaydedilmemiş,
  `--update --allow-raise` çalıştırın".
- Tek seferlik kayıt `--update --allow-raise`. `{"…"}` kapsamı yüzünden
  `attributes` sayıları artabilir — bilinçli yükseltme, PR'da sayılarla yazılır.

## Adım 3 — Testler
- **Yeni `src/lib/i18n-jsx-audit.test.ts`** (8 vaka): düz `label`, braced,
  şablon, `t()` çağrısı sayılmaz, "—"/"" sayılmaz, `title={"Help"}` artık
  sayılır, metin düğümü sayısı değişmez, `TypeArgs<A,B>` ve `a > b` sayılmaz.
- Ratchet testleri (geçici taban): prop büyürse `--check` 1; `--allow-raise`
  olmadan `--update` reddedilir; küçülünce düşer; v2 taban açık iletiyle reddedilir.
- `ci-workflow.test.ts`: audit adımı zaten pinliyse dokunulmaz.

## Adım 4 — İlk çeviri dilimi
- `--props-report` sırasında baştaki **tek dosya** (muhtemelen
  `components/ai-settings.tsx` veya `screens/Settings.tsx`) `t()`'ye taşınır;
  anahtarlar mevcut adlandırmaya uyar; en/tr eklenir; `--update` ile o dosyanın
  prop sayısı düşürülür (ratchet'in iki yönde çalıştığı kanıtı).
- Metne bağlı testler aynı metni en.json'dan aldığı için çoğu geçer; kırılanlar
  düzeltilir.

## Adım 5 — Doküman + sürüm notu
- i18n belgesi: prop listesi, kabul edilen biçimler, bilinen açık,
  `--props-report` ve `--update` akışı.
- `RELEASE_READINESS.md` E4.5'e marker'lı madde; CHANGELOG sürüm bölümü
  ("tarama `label` prop'larını ve `{"…"}` biçimini kapsıyor; taban v3").
- `6735615` docs commit'i bu PR'a dahil edilir.

## Adım 6 — Yerel doğrulama
- `node scripts/i18n-audit.mjs --check` → 0; `npm run test`; lint/format; dosya
  boyutu ratchet'i. Rust derlemesi yok → `cargo clean` gerekmez, yine `df -h /`.

## CI etkisi / kabul kriterleri
- Yeni iş yok; mevcut audit adımı ~1 sn uzar. **Beklenen CI turu: 1** (her şey
  yerelde birebir koşuyor).
- Kabul: üç biçim yakalanıyor, `t()` çağrıları sayılmıyor; taban v3'te `props`
  var ve ilk dilimde düşmüş; `i18n-audit.mjs` büyümemiş; testler + marker'lar
  yerinde; en/tr ve placeholder eşliği temiz; CI yeşil.

## Riskler / geri alma
- Yanlış pozitifler (`label="PDF"`, `label="A4"`): taban tutar, sıfır hedefi yok;
  gerekirse sonra küçük izin listesi.
- Metin değişmeden taşınacağı için E2E seçicileri etkilenmez.
- Geri alma: PR A tek squash commit → `git revert`; yalnız tarama gürültülü
  çıkarsa `PROPS` listesi boşaltılır, `{"…"}` düzeltmesi korunur.

## Sonraki PR'lar (B+)
- Her PR 1-2 dosya veya ~100 dize: `--props-report` sırasıyla `t()`'ye taşıma,
  en/tr, taban düşürme. `description`/`hint`/`emptyText` prop'ları `label`
  tabanı belirgin düşünce eklenir.


---

# Seçilen iş kalemi 2: `hint`/`subtitle` prop taraması + beş dizenin çevirisi (E4.5 kapanışı)

Plan `subagent_claude_opus` (`febca680`, 2026-11-02 22:01) tarafından üretildi;
uygulama bu dosyadaki adımlarla DeepSeek tarafında yapılır.

## Karar
`PROPS` listesi `label`ın yanına **`hint`** ve **`subtitle`** ekler; taban şeması
**v3'te kalır** (v4'e göç yok — `props` bölümü dosya başına toplam tutuyor, prop
adına göre ayırmıyor; ad eklemek sayma kuralını değiştirir, verinin biçimini
değiştirmez). Aynı PR'da beş literal `t()`'ye taşınır, böylece taban **hiç
değişmez** ve ratchet'in gerçekten "yeni literal = kırmızı" dediği kanıtlanır.

## Adım 0 — ön ölçüm (yapıldı)
- `hint` bir **bileşen prop'u**: `src/components/ui.tsx` içinde `Field` ve
  `SectionTitle` `hint?: ReactNode` alıyor ve ikisi de görünür metin çiziyor.
- `subtitle` bir bileşen prop'u: `src/components/layout.tsx` içinde `Screen`
  `subtitle?: ReactNode` alıyor ve `<p class="muted">` içinde çiziyor.
- **`description`, `caption`, `emptyText` listeye ALINMADI**: repoda JSX
  prop'u değiller (`description` `src/lib/plugins.ts` ve
  `src/lib/document-health.ts` içinde veri alanı; `caption` `office-types.ts`
  içinde şekil alanı; `emptyText` hiç yok). Opus'un Adım 0 kuralı: "repoda prop
  olarak tanımlanmamış ad listeye girmez".
- Literal sayımı: `hint` 4 (hepsi `src/screens/PetitionWizard.tsx`),
  `subtitle` 1 (`src/screens/KvkkWizard.tsx:167`), diğerleri 0. Toplam **5 dize**.
- Bütçe 10 dize; aşan ad bu PR'dan çıkar. Aşılmadı.
- Kapsam dışı açıklar (yalnız sayıldı, çevrilmedi): `options` dizilerindeki nesne
  literalleri (`KvkkWizard` 177–179 gibi `label:`), `title:` alanlı toast
  metinleri, koşullu `subtitle` dalları (PetitionWizard 134–138).

## Adım 1 — tarayıcı listesi
- `scripts/lib/i18n-jsx.mjs`: `PROPS = ["label", "hint", "subtitle"]`; yorum
  "kardeş prop'lar sonraki dilimlerde" ifadesi güncellenir.
- `title` `ATTRIBUTES` içinde olduğu için `PROPS`'a **eklenmez** (çift sayım).

## Adım 2 — çeviri dilimi (5 dize)
- `src/screens/PetitionWizard.tsx` 143/147/186/203 `hint="…"` → `hint={t("…")}`.
- `src/screens/KvkkWizard.tsx` 167 `subtitle="…"` → `subtitle={t("…")}`.
- `src/lib/i18n/{en,tr}.json`: `petition.hint.date`, `petition.hint.authority`,
  `petition.hint.taxId`, `petition.hint.attachments`, `kvkk.subtitle`.
- **tr değeri özgün metnin birebir aynısı** → tr ile koşan test/E2E seçicisi
  değişmez. `petition.hint.authority` bilinçli olarak en=tr (örnek Türk makamı).

## Adım 3 — testler
`src/lib/i18n-jsx-audit.test.ts`: PROPS pini yeni listeye; `it.each` ile her yeni
adın üç literal biçimi; `t()` ve koşullu dalın sayılmadığı; `Screen title/subtitle`
çift sayım yapmadığı; ratchet'te `hint` büyümesinin `--check` 1 döndürdüğü;
`--props-report` satırı; `--update --allow-raise` sonrası şemanın `version: 3`
kaldığı. `ci-workflow.test.ts` ve `coverage-ratchet.test.ts` **dokunulmaz**.

## Adım 4 — belge
`docs/i18n.md` (prop listesi + `title` çift rolü notu + kapsam dışı açıklar),
`RELEASE_READINESS.md` (marker'lı E4.5 maddesi), `CHANGELOG.md` bir satır,
bu dosyanın "NEREDE KALDIM" bölümü.

## Adım 5 — yerel doğrulama (sıra önemli)
1. `npm run format` → `npm run format:gate`
2. `npm run lint`, `npx tsc --noEmit`
3. `npm run i18n:audit -- --check`, `npm run test`
4. **En son** `npm run files:check` (kırmızıysa `files:update` + yeniden `files:check`)
5. `df -h /` (Rust derlemesi yok → `cargo clean` gerekmez)

## Kabul kriterleri
- `hardcoded-props` özette **6** (yalnız CalcRibbon formül adları).
- `npm run i18n:audit -- --update` sonrası `git diff scripts/i18n-baseline.json`
  **boş** (taban v3 aynı kalıyor) — kalemin ana iddiası.
- en/tr anahtar sayısı eşit, placeholder eşliği temiz, `--check` 0.
- CI'da tüm kontroller yeşil; beklenen tur **1**.

## Kapsam dışı
Taban v4 / prop adına göre ratchet, koşullu dallar, nesne literali `label:` ve
toast `title:` metinleri, `files`/`attributes` bölümlerinde kalanlar, CalcRibbon
formül adları, aday 2–8'in hiçbiri, workflow dosyaları.

---

# Seçilen iş kalemi 3: E4.6 — nesne literalleri ve `.ts` metinlerini ölçmek

Plan `subagent_claude_opus` (`a57b8339`, 2026-11-02 22:35) tarafından üretildi.
Ölçüm dilimi: metin düzeltmesi yok, yalnız tarama + ratchet.

## Karar
`scanObjects` ikinci bir okuyucu olarak eklendi; `.ts` ve `.tsx` dosyalarındaki
nesne literallerinde duran arayüz metni **ayrı `objects` bölümünde** sayılıyor,
şema **v4**. Düzeltme (kütüphanelerin `{ code, params }` döndürmesi) sonraki dilim
(E4.7) — bu dilim yalnız görünürlük ve büyümeyi kilitleme işi.

## Tasarım (dar ve gerekçeli)
- **Anahtarlar**: `label`, `title`, `description`, `hint`, `subtitle`,
  `placeholder`, `tooltip`. `name`/`value`/`message` dışarıda (tanımlayıcı, ürün
  adı, geliştirici hatası).
- **Türkçe işareti** (`ç ğ ı ö ş ü`) şartı: ürün adları ve tanımlayıcılar
  (`DeepSeek`, `Letter`) tabana girmez. Bilinçli bedel: ASCII Türkçe ("Belge")
  de kaçar; raporda `unmarked` listesinde görünür, tabanda değil.
- **Değer biçimleri**: string, şablon (`${…}` tek yer tutucu), koşullu ifade (her
  dal bir kez). Birleştirme, `??`, çağrı sayılmaz.
- **`// i18n-content: <gerekçe>`**: satırın ya da onu tutan bildirimin üstünde;
  gerekçesiz pragma geçersiz (hiçbir şey sessizce gizlenemez). Raporda listelenir.
- **`CONTENT_PATHS`**: belgenin kendisini yazan dosyalar
  (`official-letter.ts`, `official-letter-template.ts`, `ai-task-pack-eval.ts`)
  kod içinde gerekçesiyle; tabana yazılmaz ki gerekçe incelemeden geçsin.
- **`--objects-report`** üç bölüm basar: sayılan / dışlanan (gerekçesiyle) /
  işaretsiz (bilgi).

## İlk kayıt (elle denetlendi)
**34 dize, 7 dosya**: `lib/document-health.ts` 15, `screens/KvkkWizard.tsx` 6,
`PetitionWizard` 4, `Invoices` 3, `DocumentHealthScreen` 2, `Redact` 2, `Vault` 1.
Dışlanan: 12 satır (dil otantik adları) + 3 içerik dosyası. İkinci `--update`
diff'siz — taban idempotent.

## Kabul (yapıldı)
- `--check` 0: `hardcoded-jsx=77 hardcoded-attrs=9 hardcoded-props=6 hardcoded-objects=34`.
- `i18n-jsx-audit.test.ts` 18 → 30 vaka; `ci-workflow.test.ts` pini v4 + props ≤ 6
  (15'ten sıkılaştı) + objects ≤ 34.
- Belgeler: `docs/i18n.md` yeni "object literals" bölümü, `RELEASE_READINESS.md`
  E4.6 maddesi 5 marker'la.

## Kapsam dışı (E4.7)
`document-health.ts` ve benzeri kütüphanelerin metin yerine anahtar+parametre
döndürmesi, `options` dizilerinin `labelKey`'e geçmesi, ASCII Türkçe sözcük
listesi, `??`/birleştirme biçimleri.

---

# Seçilen iş kalemi 4: Scanner E2E akışı (motorsuz, her PR'da)

Plan `subagent_claude_opus` (`44997596`, 2026-11-03 10:05 UTC). **Önemli düzeltme:**
Scanner pdfium istemiyor — `src-tauri/src/scanner.rs` köşe bulmayı
`pdfcore::page_detect`, PDF yazımını `pdfcore::images` ile yapıyor. Yani akış
Linux tauri-driver işinde koşabilir; yavaş Windows turu gerekmez.

## Neden bu, Android ABI değil
`android.yml` PR'da koşmuyor; ABI matrisi PR'ı yeşil geçse bile bir şey kanıtlamaz
ve her deneme 30–40 dk'lık dispatch ister. Scanner ise kalıcı bir PR kapısı ekliyor;
ekran "experimental" rozetli ve hiç E2E kapsamı yok.

## Adımlar
0. Okuyarak doğrula: PDF yazıcısı object stream kullanıyor mu (`/Type /Page`
   sayımı güvenilir mi), gri JPEG `/DeviceGray` mi, `data-testid` değerleri i18n
   `attributes` sayımına giriyor mu, dosya boyutu bütçeleri.
1. `src/screens/scanner-dev.ts`: `devScanOutputPath(firstPath)` → `<kök>_scan.pdf`.
2. `Scanner.tsx`: `initialFiles` prop'u (tek seferlik `addPaths`, `isImage` süzgeci),
   `useScreenReady("scanner", pages.length > 0 && !detecting, String(pages.length))`,
   `build()` içinde `useDev.getState().autoRun` iken diyalog yerine dev çıktı yolu,
   `lastResult` + `data-testid="scanner-result"` (`data-pages`, `data-corrected`),
   butonlara `scanner-pick`/`scanner-build`/`scanner-send-ocr`.
3. `scanner-page-list.tsx`: satırlara `data-testid="scanner-page"` + `data-auto`,
   `data-cropped`, `data-half`; köke `data-count`; filtrelere `aria-pressed`.
4. `App.tsx`: Scanner'a `initialFiles={files}`.
5. `e2e/lib/scan-fixtures.mjs`: bağımlılıksız PNG yazıcı (`zlib.deflateSync` + CRC32);
   `scan-photo.png` (Rust testi `scan_detects_the_page_corners` geometrisi) ve
   `scan-blank.png` (düz gri). İkili dosya repoya girmez.
6. `e2e/flows/scanner.mjs`: dev launch → 2 sayfa → dedektör `[true,false]` →
   gri filtre → build → PDF diske → `scanner-result` `2 sayfa / 1 düzeltme` →
   Node'da PDF doğrulama → OCR butonu görünür.
7. `smoke.mjs --scanner`, `package.json` `e2e:scanner`, Linux `e2e` işine adım.

## Kabul
- jsdom testleri önce: `initialFiles` süzgeci, `screenReady`, `data-auto`,
  filtre `aria-pressed`, dev autoRun'da `file_token_save` çağrılmaması.
- Yerel: lint, tsc, tam test, i18n:check, files:check (format sonrası), claims, docs:stats.
- CI: 19/19, beklenen **1 tur**.

## Kapsam dışı
Kamera girdisi, köşe sürükleme, OCR'a gönderme (tesseract), çift sayfa yayılımı,
akışın Windows'ta koşması, RELEASE_READINESS iddiası.

---

# Seçilen iş kalemi 5: Android ABI matrisi doğrulaması (E5)

Plan `subagent_claude_opus` (`2e96b92a`, 2026-11-03 11:00 UTC). Üç bulgusu kalemi
yeniden çerçeveledi: (1) **emülatör zaten var** — `android.yml` KVM'yi açıp API 34
x86_64 imajında enstrümanlı testleri ve launch smoke'u koşuyor; eksik olan PR'da
tetikleme, (2) macOS seçenek değil (iç içe sanallaştırma yok), (3)
**`SHA256SUMS-android.txt` artefakta hiç yüklenmiyordu** — release'e ekleniyor ama
indirdiği koşuda yok.

## Bu dilimde yapılan
- `.github/scripts/verify-android-abis.sh`: her APK/AAB yalnız kendi ABI'sinin
  `lib/` dizinini taşıyor mu, üç yerel kütüphane (`libpdf_sak_lib.so`,
  `libpdfium.so`, `libtesseract.so`) var mı, ELF şekli doğru mu (64-bit AArch64 /
  32-bit ARM), iki APK aynı sertifikayla mı imzalı, `sha256sum -c` geçiyor mu.
- Workflow: doğrulama **yüklemeden önce**; `android-apks` artefaktı artık
  `SHA256SUMS-android.txt` dosyasını da içeriyor.
- Pin: `ci-workflow.test.ts` hem adımı hem artefakt listesini hem de sırayı tutuyor.

## Açık kalan (bilinçli)
- **Paralel iş bölünmesi** (`build` matrisi + `assemble`): yayın hattını
  yeniden düzenler ve yalnız dispatch ile doğrulanabilir; ayrı bir dilim.
- Etiketle tetiklenen emülatör koşusu (`pull_request: types: [labeled]`),
  ARM cihaz testi (Firebase Test Lab → kullanıcı kararı).

## Kabul
- `workflow_dispatch` koşusu yeşil ve kanıt run id'siyle raporlanır; artefakt
  içinde 4 paket + toplam listesi; `lib/` denetimi geçer.

---

## NEREDE KALDIM / SIRADAKI ADIM
- **Tamamlandı (2026-11-02):** E4.5 prop taraması, iki dilim.
  PR #585 (`f7e90d1`): tarayıcı modülü + taban v3 + `PetitionWizard` etiketleri.
  PR #586 (`67948c1`): kalan dokuz etiket. **Prop tabanı 25 → 15 → 6**; kalan
  altısı Calc formül adları (bilinçli literal).
- **Repo:** master `67948c1`, çalışma ağacı temiz, `target/` yok, disk %59 (40 GB boş).
- **Kapandı (2026-11-02 22:29 UTC):** `hint`/`subtitle` prop taraması + beş
  dizenin çevirisi — **PR #587 → `f796b5b`**, 19/19 kontrol yeşil. Opus planı
  `febca680`. `PROPS` = `label`, `hint`, `subtitle`; **taban değişmedi**
  (version 3, 77 metin, 9 öznitelik, 6 prop) — `--update` diff'i boş.
  Ders: kapsam borcunu kapatırken tabanı oynatmayan bir dilim, `--allow-raise`
  gerekçesi yazmaktan daha temiz.
- **Kapandı (2026-11-02 23:02 UTC):** E4.6 ölçüm dilimi — **PR #588 →
  `383639f`**, 19/19 kontrol yeşil. Nesne literalleri ve `.ts` metinleri artık
  `objects` bölümünde (şema **v4**, **34 dize / 7 dosya**, 12 satır + 3 içerik
  dosyası gerekçeli dışlanmış), `--objects-report` üç liste basıyor. İkinci
  `--update` diff'siz.
- **Bu turun iki dilimi:** PR #587 (`f796b5b`, `hint`/`subtitle` prop taraması —
  taban hiç oynamadı) ve PR #588 (`383639f`, E4.6 `objects` bölümü). Öncesinde
  bekleyen `0cbf9c4` plan commit'i ve `6735615` benzeri yerel commit yok; ağaç
  temiz, master `383639f`, disk %59 (40 GB boş), `target/` yok.
- **E4.7 birinci yarı (2026-11-03 04:10 UTC):** `objects` **34 → 13**. Dokuz
  `document-health` bulgusu `titleKey`/`descriptionKey` + `params` taşıyor,
  ekran `t(key, params)` ile çiziyor (dil değişince metin de değişiyor; eskiden
  `useMemo` içinde Türkçe donuyordu). KVKK şablon seçenekleri `t()`'ye geçti.
  Altın test dokuz çiftin Türkçesini eski cümlelerle birebir karşılaştırıyor.
  Yol üstünde gerçek bir çizim hatası düzeldi: `KvkkWizard` `t("kvkk.lawBody")`
  ve `t("kvkk.legalWarningBody")` metnini **çağrı olarak** basıyordu
  (`hardcoded-jsx` 77 → 75).
- **E4.8 (2026-11-03 04:52 UTC, aynı pencerede):** kalan 13 toast dizesi de
  taşındı — `objects` **13 → 0**, pin `toBe(0)` ile kilitlendi. Toplam bu turda
  `objects` **34 → 0**, `files` **77 → 75**.
- **E4.8 kapandı (2026-11-03 05:05 UTC):** **PR #590 → `8b5a425`**, 19/19 yeşil.
  Kalan 13 toast dizesi taşındı, `ci-workflow.test.ts` pini **0**'a çekildi.
  Bu pencerenin toplamı: `objects` **34 → 0**, `files` **77 → 75**, üç PR
  (#588, #589, #590).
- **Kapandı (2026-11-03 10:50 UTC):** Scanner E2E akışı — **PR #596 → `cbdb793`**,
  19/19 yeşil **ilk turda**. CI kanıtı: `PASS: the scanner wrote 6210 bytes with 2
  pages; detector auto=true,false`. Scanner'ın pdfium istemediği doğrulandı →
  akış her PR'da Linux işinde koşuyor. `smoke.mjs` 148 → 142 (dört akış kendi
  launch context'ini taşıyor).
- **Kapandı (2026-11-03 12:58 UTC):** Android ABI matrisi doğrulaması —
  **PR #597 → `0cbcbe6`**. Dispatch `41635920287` (success):
  `ABI matrix OK: 4 artifacts, two ABIs, one signing identity`, iki APK da
  `f17e1d97…` sertifikasıyla imzalı, artefakt **5 dosya** (4 paket +
  `SHA256SUMS-android.txt`). Baseline artefaktında toplam listesi **yoktu** —
  gerçek boşluk buydu.
- **Kapandı (2026-11-03 16:25 UTC):** Android etiket CI — **PR #598 →
  `971d454`**, 21/21 yeşil. `android-ci` etiketi PR'da tam Android işini koşuyor
  (iki ABI + emülatör + launch smoke + ABI matrisi). R0 `41653425078` (başka
  etiket → skipped), R1 `41662802381` (PR, imzasız, tag adımları skipped),
  R2 `41653625266` (dispatch, `f17e1d97…`). Üç gerçek kusur çıktı ve düzeltildi:
  v1 imza yedeği, `--require-signature` (PR imzasız doğuyor), macOS işi bir kez
  dış nedenle iptal oldu→yeniden koştu.
- **Kapandı (2026-11-03 17:25 UTC):** Android ABI matris bölünmesi —
  **PR #599 → `909b921`**, 24/24 yeşil. Etiket koşusu `41679341450`: dört iş
  paralel (arm64 14dk35, armv7 12dk30, tests 19dk48, assemble 32sn), toplam
  **20dk44sn** vs önceki tek-iş koşusu **40dk21sn** (%49 kısalma, iki kol da
  soğuk önbellek). Artefaktlar: `android-abi-arm64-v8a`, `android-abi-armeabi-v7a`,
  `android-apks` (5 dosya); `ABI matrix OK` + 4× `: OK`. Emülatör işi `-PabiList`
  ile tek kütüphane derliyor, i686 kalktı.
- **Kapandı (2026-11-03 17:55 UTC):** **(a) tag yolu doğrulaması** — imzalı
  master dispatch'i `41683412923` **success**: iki kol da `f17e1d97…` ile
  imzaladı, `assemble` `ABI matrix OK … one signing identity` dedi. Kalan tek
  risk: tag'e bağlı üç adım (`Require the keystore`, `verify-android-release.sh`,
  release attach) yalnız `v*` etiketinde koşar.
- **Kapandı (2026-11-03 17:57 UTC):** Vault sabit Türkçe metinleri + Invoices
  `TRY` soneki — **PR #601 → `2823b63`**, 19/19 yeşil. `hardcoded-jsx`
  **75 → 56** (Invoices 19→3: yalnız Luca/Logo/Zirve; Vault listeden çıktı).
  KDV tutarları artık `money()` ile: **üç ondalık hatası da düzeldi**
  (`toLocaleString` varsayılanı 3'tü).
- **Kapandı (2026-11-03 18:22 UTC):** i18n `detail` taraması — **PR #602 →
  `df81ef6`**, 19/19 yeşil. `OBJECT_KEYS`'e `detail` eklendi; ölçüm **0 dize**
  (kalanların hepsi ya `t()` çağrısı ya hesaplanan değer ya sürüm/sayı). Boşluk
  yine de kapandı: sıradaki elle yazılmış Türkçe detay denetimde takılır.
  Belgelenen yan bulgu: `--update`, tabanda **hiç kaydı olmayan** dosyayı
  `--allow-raise` olmadan kaydediyor (büyüme koruması yalnız kayıtlı dosyalar
  için).
- **Kapandı (2026-11-03 19:02 UTC):** Vault E2E — **PR #603 → `68f1dfc`**,
  19/19 yeşil. CI kanıtı: `PASS: the vault indexed 2 documents and found the
  seeded one by keyword (50 chars previewed)`. Rust değişikliği gerekmedi:
  `vault_scan` klasörleri birleştiriyor, tohumlama `PDFSAK_DEV_RUN` + Android
  dışı koşuluna bağlı. Akış her PR'da Linux işinde (~1 dk).
- **Rust kapsam ölçümü (2. veri noktası, PR #603 koşusu):** TOTAL
  **55.23 / 56.37 / 56.30** — ilk ölçümle **birebir aynı** (108710 bölge, 5771
  fonksiyon, 53769 satır). Rust'a iki ölçüm arasında dokunulmadığı için llvm-cov
  deterministik görünüyor. 55/56/56'ya çıkarma ölçütü "düşük ölçüm yeni tabanı
  ≥0,5 puan aşmalı" idi; 55.23−55 = **0.23** → **taban yükseltilmedi** (mevcut
  ~1 puanlık pay korunuyor).
- **Kapandı (2026-11-03 19:35 UTC):** i18n son İngilizce metinler — **PR #604 →
  `4eb2acc`**, 19/19 yeşil. Altı gerçek sızıntı çevrildi: parola notu, writer
  `— page break —` (iki yol), sayfa numarası biçim örnekleri, **sertifika
  bildirimi** (cümle `${…}` içinde olduğu için tarayıcı hiç görmemişti),
  Organize `alt`/`title` şablonları ve Batch'te `… total` + dosya sayısı başlığı
  ("N sayfa" → "N dosya"). `hardcoded-jsx` **56 → 50**; kalanlar bilinçli literal.
- **Kapandı (2026-11-03 19:58 UTC):** i18n şablon boşlukları — **PR #605 →
  `82a69be`**, 19/19 yeşil. `literalText()` artık `${…}` içeren şablon
  attribute/prop'larını **düz kelimeleriyle** sayıyor (her enterpolasyon tek
  `${…}`); hemen iki gerçek sızıntı çıktı ve çevrildi: `Source page ${n}` ve
  `${rows} by ${cols}` (TR: `{rows} × {cols}`). Taban `attributes` 9 → **11**
  (kalan ikisi kısayol ipucu: `${…} (Ctrl+K)`). Ölçüldü: nesne tarafındaki
  `${…}` içi dizeler **0** — o boşluk boş çıktı, kapatılmadı.
- **SIRADAKİ ADIM:** motor önbelleğini release/beta/nightly'a taşımak
  (tag/nightly'de sınanır), Scorecard/Renovate kararı, ya da kalan
  `${…}`-içi/koşullu dal boşlukları (sayım çok satıra geçer). Seçim Opus'a
  sorulacak.
- **Ertelendi:** (1) Rust kapsam tabanı (yukarıdaki ölçüt), (2) motor önbelleğini release/beta/nightly'a taşımak,
  (3) Scorecard/Renovate kararı, (4) `Redact` kalan 1 metin düğümü + `Vault`
  kalan 1 attribute. Seçim Opus'a sorulacak.
  Bekleyen risk: tag'e bağlı üç Android adımı ilk `v*` etiketinde sınanacak.
- **NEREDE KALDIM / SIRADAKİ ADIM:** master `8b5a425`, ağaç temiz, `target/` yok,
  disk %59. Sıradaki iş kalemi **Scanner E2E akışı** (pdfium → yalnız Windows
  E2E işi) ya da **Android ABI matrisi** (`workflow_dispatch`, 30-40 dk); ikisi de
  uzun CI penceresi ister → hafta sonu ya da 10:00 UTC sonrası. Küçük alternatif:
  `detail` alanlarını tarayıcıya katmak (bilinçli `--allow-raise` ölçüm PR'ı).
  Başlamadan önce `subagent_claude_opus`'tan plan alınacak.
- **Neden burada duruldu:** sessiz saat Pzt-Cum UTC 01:00-04:00; E4.7 mimari bir
  refactor ve Opus planı gerektiriyor, sıfırdan başlatmak yerine temiz bir
  noktada bırakıldı. Başlamak için tek komut yeterli.
- **Kapalı ölçüm boşluğu (∂):** (a) nesne literallerindeki
  `label:` (test dışı 39; Türkçe işaretli 6; çoğu ürün/model adı), (b) `.ts`
  dosyalarında üretilen metinler — test dışı 44 dize `title:`, 25'i Türkçe
  (`src/lib/document-health.ts` 9, `src/lib/official-letter.ts` 7). Tarayıcı
  bugün yalnız `.tsx` okuyor.
- **Ders (bu turdan):** dosya boyutu bütçeleri **her zaman** son format
  geçişinden sonra kaydedilmeli; yoksa CI ratchet'i haklı olarak kırmızı olur.
- **Aday kuyruğu (değer/risk):** Scanner E2E (yalnız Windows CI), Android ABI
  matrisi (dispatch), motor önbelleği (release/beta/nightly), Scorecard/Renovate
  kararı, Rust kapsam tabanını yükseltme.

---

# Seçilen iş kalemi: **v7.5.0 sürüm treni** (Opus kararı, 2026-11-03 20:00 UTC)

Opus (`subagent 2f6659ad`) adayları karşılaştırdı ve **sürümü** seçti:
kalıcı talimat her sürümde APK + exe paylaşmayı istiyor, v7.4.0'dan bu yana 25
commit birikti, içlerinde kullanıcının gördüğü düzeltmeler var (`money()` iki
ondalık hatası, sekiz İngilizce sızıntı, Vault/Faturalar çevirisi) ve
`[7.4.0]` Notes Android ABI matrisini açıkça v7.5.0'a yazmıştı.

**Kritik bulgu (planı değiştirdi):** geri dönüşsüz adım elle tag değil,
**sürüm PR'ının merge'ü**. `auto-tag.yml` master'da Desktop yeşil bitince
`v$version` tag'ini kendi oluşturup `release.yml` + `android.yml`'ı tag üzerinde
başlatıyor. Yani merge'den ~40 dk sonra sürüm, kimse onay vermeden yayınlanır.
Elle tag atılmaz.

## Adım 0 — Envanter (yapıldı)
- `v7.4.0..origin/master`: **25 commit / 24 PR** (#575–#605; #600 yok).
- Sürüm dosyaları: `package.json`, `package-lock.json` (2 yer), `Cargo.toml`,
  `Cargo.lock` (7 üye), `src-tauri/tauri.conf.json`,
  `src-tauri/gen/android/app/tauri.properties` (versionName + versionCode).
- `fuzz/Cargo.lock` **zaten bayat** (officecore 6.5.1) ve hiçbir kapı onu
  okumuyor; fuzz işleri `--locked` kullanmıyor → dokunulmaz (kasıtlı).
- Fastlane dizinleri: **en-US + tr-TR** (başka yerel yok).
- v7.4.0 varlık sayısı: **18** (10 Windows/Linux ortak + 5 Android + 3 checksum…
  tam liste: 2 exe, 1 sig, 1 zip, 2 Linux, 4 Android, 1 build-info, 1 latest.json,
  2 SBOM, 4 SHA256SUMS).
- `release.yml` / `linux-package.yml` / `package.ps1` / `make-latest-json.mjs`
  v7.4.0'dan beri **değişmedi** → `release.yml` dispatch provası gerekmiyor.
  Yalnız `android.yml` değişti (192+/62-) → **android dispatch zorunlu**.
- i18n tabanları: v7.4.0 `files`=80 → bugün **50**; `props` 25 → **6**;
  `objects` 34 → **0**; `attributes` 9 → **11**. i18n anahtarı en=tr=3245.

## Adım 1–2 — Sürüm, CHANGELOG, fastlane, README, READINESS (yapıldı)
- Sürüm **7.5.0**, Android versionCode **7005000**; sürüm tutarlılık kapısı
  (release.yml'in aynısı) ve `cargo metadata --locked` yerelde yeşil.
- `CHANGELOG.md` `[7.5.0] - Uçtan uca kanıt` bölümü yazıldı (release gövdesi
  buradan üretiliyor).
- `fastlane:sync` en-US `7005000.txt` üretti; tr-TR elle yazıldı.
- README: sürüm satırı, "What's new (6.2 to 7.5.0)", 7.5.0 paragrafı, indirme
  bloğu, `docs:stats` istatistik tablosu.
- RELEASE_READINESS: başlık 7.5.0; tag sonrası giren 18 madde yeni
  `## Implemented (this cycle, 7.5.0)` bölümüne taşındı, eski bölüm
  `(v7.4.0 cycle)` olarak etiketlendi.

## Yol üstünde bulunan gerçek hata: fastlane "current version"
`scripts/fastlane-changelogs.mjs` çevrilmiş yerelleri **en eski** CHANGELOG
bölümüyle karşılaştırıyordu (`wanted[wanted.length - 1]`), yani sonsuza kadar
6.6.0 ile. tr-TR `7005000.txt` yoktu ve `fastlane:check` yine de yeşildi —
yani mağaza listesi 7.4.0 notlarında kalacaktı. Düzeltildi: en yüksek sürüm
seçiliyor, eksik dosya adıyla hata veriyor, `FASTLANE_ROOT` test kancası var,
yazma modu eksik çeviriyi **rapor ediyor ama düşmüyor** (İngilizce gövde önce
üretilebilsin; kapı `--check`, yani CI). 4 test eklendi.

## Adım 3 — Yerel kapılar
`lint`, `tsc --noEmit`, tam vitest paketi, `i18n:audit --check`,
`claims:check`, `docs:stats:check`, `files:check`, `fastlane:check`,
`format:check`, sürüm tutarlılığı, `cargo metadata --locked`.

## Adım 4 — PR ve tag öncesi prova
- PR `chore(release): v7.5.0`; PR CI'da beklenen: 19+ kontrol.
- **Zorunlu prova:** `android.yml` dalda `workflow_dispatch` — imzalı yol,
  `--require-signature`, ABI matrisi; tag'e bağlı yükleme adımları atlanır.
- **Merge şartı:** tüm PR kontrolleri + dispatch yeşil.

## Adım 5 — Tag sonrası (otomatik)
`auto-tag.yml` → tag `v7.5.0` → `release.yml` (Windows 10 varlık + Linux 3) ve
`android.yml` (5 varlık) → **toplam 18 varlık**.

## Adım 6 — Kanıt
Üç koşunun `success` olması, 18 varlık, `SHA256SUMS` satırlarının API digest'iyle
eşleşmesi, `latest.json` sürümü 7.5.0, release gövdesinin
`changelog-section.mjs 7.5.0` çıktısıyla birebir aynı olması, Android logunda
`versionCode 7005000` + imza kimliğinin v7.4.0 ile aynı olması.

## Ertelenen (7.6.0)
Motor önbelleğini release/beta/nightly'a taşımak, OpenSSF Scorecard/Renovate
kararı, tarayıcı boşlukları (koşullu dallar, nesne içi `${…}`), fuzz genişletmesi.

- **NEREDE KALDIM / SIRADAKİ ADIM:** v7.5.0 sürüm PR'ı hazırlanıyor
  (`release/v7.5.0` dalı). Sıradaki adım: yerel kapılar → PR → `android.yml`
  dispatch provası → kontrol yeşilse squash merge (merge = otomatik tag ve
  yayın) → 18 varlığı doğrula → #550'ye kapanış yorumu.

---

# KAPANDI — v7.5.0 yayınlandı (2026-11-03 22:00 UTC)

**PR #606 → `e617e16`** (squash), 23/23 kontrol yeşil; `auto-tag.yml` tag'i
kendisi attı (`v7.5.0` → `e617e16`), Release ve Android işleri yeşil.

## Yayın kanıtı (Opus Adım 6'nın tamamı)
- Üç koşu `success`: auto-tag `41719037473`, Release `41719074310` (3/3 iş),
  Android `41719079469` (5/5 iş).
- Release "OmniOffice v7.5.0": **18 varlık**, draft/prerelease değil — v7.4.0'ın
  varlık kümesiyle birebir aynı adlar.
- Checksum: `SHA256SUMS.txt` 8/8, `-windows` 3/3, `-linux` 2/2, `-android` 4/4
  satırı GitHub asset digest'iyle eşleşti (17/17). İki Windows kurulum dosyası
  aynı özet (`170a64e8…`).
- `latest.json`: `version=7.5.0`, url `v7.5.0/OmniOffice_7.5.0_x64-setup.exe`,
  `signature` dolu.
- Release gövdesi `changelog-section.mjs 7.5.0` çıktısıyla birebir aynı (tek fark
  sondaki satır sonu).
- Android tag logu: `APK versionCode: 7005000`,
  `Signing certificate matches the previous release (v7.4.0)`,
  `ABI matrix OK: 4 artifacts, two ABIs, one signing identity`; iki ABI'de de
  sertifika `f17e1d97…`.
- Tag öncesi prova: dalda `android.yml` dispatch'i (`41711776118`) imzalı yolu
  `--require-signature` ile sınadı ve 17 dakikada yeşil bitti.

## Yol üstünde bulunan ve düzeltilen hata
`fastlane-changelogs.mjs` "current version" olarak CHANGELOG'un **son** bölümünü
(en eskisini) alıyordu → çevrilmiş yereller sonsuza kadar 6.6.0 ile
karşılaştırılıyordu, `tr-TR/changelogs/7005000.txt` yokken `fastlane:check`
yeşildi. Düzeltildi (en yüksek sürüm + `FASTLANE_ROOT` kancası + yazma modunda
rapor et/düşme), 4 test eklendi, `RELEASE_READINESS`'te işaretli.

## Ders
Sürüm treni tek PR'da 3 saatte bitti çünkü: envanter (Adım 0) tag öncesi
yapıldı, `release.yml` hattı v7.4.0'dan beri değişmemişti (yalnız `android.yml`
değişmişti → sadece o dispatch edildi), ve merge kapısı öncesi Android prova
koşusu yapıldı. Geri dönüşsüz adımın merge olduğunu bilmek planı basitleştirdi.

- **NEREDE KALDIM / SIRADAKİ ADIM:** v7.5.0 yayında ve doğrulandı; master
  `e617e16`, ağaç temiz, `target/` yok. Sıradaki kalem **7.6.0'ın ilk dilimi**:
  motor önbelleğini `release.yml`/`beta.yml`/nightly'e taşımak (dalda dispatch
  ile sınanır). Onun için önce `subagent_claude_opus`'tan plan alınacak; ardından
  OpenSSF Scorecard/Renovate kararı ve tarayıcı boşlukları.
  Bekleyen dış adımlar: IzzyOnDroid başvurusu (#502), fiziksel cihaz turları.

---

# Seçilen iş kalemi: motor önbelleği sürüm hattına (7.6.0, ilk dilim) — Opus planı `e9f2a56d`

Opus planı dört varsayımımı düzeltti, hepsi uygulandı:

1. **`beta.yml` dalda dispatch edilirse YAYINLAR.** "Refresh the rolling beta
   prerelease" adımı tag'e bağlı değil; daldan bir derleme beta kanalındaki
   kullanıcılara güncelleme olarak gider. **Bu dilimde beta dispatch edilmedi.**
2. **Önbellek izin gerektirmiyor** (`ACTIONS_RUNTIME_TOKEN`, GITHUB_TOKEN değil)
   → `permissions:` değişmedi.
3. **Önbellekten gelen dosyalar lock'a göre yeniden doğrulanmıyor** (betik
   "already present" deyip `Assert-Artifact`'i atlıyor) → anahtar
   `engines.lock.json` **+ `fetch-engines.ps1`** hash'i. Sürüm işinde bu, kurulum
   paketine giren dosyalar demek; bedeli tek seferlik soğuk indirme.
4. **Asıl kazanç dakika değil güvenilirlik:** mutable `main` URL'leri
   (tessdata_fast, google/fonts, tesseract `pdf.ttf`) kritik yoldan çıkıyor.

## Tasarım
- **Yazma tek yerde:** motor önbelleğini yalnız `desktop.yml` yazar. Yayın
  işleri `actions/cache/restore@0057852…` ile **yalnız okur** — yayın koşusu
  kendi kapsamına kayıt yazıp başka koşuların güvendiği kaydı tohumlayamaz.
- `release.yml` + `beta.yml`: restore-only adım, aynı yol/anahtar.
- `nightly.yml`: **bilinçli önbelleksiz** — günlük upstream kayma kanaryası;
  önbellek tam da yakalamak istediği kaymayı gizlerdi.
- `actions/cache@0057852…` (v4) korunuyor: `bench` işinin v6.1.0'ı motor kaydıyla
  uyumsuz olabilir (önbellek sürüm hash'i/sıkıştırma), o yüzden karıştırılmadı.

## Testler (4 yeni + 1 güncellenen, hepsi `ci-workflow.test.ts`)
`restores the engine cache before fetching them, everywhere but the nightly`,
`publishing workflows only read the engine cache`,
`pins one actions/cache commit for the engine cache`,
`nightly stays the uncached upstream-drift canary`; mevcut
`caches the engine downloads by lock hash` anahtarı yeni ifadeye güncellendi.

## Doğrulama yolu
Dalda `desktop.yml` dispatch'i yeni anahtarla kaydı yazar (auto-tag yalnız master
push'unu dinler), sonra `release.yml` dispatch'i restore'u ve
`(already present)` satırlarını kanıtlar. **`beta.yml` dispatch edilmez** (1. madde).

- **NEREDE KALDIM / SIRADAKİ ADIM:** 7.6.0 dilimi uygulandı, yerel kapılar
  koşuyor; sıradaki adım PR + dalda `desktop.yml` dispatch (kayıt) + `release.yml`
  dispatch (restore kanıtı), sonra kontrol yeşilse squash merge.
  Scorecard ayrı ve sonraki dilim (yeni workflow + yeni check), Renovate
  **yapılmayacak**: Dependabot zaten çalışıyor, karar notu bu dosyada.

## Kapandı (2026-11-04 00:37 UTC) — PR #607 → `a4be68f`
37 kontrol fail 0 / pending 0. Dalda iki dispatch kanıtı:
- `desktop.yml` `41730857483` → yeni anahtar yazıldı
  (`engines-Windows-96edecc6…`, 56 MB, `refs/heads/ci/engine-cache-release`).
- `release.yml` `41734071635` → `Cache restored from key: engines-Windows-96edecc6…`,
  fetch adımı `pdfium/qpdf/tesseract/liberation fonts (already present)` dedi;
  yalnız `ip7z` bootstrap indirildi.
- **Dürüst ölçüm:** fetch v7.5.0 tag'inde 15 sn, bu dalda restore 3 sn + fetch
  6 sn = 9 sn. Yani kazanç ~6 sn; **asıl değer hız değil güvenilirlik** — mutable
  `main` URL'leri (tessdata_fast, google/fonts, tesseract `pdf.ttf`) artık sürüm
  işinin kritik yolunda değil. Bu, PR'da da bu şekilde yazıldı.
- **`beta.yml` dispatch EDİLMEDİ** (Opus uyarısı: rolling beta prerelease adımı
  tag'e bağlı değil, daldan derleme kanala gider).

## Takip işleri (ölçüldü, yapılmadı)
- `7zr.exe` + `7z2301-x64.exe` bootstrap'i koşulsuz indiriliyor (fetch betiği
  ~201-214) → tesseract bloğuna taşınabilir.
- Tesseract closure'ında `Write-Warning` ile geçen hatalar önbellekte kalıcı →
  `-Strict` anahtarı bunları hataya çevirmeli.
- v4 → v6 önbellek sürüm birleştirmesi (bench işi v6.1.0'da).

- **NEREDE KALDIM / SIRADAKİ ADIM:** motor önbelleği dilimi `a4be68f` ile
  master'da. Sürüm numarası değişmediği için `auto-tag` bir sonraki master
  Desktop koşusunda "v7.5.0 already exists" deyip hiçbir şey yayınlamaz.
  Sıradaki kalem **OpenSSF Scorecard** (yeni workflow + yeni check;
  `publish_results` yalnız varsayılan dalda anlamlı olduğu için ayrı dilim olarak
  ve Opus planıyla). Ondan sonra: tarayıcı boşlukları (koşullu JSX dalları,
  nesne içi `${…}`). **Renovate yapılmayacak** — Dependabot zaten pinleri
  güncelliyor (karar).

---

# Seçilen iş kalemi: OpenSSF Scorecard (7.6.0, ikinci dilim) — Opus planı `dbca9faf`

Opus, `scorecard-action`'ın `options/options.go` dosyasını okuyup kritik kısıtı
verdi: **varsayılan dal dışındaki her olay reddediliyor** (satır 121-126,
`errOnlyDefaultBranchSupported`) — yani daldan `workflow_dispatch` **kırmızı
olur** ve `publish_results` değeri kurtarmaz. Merge öncesi tek kanıt yolu
**pull-request koşusu**; PR olayında action yayını kendisi kapatıyor.

**Opus'tan bilinçli sapma (tek nokta):** Opus `pull_request` tetikleyicisini
yalnız geçici olarak ekleyip ikinci commit'te silmeyi önerdi. Bunun yerine
tetikleyici **kalıcı ama path filtreli**:
`pull_request: paths: [".github/workflows/scorecard.yml"]`. Gerekçe: (a) path
filtresi eşleşmeyen PR'da koşu hiç oluşturmuyor → "her PR'a yeni check"
itirazı ortadan kalkıyor, (b) bu dosyaya dokunan her gelecek PR otomatik olarak
aynı kanıtı üretiyor, (c) ikinci 40 dakikalık Desktop turu gerekmiyor — pencere
06:00 UTC'de kapanıyordu.

## Kapandı (2026-11-04 04:30 UTC) — PR #609 → `e40e3ba`
Kanıt iki aşamada:
- **A (PR):** `Scorecard analysis` yeşil; log `Event name: pull_request`,
  `Publication enabled: false`, `Successfully uploaded results`.
- **B (master push):** koşu yeşil; log `Event name: push`, `Default branch:
  master`, `publish_results: true`, `Retrieving signed certificate...`,
  `Successfully verified SCT...`.
- **Dış kanıt:** `api.securityscorecards.dev/.../omnioffice` → **score 4.6**,
  `commit: e40e3ba6a01e` (merge commit'iyle birebir); code scanning'de
  `Scorecard | refs/heads/master | e40e3ba`.

## Scorecard'ın bulduğu gerçek boşluklar
1. **Token-Permissions:** beş workflow üst düzeyde yazma izni veriyordu → skor 0.
2. **Security-Policy (kısmi puan, yanlış negatif DEĞİL):** `SECURITY.md` var ama
   içinde **hiç URL/e-posta yok**; ayrıca GitHub'da **özel güvenlik açığı
   raporlama KAPALI** (`private-vulnerability-reporting: enabled=false`).
   Skorun düşük kalmasının sebebi bu; ancak raporlama kapalıyken link eklemek
   kırık link olur → bu dilimde yapılmadı, kullanıcı adımı olarak not edildi.
3. **Signed-Releases:** D10 (imzasız Windows) → bilinçli.
4. **Packaging / CII / Fuzzing:** kapsam dışı.

---

# Seçilen iş kalemi: Token-Permissions temizliği (7.6.0, üçüncü dilim) — Opus planı `16d64dc3`

Opus'un doğruladığı üç teknik gerçek:
1. **İş düzeyindeki `permissions:` üst düzeyin yerini alır** (eklenmez) — bu
   yüzden `release/version-consistency` tek gerçek değişiklik: `contents: write`
   → `read` (o iş yalnız checkout + `node -e` yapıyor).
2. **`actions: write` gerçekten gerekli:** `gh workflow run` = dispatch API =
   Actions izni; `contents: write` buna yetmez. `auto-tag` işinde kaldı.
3. **`linux-package.yml` yeniden kullanılabilir workflow** (`workflow_call`);
   tavanı `release.yml`'deki `linux` işi koyuyor → oradaki `contents: write`
   olduğu gibi korundu.

**Doğrulama yolu:** her işin "Set up job" logunda basılan `GITHUB_TOKEN
Permissions` listesi (tag yolu çalıştırmadan). Ayrıca dalda `release.yml`
dispatch'i (yayınlama adımları tag'e kilitli) nested linux tavanını canlı
sınar. **`beta.yml` ve `auto-tag.yml` dispatch EDİLMEZ:** beta yayınlar
(rolling prerelease), auto-tag ise tag yoksa dallanmamış koddan sürüm çıkarır.

**Kırmızı→yeşil kanıtı:** testler önce yazıldı; workflow'lar değişmeden
`never grants a write permission at the workflow level` **kırmızı** düştü
(`auto-tag.yml grants [contents: write, actions: write]`), değişiklikten sonra
yeşil.

## Kapandı (2026-11-04 11:05 UTC) — PR #610 → `3db1519`

Testler önce yazıldı: workflow'lar değişmeden `never grants a write permission
at the workflow level` **kırmızı** düştü, değişiklikten sonra yeşil.
22 kontrol fail 0 / pending 0.

**Canlı kanıt (dalda `release.yml` dispatch `41813409104`, success):**
```
Version consistency        Contents: read   Metadata: read
Linux packages (nested)    Contents: write  Metadata: read
Windows installer          Actions: read  Attestations: write  Contents: write  Metadata: read
```
`version-consistency` gerçekten yalnız okumaya indi; nested reusable workflow
tavanı tuttu (startup failure yok). `pr-size` PR'da koştu: `Issues: write`,
`PullRequests: write`, log satırı normal (`skipped (403` yok).
**`beta.yml` ve `auto-tag.yml` dispatch edilmedi** (ikisi de yayın/tag üretir).

**Skor: 4.6 → 5.3.** Kontrol bazında:
- **Token-Permissions 0 → 10** ("follow principle of least privilege").
- **Security-Policy 4** — dosya **bulunuyor** ("security policy file detected"),
  yani yanlış negatif değildi; puan kısmi çünkü dosyada bağlantı yok ve özel
  raporlama kapalı.
- **Signed-Releases 8** — updater imzası (`_x64-setup.exe.sig`) sayılıyor.
- **Dangerous-Workflow 0** — sebep: `auto-tag.yml`'de
  `checkout '${{ github.event.workflow_run.head_sha || github.sha }}'`
  (Scorecard `workflow_run`'dan kod checkout'unu tehlikeli sayıyor; bizim
  koşulumuz yalnız master push'u ve Desktop success ile sınırlı, ama statik
  analiz bunu görmüyor).

- **NEREDE KALDIM / SIRADAKİ ADIM:** üç 7.6.0 dilimi master'da
  (`a4be68f`, `e40e3ba`, `3db1519`). Sıradaki kalem **Dangerous-Workflow
  bulgusu**: `auto-tag.yml`'ın `workflow_run` checkout'unu ya daraltmak
  (`github.sha` ile master tipi) ya da gerekçeli "bilinen yanlış pozitif"
  olarak belgelemek — Opus'a sorulacak. Ondan sonra tarayıcı boşlukları
  (koşullu JSX dalları, nesne içi `${…}`) ve kullanıcı adımı: GitHub'da özel
  güvenlik açığı raporlamasını açmak + `SECURITY.md`'ye advisories linki
  (Security-Policy puanını yükseltir).

---

# Seçilen iş kalemi: Dangerous-Workflow bulgusu (7.6.0, dördüncü dilim) — Opus kararı `770f327a`

Opus **D** dedi ama benim önerdiğim biçimde değil (checkout'tan önce doğrulama
adımı Scorecard'ı susturmaz, çünkü statik analiz yalnız `ref:` ifadesine bakar).
Onun yerine: **`actions/checkout` tamamen kaldırılsın**, gereken dört dosya
Contents API ile `TARGET_SHA`'dan okunsun.

Gerekçe (Opus): gerçek risk yoktu — kapı yalnız master push + Desktop success
kabul ediyor, çekilen ağaçtan kod çalıştırılmıyor (`node -p` JSON okuyor) — ama
checkout'a da gerek yoktu. Kaldırma bulguyu **gerçekten** kapatıyor (yanlış
pozitif olarak belgelemek yerine), doğruluğu koruyor (dosyalar tag'lenecek
commit'ten) ve yazma yetkili tokenı çalışma ağacından çıkarıyor.

Uygulama:
- checkout adımı silindi, yerine "neden checkout yok" yorumu (geri eklenirse
  bulgu da geri gelir).
- `fetch()` yardımcısı + dört dosya `?ref=$TARGET_SHA`, raw medya tipi.
  İndirme başarısızsa `set -euo pipefail` işi durdurur ve **tag oluşmaz**.
- Kapıya `head_repository.full_name == github.repository` eklendi: güven sınırı
  kodda yazılı, yalnız olay filtresine bağlı değil.
- 3 test: checkout yok, dört dosya aynı ref'ten, kapı repository karşılaştırmalı.

**Ön doğrulama:** dört dosya da aynı curl/`Accept: raw` biçimiyle `3db1519`
üzerinden HTTP 200 döndü (5758 / 3602 / 3236 / 132 bayt) — CI'ı beklemeden
API yolunun doğru olduğu teyit edildi.

## Kapandı (2026-11-04 13:10 UTC) — PR #611 → `7d9cd8f`

19 kontrol fail 0 / pending 0. Merge sonrası iki kanıt da geldi:

1. **auto-tag checkout'suz koştu** (`41827816676`, success): logda
   `v7.5.0 already exists; nothing to do.` — yani dört dosya Contents API ile
   `TARGET_SHA`'dan indi, üç sürüm kontrolü (package.json / tauri.conf.json /
   Cargo.toml + Android metadata) geçti ve tag yolu beklendiği gibi kısa devre
   yaptı. Tag oluşturma kısmı çalışmadı (sürüm değişmedi), o yol 7.6.0'da
   sınanacak.
2. **Scorecard:** `Dangerous-Workflow 0 → 10` ("no dangerous workflow patterns
   detected"); **toplam skor 5.3 → 6.5**.

### Skor yolculuğu
| Sürüm | commit | skor |
| --- | --- | --- |
| Scorecard eklendiğinde | `e40e3ba` | 4.6 |
| Token-Permissions sonrası | `3db1519` | 5.3 |
| checkout'suz auto-tag sonrası | `7d9cd8f` | **6.5** |

Kalan iki düşük kontrol: **Security-Policy 4** (kullanıcı adımı: private
vulnerability reporting açılınca link eklenecek) ve **Signed-Releases 8**
(D10, bilinçli).

---

# Seçilen iş kalemi: i18n tarayıcısında koşullu dallar ve `${…}` içi (E4.7) — Opus planı `4d5a0069`

Opus, `docs/i18n.md`'de yazan eski boşlukları da okuyup kapsamı netleştirdi:
- Nesne taraması koşullu dalları **zaten** sayıyor (`valueTexts` → test var);
  açık olan JSX tarafı: koşullu attribute/prop değeri, JSX çocuğu olarak yazılan
  ifade, ve çocuk olarak yazılan düz dize/şablon.
- `${…}` içi **iki yerde** açık: nesne taraması ve JSX attribute'u.
- `ci-workflow` tavanları gevşek (`files ≤ 77` ama gerçek 50).

Tasarım: tek özyinelemeli `expressionTexts()` yardımcısı iki boşluğu da kapatır
(koşullu dallar, `??`/`||`/`&&` sağ tarafı, `${…}` içi). Koşul kısmı hiç
okunmaz (`mode === "edit"` sayılmaz), `+` birleştirme bilinçli olarak dışarıda.
Şema **v4'te kalır**, yeni bölüm açılmaz; `--expressions-report` bayrağı hangi
dizenin yeni kuraldan geldiğini satır satır gösterir.

**Durak A (Opus):** ölçüm yapıldıktan sonra üç yol — ≤12 dize ise ölçüm+çeviri
aynı PR'da; daha fazlaysa yalnız ölçüm (`--allow-raise`, tavanlar yeni sayıya);
bir bölümde 0 ise "kapattık, bulgu yok" yazılır. `objects` pini `0` kalır; orada
bulgu çıkarsa çevrilir.

- **NEREDE KALDIM / SIRADAKİ ADIM:** dört 7.6.0 dilimi master'da
  (`a4be68f`, `e40e3ba`, `3db1519`, `7d9cd8f`); skor 6.5. Yerelde `b9ee5bf`
  (plan) commit'i sıradaki PR'a girecek. Sıradaki kalem **i18n koşullu dal +
  `${…}` taraması** (Opus planı yukarıda, uygulama hazır). Sonra: 7.6.0 sürüm
  treni (auto-tag'in tag oluşturma yolu ilk kez orada sınanacak).

## Uygulandı (2026-11-04 12:45 UTC) — PR #612 (i18n ifadeler)

Opus planı birebir uygulandı: `expressionTexts()` + `attributeTexts()` tek
özyinelemeli kural; koşul okunmuyor, `+` birleştirme dışarıda; `--expressions-report`
bayrağı eklendi. İki eski "bilinen boşluk" testi gerçek kelimelerle güncellendi
(adları korundu — claims işaretleri).

**Ölçüm:** `files` 50 → **73**, `attributes` 11 → **20**, `props` 6 → **8**,
`objects` 0 → **0**. Şema **v4**'te kaldı. `--update` bayraksız reddetti ve
büyüme listesini bastı (ratchet çalışıyor).

**34 dizenin ~14'ü gerçek sızıntı** (çeviri diliminin listesi):
- `Merge.tsx:86`, `Ocr.tsx:118`, `Compress.tsx:210` — elle yazılmış
  `language === "tr" ? … : …` üçlüleri (sözlüğü atlıyorlar).
- `PetitionWizard.tsx:134` — iki Türkçe `subtitle` dalı; birinde "UDF, DOCX ve
  PDF" (muhtemelen "ODT" — çeviride doğrulanacak).
- `Security.tsx:77` + `converter.tsx:49` `Hide`/`Show`; `History.tsx:236`
  `Date` ×2; `PdfStudio.tsx:1413` `- no private key`; `Ai.tsx:1306`
  `Jump to page ${…}`; `TablesUi.tsx:231` `Column`.
- Kalan ~20'si meşru literal (PDF, jpg, (CA), `${…} min`, `https://`,
  `C:\Documents`, `English (1,234.56)`).

- **NEREDE KALDIM / SIRADAKİ ADIM:** PR #612 CI'da. Merge sonrası sıradaki kalem
  **çeviri dilimi**: yukarıdaki ~14 gerçek sızıntıyı sözlüğe taşımak (koşullu
  üçlüler `t()` + dil değişimi), `PetitionWizard` "UDF" yazımını doğrulamak ve
  tabanı düşürmek (`--update`, bayraksız). Ondan sonra 7.6.0 sürüm treni
  (auto-tag'in tag oluşturma yolu ilk kez orada sınanacak).

## Kapandı (2026-11-04 13:50 UTC) — PR #613 → `0fd9408`

Opus planı (`0dc6c43b`) sınırları netleştirdi, uygulandı. **16 dize, 8 dosya.**
Türkçe metinler birebir korundu (`i18n-conditional-strings.test.ts` on anahtarı
pinliyor). Tek bilinçli görünür değişiklik: dilekçe sihirbazının alt başlığı
İngilizce arayüzde **Türkçe** görünüyordu.

**Yol üstünde bulunan gerçek hata:** `writer.pageBreak` iki kez tanımlıydı
(en: satır 1242 ve 3236) ve JSON'da sonuncusu kazanıyordu → Writer şeridindeki
düğme "— sayfa sonu —" gösteriyordu. İşaret artık `writer.pageBreakMarker`.

**Ölçüm:** `files` 73 → **64**, `attributes` 20 → **15**, `props` 8 → **6**,
`objects` 0. `--update` **bayraksız** geçti (ratchet düşüşü kabul ediyor);
`ci-workflow` tavanları 64/15/6/0. `--expressions-report` 34 → **18** satır;
kalanlar meşru literal (`https://`, `jpg`/`png`, `(CA)`, `${…} min`, biçim
örnekleri). `TablesUi`'deki `"Column"` bilinçli (formül sözdizimi).

**Yan düzeltme:** `i18n-audit.mjs`'te `--expressions-report` bloğu iki kez
vardı, ilki erken çıkıyordu → nesne tarafının ifade satırları hiç yazılmıyordu.

**Dosya bütçesi:** iki `PasswordField` bileşenine `const t = useT();` eklendiği
için `converter.tsx` 295→296, `Security.tsx` 262→263; gerekçeli, aynı PR'da.

- **NEREDE KALDIM / SIRADAKİ ADIM:** beş 7.6.0 dilimi master'da
  (`a4be68f`, `e40e3ba`, `3db1519`, `7d9cd8f`, `0fd9408`) + `917acd1`.
  Skor 6.5; Security-Policy 4 yalnız kullanıcı adımıyla yükselir.
  **Sıradaki en değerli kalem: 7.6.0 sürüm treni** — auto-tag'in tag oluşturma
  yolu (checkout'suz hali) ilk kez orada uçtan uca sınanacak; ayrıca 10+ dilim
  birikti. Ondan sonra: `PasswordField` tekrarını ortak bileşene almak, JSX
  tarafına `i18n-content:` benzeri yoksayma işareti.

---

# Seçilen iş kalemi: v7.6.0 sürüm treni — Opus planı `4e33dae7`

Opus brifingdeki iki noktayı düzeltti: (1) çalışma ağacında sürüm artışı
"yarım" görünüyordu — o benim devam eden işimdi, uzakta `v7.6.0` tag'i **yok**
(404) ve master `0fd9408`; (2) `release.yml` v7.5.0'dan beri **değişti** (motor
önbelleği + izin taşıma), yani değişmeyen tek hat `android.yml`.

Aldığım önlemler:
- **auto-tag okuma provası** yerelde: dört dosya dal SHA'sından (8154a7d)
  Contents API ile indi, üç sürüm kontrolü + Android metadata geçti
  (`version=7.6.0`, `androidCode=7006000`). POST'a gidilmedi.
- **ruleset kontrolü:** 0 ruleset → tag POST'u engellenmeyecek.
- **statik izin kontrolü:** `release/windows` işi `contents: write`,
  `actions: read`, `id-token: write`, `attestations: write` taşıyor;
  `release/linux` → `linux-package.yml` tavanı.
- **Dalda `release.yml` dispatch'i** (yayın yapmaz, yükleme adımları tag'e
  kilitli) — hat v7.5.0'dan beri değiştiği için gerçekten gerekli.
- **`android-ci` etiketi** — PR derlemesi ucuz sigorta (imzalı yolu sınamaz).

**Kritik kabul:** tag oluşturma yolu (POST `git/refs` + iki dispatch) merge
sonrası **ilk kez** koşacak. Geri dönüş yolu: okuma hatasında tag oluşmaz,
düzeltip `gh workflow run auto-tag.yml --ref master`; POST başarılı + dispatch
başarısızsa auto-tag'i tekrar koşturma, eksik workflow'u elle başlat.
