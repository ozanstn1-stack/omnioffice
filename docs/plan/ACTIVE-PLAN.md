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

## KAPANDI — v7.6.0 yayınlandı ve doğrulandı (2026-11-04 16:45 UTC)

**PR #614 → `153b768`.** Tag `v7.6.0` → `153b768` (merge commit).

### Yeni auto-tag yolu ilk kez uçtan uca koştu
Log (`Auto release tag`, `workflow_dispatch`, success):
```
created v7.6.0 at 153b768836a4650cdb357ded4d836deb41a0ad3d
dispatched release.yml and android.yml on v7.6.0
```
Yani Contents API ile dört dosya indirme + üç sürüm kontrolü + `git/refs` POST
+ iki dispatch **çalışıyor**. `::error` satırı yok, "already exists" yok.

### Volta: master Desktop iki kez `cancelled`
Merge sonrası master Desktop **kırmızı** bitti: tek sebep
`Rust (macos-latest)` işinin **runner sağlanamadığı için** iptal edilmesi
(15 dk, **hiç adım koşmadı**, `runner_name` boş). `rerun-failed-jobs` iki kez
denendi; ikisi de aynı şekilde iptal, üçüncü deneme kuyrukta kaldı.
Aynı ağaçtaki `Rust (macos-latest)` **PR koşusunda 21 adımla geçmişti**.
Bu yüzden `auto-tag`'in **belgelenmiş manuel yolu** kullanıldı
("Manual dispatch skips the gate on purpose (a human decided the commit is
good)"). Kod hatası yok; hosting kapasitesi sorunu.

### Yayın kanıtı
- `Release` `41863872326` success (3/3 iş), `Android` `41863875733` success.
- Release "OmniOffice v7.6.0": **18 varlık**, draft/prerelease değil; ad kümesi
  v7.5.0'ınkiyle **birebir aynı** (7.6.0'a yeniden adlandırılmış).
- Checksum: `SHA256SUMS.txt` 8/8, `-windows` 3/3, `-linux` 2/2, `-android` 4/4
  → **17/17** GitHub digest'iyle eşleşti.
- `latest.json`: `version=7.6.0`, url `v7.6.0/OmniOffice_7.6.0_x64-setup.exe`,
  imza dolu.
- Release gövdesi `changelog-section.mjs 7.6.0` çıktısıyla **birebir aynı** (4699 karakter).
- Android tag logu: `APK versionCode: 7006000`,
  `Signing certificate matches the previous release (v7.5.0)`,
  `ABI matrix OK: 4 artifacts, two ABIs, one signing identity`
  (iki ABI'de de `f17e1d97…`).

### Ders
Sürüm treninin tek gerçek riski (tag oluşturma yolu) sorunsuz çalıştı; asıl
aksilik **CI altyapısıydı** (macOS runner kapasitesi). Bir sonraki sürümde:
merge'den sonra master Desktop'ın macOS işini bekle, `cancelled` ise
auto-tag'i manuel dispatch etmek meşru — ama önce aynı ağacın PR'da geçtiğini
kanıtla (bu turda öyle yapıldı).

- **NEREDE KALDIM / SIRADAKİ ADIM:** v7.6.0 yayında ve doğrulandı; master
  `153b768`, tag `v7.6.0`. Yerelde bu plan commit'i sıradaki PR'a girecek.
  Sıradaki adaylar: (a) `PasswordField` tekrarını ortak bileşene almak,
  (b) JSX tarafına `i18n-content:` benzeri yoksayma işareti (TablesUi gibi
  bilinçli literaller için), (c) Scorecard'ın kalan düşük kontrolleri.
  Kullanıcı adımı: GitHub'da private vulnerability reporting'i açmak
  (Security-Policy 4 → yükselir).

---

# Seçilen iş kalemi: Dependabot birikimini ayıklamak (Opus kararı `3f5c2943`)

Opus: bu pencerenin en değerli işi bağımlılık birikimi (7 açık PR); (a)/(b)/(c)
adayları sonraki dilime. macOS iptalleri tek başına iş değil, ön koşul.

## macOS teşhisi (Faz 0, yapıldı)
`Rust (macos-latest)` işi master'da **4 denemede de** iptal edildi. Karşılaştırma:
- push (master): `runner_name` **boş**, **0 adım**, created → cancelled 15 dk.
- PR (aynı ağaç): runner `GitHub Actions 1000017468`, **21 adım**, success.
- `rust` işi matrisi push/PR için **birebir aynı** (`macos-latest`, timeout 45).

Yani sebep kod değil, **macOS runner kapasitesi** (hiç atanmadı). Opus'un karar
tablosuna göre koda dokunulmadı; `continue-on-error` **yapılmadı** (tek macOS
derleme korumasını sustururdu). Deneme 4 kuyrukta bırakıldı; master Desktop bu
yüzden kırmızı kalıyor ve bir sonraki sürüm yine elle tag'lenmek zorunda.

## Birleşenler
- **#593** Cargo rust grubu → `8886330`.
- **#591** chrome-extension npm grubu → `f79a92e`.

## #608 → #616 (actions grubu)
İlk PR (#608) scorecard.yml eklenmeden açılmıştı: `github/codeql-action` pini
`codeql.yml`'de güncellenip `scorecard.yml`'de eski kalacaktı, yani benim
"tek codeql-action commit'i" testim merge sonrası kırılacaktı.
`@dependabot rebase` istendi → Dependabot **#616** olarak yeniden açtı; yeni dal
9 dosyayı (scorecard.yml dahil) kapsıyor ve iki dosyada da aynı yeni SHA
(`9b325408…`) var. #608 kapandı.

## Kırıcı üç bump: kapatıldı + ignore + tracking issue
| PR | Ne | Issue |
| --- | --- | --- |
| #595 | `sha2 0.11` (digest 0.11 / hybrid-array; hmac/pbkdf2 ailesi 0.10'da) | **#617** |
| #594 | `getrandom 0.4` (`getrandom()` → `fill()`, `rand` 0.8 hâlâ 0.2 çekiyor) | **#618** |
| #470 | chrome-extension `vite 8` (rolldown, manifest yolları) | **#619** |

`.github/dependabot.yml`'ye gerekçeli `ignore` kuralları eklendi (mevcut
`eslint`/`typescript`/`vitest`/`der`/`hmac` kalıbıyla aynı biçim).

## Kapandı (2026-11-04 17:58 UTC) — Dependabot ayıklaması

**Birleşenler:** `8886330` (#593 Cargo rust grubu), `f79a92e` (#591 extension
grubu), `56d04ae` (#616 actions grubu), `0fb96c3` (#620 bizim PR).

**#608 → #616:** ilk PR scorecard.yml eklenmeden açılmıştı; `@dependabot rebase`
sonrası 9 dosyayı kapsadı ve `codeql.yml` + `scorecard.yml` aynı yeni SHA'da
(`9b325408…`) — pin testim merge'de kırılmadı.

**Park edilen üç major** (ignore + tracking issue): #617 `sha2 0.11`,
#618 `getrandom 0.3+`, #619 extension `vite 8`. PR'lar #595/#594/#470
gerekçeyle kapatıldı.

**Gerçek sebep bulundu:** GitHub'ın **Actions ve Pages kesintisi** (major,
16:05–17:01 UTC, Actions `partial_outage`); macOS işlerinin 15 dk sonra
`cancelled` olması ve kuyrukta kalması bundandı. Kesinti çözüldükten sonra
#616 ve #620 yeşile geldi ve birleşti.

- **AÇIK:** #615 (frontend grubu) — tek kırmızı `Rust (macos-latest)`
  iptaliydi; yeniden koşu istendi. macOS kuyruğu düzelince birleştirilecek.
  **master Desktop** de aynı sebeple kırmızı kaldı; kuyruk normale dönünce
  yeşile gelecek ve `auto-tag` "already exists" diyecek.

---

# Kapandı (2026-11-04 18:20 UTC) — PR #625: ortak `PasswordField`

Opus `342a2fb4` **(a)** dedi; (b) i18n yoksayma işareti sonraki dilime,
(c) Scorecard'ın kalanı kullanıcı/hesap adımı.

**Keşif (DUR-1 geçildi):** iki kopya arasındaki tek davranış farkı
`autoComplete` (`new-password` vs `off`) ve `autoFocus` prop'u — ikisi de prop
oldu.

**Beklenmedik engel:** `jsx-a11y/no-autofocus` kuralı özel bileşende de
`autoFocus` prop'unu işaretliyor ve JSX attribute listesinin içine disable
yorumu konamıyor → prop **`focusOnMount`** olarak adlandırıldı.

**Sonuç:** Security 263 → **241**, converter 296 → **262**; bütçeler de indi.
Yeni `password-field.tsx` 52 satır (200 satır eşiğinin altında, bütçe kaydı
gerekmiyor). i18n tabanı **değişmedi** (64/15/6/0) — çünkü iki kopyadaki
`aria-label` literalı geçen dilimde `t()`'ye dönmüştü.

7 yeni test; mevcut ekran testlerinin assertion'ları **değişmedi**.

- **NEREDE KALDIM / SIRADAKİ ADIM:** PR #625 CI'da. Sıradaki kalem: **(b) JSX
  tarafına `i18n-content:` benzeri yoksayma işareti** (TablesUi'deki `"Column"`
  gibi bilinçli literaller için; işaret başına gerekçe zorunlu, işaret sayısına
  tavan testi). Ondan sonra kullanıcı adımı: GitHub'da private vulnerability
  reporting'i açmak → `SECURITY.md` linki (Security-Policy 4 → yükselir).
  Açık: #615 (frontend grubu; macOS iptali yüzünden bekliyor).

---

# Kapandı — PR #627 `66c5f38` (kalan dört çeviri) + PR #628 açık (toast kusuru)

**#627 birleşti.** Taban 64/14/6 → **58/12/6**, literal 3, `--expressions-report`
18 → 9. Çevrilen: Impress boş görsel ipucu, otomatik kaydetme birimleri
("30 s"/"5 min" → "30 sn"/"5 dk"), imza paneli "Yes / Off" → "Evet / Kapalı",
sayfa örneği (mevcut `common.pageAlt`). İki yeni `i18n-literal` işareti
(LinkDialog örnek adres, Convert uzantıları).

**Opus'un pivot önerisi reddedildi (gerekçeli):** `pivot.ts`'deki
`` `Column ${n}` `` pivot alanı **anahtarı** olarak kullanılıyor; çevirmek
eşleştirmeyi bozardı ve zaten tabanda sayılmıyor.

## E6 taraması (ürün derinliği)
`docs/plan/PHASE-E-PROPOSAL.md`'deki E6'nın sekiz maddesi okundu:
- **Holiday table 2030+**: dosyada **bilinçli** olarak 2029'da kesiliyor
  ("2030'da iki Ramazan var, Diyanet takvimi gerekli"). Tarihleri uydurmak
  yasak → bu madde kullanıcı/veri adımı, **yapılmadı**.
- **LAN resumable downloads, writer-desk tasks, dynamic-array spill, AI
  embedding batching**: her biri ayrı, uzun dilim.
- **6.5.1 cihaz denetiminin son açık kusuru: toast alt çubuğu örtüyor** →
  seçildi: küçük, doğrulanabilir, kullanıcıya görünür.

## PR #628 — toast yerleşimi
`.toast-host`: her yerde `env(safe-area-inset-bottom)`, **720 px altında** ayrıca
alt çubuk yüksekliği (56 px) + inset + 12 px; yığın `pointer-events: none`,
yalnız kartlar `auto` (tam genişlik toast arkadaki dokunuşu yutmasın).
Üç CSS iddiası `touch-css.test.ts`'te (jsdom stil uygulamadığı için tek yol).
`RELEASE_READINESS`'teki kusur notu "Fixed" olarak güncellendi.

- **NEREDE KALDIM / SIRADAKİ ADIM:** PR #628 CI'da. `[Unreleased]` artık **beş**
  girdi taşıyor (PasswordField, literal işareti, son dört çeviri, toast) →
  **7.7.0 sürüm treni** için yeterli. Sonraki adım: #628 merge → sürüm treni
  (Opus planıyla). Kullanıcı adımı: private vulnerability reporting.

---

# Kapandı — PR #629 `44a3c56`: v7.7.0 sürüm treni (2026-11-04 21:30 UTC)

Opus `ca01e29b` "gece hazırla, yarın 10:00'da yayınla" dedi; istisna olarak
"kapı 22:30'a kadar tamamen yeşilse gece birleştirilebilir" dedi. **Kapı tamamen
yeşil olduğu için birleştirildi.**

Gate kanıtı (hepsi merge öncesi):
- Sürüm PR'ı **27 kontrol, 0 hata** — Desktop (macOS dahil) **success**.
- **`android-ci` prova: success** → versionCode **7007000** ile APK/AAB derlendi,
  ABI matrisi denetlendi.
- **`release.yml` dalda dispatch provası: success** (3/3 iş: version-consistency,
  Windows, Linux) → tag'de koşacak hattın aynısı 7.7.0 ile çalıştı.
- `7.6.0` / `7006000` için stale referans taraması **boş**.
- macOS kapasitesi geri döndü (macOS işi hızlı runner aldı).

Sürüm dosyaları 7.7.0 / versionCode 7007000; sürüm tutarlılık kapısı ve
`cargo metadata --locked` yeşil; `target/` oluşmadı.

`[Unreleased]` dört dilim taşıyordu → `[7.7.0]` bölümü `Fixed` → `Changed` →
`Developer notes` sırasıyla yazıldı (mağaza listesi toast düzeltmesiyle başlıyor).

## KAPANDI — v7.7.0 yayınlandı ve doğrulandı (2026-11-04 22:30 UTC)

**PR #629 → `44a3c56`.** Tag `v7.7.0` → `44a3c56` (merge commit).

**auto-tag bu kez kendiliğinden koştu** (manuel dispatch gerekmedi):
```
created v7.7.0 at 44a3c56dd7752a5018fff2fa0c014cde34b6eea0
dispatched release.yml and android.yml on v7.7.0
```
Master Desktop yeşil bitince tetiklendi; macOS kapasitesi geri dönmüştü.

### Yayın kanıtı
- `Release` `41919394646` success (3/3), `Android` `41919398784` success (5/5).
- **18 varlık**, draft/prerelease değil; ad kümesi v7.6.0'ınkiyle **birebir aynı**.
- Checksum: `SHA256SUMS.txt` 8/8, `-windows` 3/3, `-linux` 2/2, `-android` 4/4
  → **17/17** GitHub digest'iyle eşleşti.
- `latest.json`: `version=7.7.0`, imza dolu, url v7.7.0 varlığını gösteriyor.
- Release gövdesi `changelog-section.mjs 7.7.0` çıktısıyla **birebir aynı** (1604 karakter).
- Android: `APK versionCode: 7007000`,
  `Signing certificate matches the previous release (v7.6.0)`,
  `ABI matrix OK: 4 artifacts, two ABIs, one signing identity`.

### Bu sürümde
Kullanıcıya görünen: **toast artık alt çubuğu örtmüyor** (6.5.1'in son açık
kusuru) ve **son dört İngilizce metin Türkçeleşti**. Altyapı: ortak şifre alanı
(263→241, 296→262 satır), JSX `i18n-literal` işareti (şema v5, taban 58/12/6,
tavan 5).

- **NEREDE KALDIK / SIRADAKİ ADIM:** v7.7.0 yayında ve doğrulandı. Kullanıcı
  adımı bekliyor: GitHub'da *Settings → Code security → Private vulnerability
  reporting* açılırsa `SECURITY.md`'ye advisories linki eklenecek
  (Security-Policy 4 → yükselir). Sonraki ürün dilimi adayı: E6'nın kalan
  maddeleri (LAN resumable downloads, dynamic-array spill, AI embedding
  batching) veya holiday table 2030+ (Diyanet verisi gerekli — kullanıcı adımı).

---

# Seçilen iş kalemi: LAN'da kaldığı yerden devam eden indirme (E6.3) — Opus planı `ca5c182c`

Opus, E6'nın kalan maddelerini karşılaştırıp **resumable download**'ı seçti:
yükleme kesintiden sonra devam ediyor (`begin_put`/`put_chunk`/`finish_put`,
`.parts/`), indirme ise tek `get` isteğinde — Wi-Fi koparsa baştan başlıyor.

**Elenenler (gerekçeli):** PDF artık nesneleri (artımlı kayıt nesne silemez;
imzaları bozmadan silmek dosyayı baştan yazmayı gerektirir → ürün kararı),
N-up bağlantıları (yedek aday), embedding toplu gönderme (sağlayıcı davranışı
doğrulanmalı), Writer seçim görevleri (`WriterEditor.tsx` bütçe sınırında),
spill hesaplama (çok büyük), Chrome eklentisi + 2030 tatil tablosu (sahibin
kararı/verisi).

**Tasarım (Opus):** protokole iki yeni işlem — `begin_get` (boyut+sha+modified)
ve `get_chunk(offset,total)`; istemcide `fetch_resumable` yarım dosyayı
`root/.parts/<sha[..32]>.get` + `.get.json` notunda tutar, boyut/sha değişirse
baştan başlar, sonda tüm dosyanın sha'sını doğrular, 3 deneme yapar. Eski sunucu
"unknown operation" derse tam `fetch`'e düşer.

**Bilinçli sapma (bu tur):** Opus yükleme kodunun tamamını `lan/upload.rs`'e
taşımayı önerdi. Ben yalnız **sunucu tarafı** yükleme işleyicilerini taşıyorum
(~130 satır, saf fonksiyonlar): aynı bütçe kazancını verir, istemci `impl
LanClient` bloğunu bölme riskini almaz. Ayrıca commit mesajlarına
`Co-Authored-By: Claude` satırı eklenmiyor — repo konvansiyonu DSH rozeti.

---

# Açık — PR #630 birleşti (`53cdd82`, LAN resumable download)

19 kontrol yeşil (2 beklenen skip: Benchmarks, Fuzz), macOS dahil. Dal silindi.
`[Unreleased]` artık LAN resumable download girdisini taşıyor (7.8.0 treni).

## Seçilen iş kalemi: N-up /Link açıklamalarını koruma — **Opus planı `fc13a2c8`**

Aday gerekçesi: `docs/plan/PHASE-E-PROPOSAL.md` E6 madde 7'de açık duran "N-up drops
links" sınırı; `nup.rs` doc yorumunda da yazılı. Kullanıcıya görünür (içindekiler/URL
bağlantısı olan PDF 2-up basılınca tıklanabilirlik kayboluyor), VPS'te tamamen
doğrulanabilir (pdfcore testleri), yeni bağımlılık yok.

### Kararlar (Opus)
1. **Yalnız `/Subtype /Link` taşınır.** `/Text`, `/Highlight`, `/Widget` gibi türler
   bilinçli olarak düşer: görünüm akışları sayfa uzayında, Form XObject içine konamaz;
   taşımak her alt tip için ayrı dönüşüm ister. Doğru çözüm ileride **düzleştirme**
   (PR-2 adayı). Bugün de görünmüyorlar, PR kötüleştirmiyor.
2. **URI** → `/Rect` dönüştürülür + hücreye kırpılır, `/A /S /URI` inline kopya.
   `/Next` atılır (döngü riski yok). **GoTo / Dest / adlandırılmış hedef** → hedef
   sheet'e **açık dizi** olarak yeniden yazılır (`/Names /Dests` ağacına bağımlılık
   kalmaz). **GoToR / Launch / JavaScript / Named / bilinmeyen** → atılır.
3. `/Rect`: dört köşe yerleşim matrisiyle dönüştürülür, min/max (döndürmeli sayfada
   eksen hizalı) ve **hücre dikdörtgeniyle kesiştirilir** (Form BBox kırptığı için
   komşu hücreye taşan tıklama alanı yanlış olurdu). `/QuadPoints` varsa dönüştürülür,
   geçersizse atılır. `/P` yeni sheet, `/AP` ve `/StructParent` atılır.
4. Görünüm tipleri: `/XYZ` noktası kesin eşlenir (zoom `null`), `/FitR` dönüştürülüp
   kırpılır, diğer her şey (`/Fit`, `/FitH`, `/FitV`, `/FitB*`) **hedef hücrenin
   `/FitR`**'ine indirgenir (sheet'in tamamına `/Fit` 2-up'ta anlam değiştirir).
5. Dosya bütçesi: `nup.rs` 398'de sabit → `build_sheet_page` **`nup/sheet.rs`**'e
   taşınır ve `Placement { source, matrix, clip }` döndürür; yeni mantık
   **`nup/links.rs`** + **`nup/dests.rs`**. `lib.rs` değişmez (alt modül).
6. Hata politikası: `carry_links` **`Result` döndürmez, `?` kullanmaz** — bozuk
   `/Annots`, silinmiş nesne, kırık `/A`, döngülü name tree, seçim dışı sayfa hedefi
   → yalnız o bağlantı atılır, işlem düşmez.
7. Kopya **paylaşılmaz**: her açıklama yeni nesne, `/P` sheet'e; kaynak sözlüğe hiç
   yazılmaz (`docutil::clone_page_object` kalıbı).

### Sapmalar (benden)
- Commit mesajlarında `Co-Authored-By: Claude` yok, PR gövdesinde Claude rozeti yok →
  repo konvansiyonu DSH rozeti (`dsh-badge`).
- Opus "PR-2 flatten / PR-3 outline" önerilerini roadmap'e aday olarak yazıyorum,
  uygulamıyorum.

### Adımlar
1. `refactor(pdfcore)`: `build_sheet_page` → `nup/sheet.rs`, `Placement` döndür, `PageForm.source`
   alanı; davranış aynı, 9 mevcut N-up testi değişmeden geçer, `files:check` tabanı düşer.
2. `feat(pdfcore)`: `nup/dests.rs` (DestMap, name tree okuyucu, `map_dest`, `map_view`) +
   `nup/links.rs` (`carry_links`, `source_annots`, `map_rect`, `clip`, kopya süzgeci) +
   `nup.rs`'te tek çağrı.
3. `test(pdfcore)`: `tests/common/link_fixtures.rs`, `tests/nup_links_test.rs` (geometri/hedef),
   `tests/nup_links_edge_test.rs` (süzme/bozuk girdi/paylaşım yok) — 12 vaka.
4. `docs`: `nup.rs` doc yorumu, `README.md` N-up satırı, `docs/roadmap.md` (~281),
   `PHASE-E-PROPOSAL.md` madde 7, `CHANGELOG.md [Unreleased]`.

### Kapılar
`cargo fmt -p pdfcore -- --check` · `cargo check -p pdfcore --all-targets` ·
`cargo clippy -p pdfcore --all-targets -- -D warnings` · `cargo test -p pdfcore --lib` ·
`cargo test -p pdfcore --test nup_test` · yeni iki test dosyası · `files:check` ·
`claims:check` · `npx vitest run`. Sonunda `cargo clean` + `df -h /`.

- **DURUM:** dört adım da uygulandı, tek PR **#631** (`feat/nup-link-annots`,
  `ebffd22` kod + `c0b6aa4` test + `af7e426` belge). Yerelde: pdfcore lib 197,
  `nup_test` 8, yeni `nup_links_test` 9, `nup_links_edge_test` 4 test geçti;
  clippy `-D warnings`, fmt, `files:check`, `claims:check` temiz.
  `nup.rs` 398 → 362 (taban düştü). CI bekleniyor.

- **NEREDE KALDIM / SIRADAKİ ADIM:** #631 **birleşti** (`58a69d0`); dal silindi.
  Yeni dilim aşağıda (Opus `0c98e515`).

---

# Seçilen iş kalemi: N-up yer imlerini (outline) yeni sayfalara eşleme — **Opus planı `0c98e515`**

Gerekçe (Opus): `DestMap` #631'de yazıldı ve bağlam taze; outline aynı hedef
çözümünü kullanıyor (`/Dest`, adlandırılmış hedef, `/XYZ` nokta, `/Fit*` → hücre
`/FitR`). Kullanıcı farkı hemen görür (yer imleri çalışır), tamamı VPS'te
`cargo test -p pdfcore` ile doğrulanır; pdfium/sır gerekmez. Markup düzleştirme
(PR-2) görünüm akışı matrisleri yüzünden riskli, görsel doğrulama istiyor.

## Adımlar
1. **`dests.rs` API'si**: hedef yeniden yazımı tek kapıya iner —
   `remap_dest(&self, src: &Document, dest: &Object) -> Option<Object>` (ad →
   `/Names /Dests` + eski `/Dests`, dizi → sheet, `/XYZ` nokta, `/Fit*` → `FitR`).
   `links.rs` bunu çağıracak şekilde sadeleşir; #631'in 13 testi değişmeden geçer.
2. **`crates/pdfcore/src/nup/outline.rs`** (≤ 250 satır):
   `OutlineNode { title, target, color, flags, open, children }`,
   `OutlineReport { kept, dropped, truncated }`,
   `carry_outline(src, dst, &DestMap)` → `read_level` (`/Next` zinciri, `seen`
   ile döngü kesme, `MAX_DEPTH 64`, `MAX_NODES 10_000`) + `write_level`
   (`/Parent`, `/Prev`, `/Next`, `/First`, `/Last`, `/Count`).
   `/Count`: açık düğüm `+n`, kapalı `-n`, çocuksuz anahtar yok.
3. **Hedef önceliği**: `/Dest` sonra `/A`; `/A /GoTo /D` → `remap_dest`; `/URI`
   korunur; `GoToR/Launch/JavaScript/Named` düşer. Hedef her zaman
   `[sheet /FitR …]` (ya da `/XYZ` noktası) olarak normalize edilir.
4. **Budama**: hedefsiz ve çocuksuz düğüm düşer; hedefsiz ama çocuklu düğüm
   başlık olarak kalır. Hiç düğüm kalmazsa katalogda `/Outlines` yazılmaz;
   kaynakta `/PageMode /UseOutlines` varsa kalan varsa kopyalanır.
5. **Hata politikası**: `Result` yok, panik yok; bozukluk `dropped` sayacına.
   Limit aşımı `truncated` + o ana kadarki ağaç.
6. **`nup.rs` bağlama**: ratchet 362 — `mod outline;` + tek çağrı; satır
   nötrlüğü için `carry_links` + `carry_outline` birlikte tek yardımcıya alınır
   (`dests::carry_navigation`), `nup.rs`'te tek satır kalır.
7. **Testler** `crates/pdfcore/tests/nup_outline_test.rs` (≤ 400; `nup_test.rs`
   283 sabit): düz outline → sheet eşlemesi · `/XYZ` dönüşümü · iç içe yapı ve
   `/Count` işaretleri · seçim dışı yaprak düşer/üst düğüm kalır · hepsi
   düşerse `/Outlines` yok · kaynakta yoksa çıktıda yok · adlandırılmış hedef
   (name tree + eski `/Dests`) · `/A /GoTo` normalizasyonu · URI korunur, tehlikeli
   eylemler düşer · booklet slot eşlemesi · `/Next` döngüsü sonlanır · derinlik/
   düğüm limiti · bozuk öğe toleransı · başlık baytları + `/C`,`/F` korunur ·
   kaydet/yeniden yükle. Yardımcı: `assert_outline_consistent(doc)`.
8. **Belgeler**: `CHANGELOG [Unreleased]` üçüncü girdi, `docs/roadmap.md` PR-3
   kapandı (PR-2 açık), `RELEASE_READINESS` N-up satırı, `nup.rs` doc yorumu,
   README N-up maddesi.
9. **Kapılar**: `cargo fmt --all -- --check`, `cargo clippy -p pdfcore
   --all-targets -- -D warnings`, `cargo test -p pdfcore` (yeni testler + #631'in
   13'ü + mevcut 8 N-up testi), `files:check`/`files:update`, `claims:check`,
   `docs:stats` (README sayıları). Sonunda `cargo clean` + `df -h /`.

## Kapsam dışı
Markup düzleştirme (`/Text`, `/Highlight`, `/Widget`), `/GoToR` desteği, UI
seçeneği — yer imleri her zaman taşınır, yeni ayar eklenmez.

- **DURUM:** dört adım da uygulandı; PR açılıyor (`feat/nup-outline-remap`,
  `042fbb5` kod + `6a813d2` test + `c2a25f2` belge). Yerelde: pdfcore lib 197,
  eski 8 N-up testi değişmeden 8, link 9+4, **outline 13** geçti; clippy
  `-D warnings`, fmt, `files:check`, `docs:stats`, `claims:check` temiz.
  `nup.rs` 362 → **361**, `outline.rs` 236, `dests.rs`'e tek kapı (`remap_dest`
  yerine mevcut `destination`) — sapma: ayrı bir `remap_dest` fonksiyonu
  yazmadım, mevcut `DestMap::destination` zaten o işi yapıyor ve `links.rs` onu
  kullanıyor; outline de aynı fonksiyonu çağırıyor.
- **NEREDE KALDIM / SIRADAKİ ADIM:** #632 birleşti (`4af5a99`) → **v7.8.0 sürüm
  treni yürütülüyor** (bu dal). Yapılanlar: `Cargo.toml`/`Cargo.lock` +
  `package.json`/`package-lock.json` + `tauri.conf.json` 7.8.0,
  `tauri.properties` versionName 7.8.0 / versionCode **7008000**, CHANGELOG
  `[Unreleased]` → `[7.8.0] - 2026-11-06`, README sürüm/indirme bağlantıları
  7.8.0 + "What's new" cümlesi, `RELEASE_READINESS` başlığı ve N-up iddiası
  (6 test işareti), fastlane `en-US/7008000.txt` (script) + `tr-TR/7008000.txt`
  (elle). Kapılar: `claims:check` 386 iddia / 485 işaret, `fastlane:check`,
  `docs:stats`, `files:check`, `i18n:audit` temiz.
  **v7.8.0 YAYINLANDI ve doğrulandı** (2026-11-06): PR #633 → `4e67f93`, tag
  `v7.8.0` → `4e67f93`, Release + Android workflow'ları yeşil, release sayfası
  **18 varlık** (Windows setup/portable, Linux deb/AppImage, 4 Android APK/AAB,
  `latest.json`, `build-info.json`, 2 SBOM, 4 checksum dosyası),
  `SHA256SUMS.txt` 8 satır (v7.7.0 ile aynı değişmez), `latest.json` sürüm
  **7.8.0** + imza dolu (windows-x86_64), release gövdesi CHANGELOG `[7.8.0]`
  bölümüyle **birebir** (1578 karakter), Android versionCode **7008000**
  (fastlane dosyası + repo `tauri.properties`).

- **NEREDE KALDIM / SIRADAKİ ADIM:** sürüm çıktı. Sıradaki kalem: **PR-2 —
  N-up'ta baskıya çıkan açıklamaları düzleştirme** (`/F` Print biti olan
  `/Text`, `/Highlight`, `/Ink`, `/Square`, `/Line`, `/FreeText`, `/Stamp`
  türlerinin `/AP /N` akışını sheet'e çizmek); Opus planı `5062f799` aşağıda.

---

# Seçilen iş kalemi: N-up'ta baskıya çıkan açıklamaları düzleştirme (PR-2) — **Opus planı `5062f799`**

Bugün N-up çıktısında yalnız `/Link` taşınıyor; `/Text`, `/Highlight`, `/Ink`,
`/Square`, `/Stamp` gibi açıklamalar ne basılıyor ne tıklanıyor. Doğru çözüm
onları **sayfaya çizmek**.

## Kararlar (Opus)
1. **Tür listesi (15):** `Text`, `FreeText`, `Line`, `Square`, `Circle`,
   `Polygon`, `PolyLine`, `Highlight`, `Underline`, `Squiggly`, `StrikeOut`,
   `Caret`, `Ink`, `Stamp`, `FileAttachment`. Liste dışı atlanır.
2. **Koşul:** `/F & 4` (Print) var, `/F & 2` (Hidden) yok, `/AP /N` bir
   **referansa** çözülüyor (gerekirse `/AS` ile durum seçilir). Görünüm akışı
   yoksa açıklama atlanır (görünüm üretmek kapsam dışı).
3. **Dışlananlar:** `Widget` (AcroForm düzleştirme ayrı iş — yanlış alan değeri
   basmamak için), `Popup`, `Link` (zaten taşınıyor), `Redact` (güvenlik: gerçek
   karartma izlenimi), `Watermark` (`/FixedPrint`), `PrinterMark`/`TrapNet`,
   çoklu ortam türleri.
4. **Geometri:** `A = T(rx0,ry0)·S(sx,sy)·T(-bx0,-by0)` (BBox → `/Rect`;
   `map_rect(M_ap, BBox)` ile sınır kutusu), sheet'e `C = P·O·[R]·A`
   (`P = Placement.matrix`, `O = MediaBox başlangıcını sıfıra taşır`,
   `R` yalnız NoRotate'te `+r` döndürme). `M_ap` `cm`'e **konmaz** (`Do` onu
   kendi uygular). Kırpma `placed ∩ Placement.clip`, blok başına `re W n`.
5. **Çizim yeri:** kaynak sayfanın Form'una değil, **sheet içerik akışına ayrı
   `q … re W n … cm … Do … Q` blokları** (grafik durumu temiz; Form içindeki
   dengesiz `q`/`Q` sızmıyor; kaynak ad çakışması yok). Yeni akış ikinci stream,
   sheet `/Contents` dizisine eklenir; çizilecek yoksa `/Contents` ve
   `/Resources` **hiç değişmez**.
6. **Kaynaklar:** AP'nin kendi `/Resources`'u içinde kalır; sheet'e yalnız
   `/XObject /AnK`, gerekirse `/ExtGState /AnGsK` (`/CA`, `/ca`, `/BM`) ve
   `/Properties /AnOcK` (`/OC`) eklenir. Adlar `fresh_name` ile çakışmadan.
7. **Hata politikası:** her hata yalnız o açıklamayı atlar (`unwrap` yok, işlem
   düşmez); NaN/tekil matris, bozuk BBox, taşan koordinat, sarkan referans,
   `/AS` yok → atla. Sayfa başına `MAX_MARKUP_PER_SHEET = 4096`.
8. **Bütçe:** yeni `crates/pdfcore/src/nup/markup.rs` ~240 satır;
   `nup.rs` **361'de kalır** (tek `mod` satırı + doc yorumundan 1 satır kazanç),
   `sheet.rs` ve `Placement` değişmez; çağrı `links::carry_navigation` içinden.
9. **Testler:** `tests/common/markup_fixtures.rs` + `tests/nup_markup_test.rs`
   (10) + `tests/nup_markup_edge_test.rs` (7) = **17 yeni**; mevcut 34 N-up
   testi değişmeden geçer. Piksel/pdfium testi yok (pdfcore pdfium'a bağlı
   değil); doğrulama lopdf ile yapısal. Beklenen değer, aynı `/Rect`'e konan bir
   `Link`'in taşınan dikdörtgeniyle karşılaştırılarak bağımsız hesaplanır.

## Sapmalar (benden)
- Commit'lerde `Co-Authored-By: Claude` ve PR'da Claude rozeti yok (repo
  konvansiyonu DSH rozeti).
- NoRotate testinde Opus'un istediği `|b|,|c| ≈ 0` özelliği korunur; ek olarak
  `(rx0, ry1)` noktasının sabit kaldığı doğrulanır.

## Adımlar
1. `markup.rs` + `nup.rs`'te `mod markup;` + `links.rs` çağrısı + doc yorumu.
2. `markup_fixtures.rs` ve 17 test.
3. Belgeler: README N-up maddesi, roadmap (PR-2 kapandı + takip listesi),
   `RELEASE_READINESS` iddiası + işaretler, `CHANGELOG [Unreleased]`,
   `PHASE-E-PROPOSAL` madde 7, `files:update`, `docs:stats`.
4. Kapılar: fmt, clippy `-D warnings`, `cargo test -p pdfcore` (lib + 7 N-up
   dosyası), `files:check`, `claims:check`, `docs:stats`; sonunda `cargo clean`
   + `df -h /`. Tek PR `feat/nup-markup-flatten`.

- **DURUM:** dört adım da uygulandı. `nup/markup.rs` **314 satır** (15 tür,
  Print/Hidden süzgeci, `/AP /N` + `/AS`, BBox→Rect uyumu, `/NoRotate`, kırpma,
  `/CA`/`/ca`/`/BM` → ExtGState, `/OC` → BDC, 4096 sınırı); çizim **sheet içerik
  akışına** ikinci stream olarak ekleniyor, çizilecek yoksa `/Contents` ve
  `/Resources` değişmiyor. `nup.rs` **360** (ratchet düştü), `links.rs` 201.
  Testler: lib 197 · `nup_test` 8 · link 9+4 · outline 7+6 · **markup 10+7 = 17
  yeni** (hepsi geçti). Kapılar: fmt, clippy `-D warnings`, `files:check`/
  `files:update`, `docs:stats`, `claims:check` (387 iddia / 490 işaret) temiz.
  **Opus planından sapma:** dönüşüm `C = P·A` (Opus `P·O·A` demişti) — `A`
  zaten sayfa uzayına çıktığı ve `Placement.matrix` sayfa uzayı→sheet olduğu
  için `O` (MediaBox ötelemesi) gerekmiyor; link testleri bunu kanıtlıyor.
  Ayrıca `fresh_name` yardımcısı kullanılmadığı için silindi.
- **PR #634 birleşti** (`7d86c21`), 18 kontrol yeşil, dal silindi; `target/`
  temizlendi (4.1 GiB), disk %60.
- **NEREDE KALDIM / SIRADAKİ ADIM:** sıradaki kalem Opus'a soruldu
  (`96dedcbd`); dal `feat/nup-form-flatten` açıldı. Karar gelince plan aşağıya
  yazılacak ve uygulanacak.

---

# Seçilen iş kalemi: N-up'ta doldurulmuş form alanlarını (Widget) düzleştirme — **Opus planı `96dedcbd`**

Gerekçe (Opus): `[Unreleased]` tek girdiyle zayıf; widget dilimi markup
düzleştirmeyle aynı hikâyeyi tamamlıyor ("N-up'ta kâğıda çıkan her şey çıkar") ve
v7.9.0 iki girdiyle anlamlı oluyor. Kodun ~%80'i `markup.rs`'te (AP seçimi,
`/AS`, `P·A`, kırpma, ExtGState, `/OC`, hata politikası); yeni kısım yalnız alan
çözümleme ve atlama kuralları. Görsel doğrulama gerekmiyor (yeni görünüm
üretilmiyor); riskli iki kısım (`/NeedAppearances`, imzasız imza alanı) "atla"
kuralıyla çözülüyor — atlanan widget bugünkü gibi kaybolur, gerileme olmaz.

## Kapsam (dar)
1. **`crates/pdfcore/src/nup/widget.rs`** (≤180 satır):
   `AcroCtx { need_appearances }` (Root `/AcroForm /NeedAppearances`, bozuksa
   `false`), `FieldInfo { ft, ff, has_v }` — `/FT`, `/Ff`, `/V` widget'ta yoksa
   `/Parent` zincirinde aranır (derinlik 32, `HashSet` ile döngü kesme),
   `classify(doc, widget, &ctx) -> Draw | Skip(reason)`.
2. **Atlama kuralları sırası:** `/Sig` + `/V` yok → atla; `/Sig` + `/V` var →
   çiz (imza zaten geçersiz, görünüm basılır); `/Tx` veya `/Ch` +
   `NeedAppearances` → atla (bayat görünüm yanlış değer gösterebilir);
   `/Btn` → çiz; `/FT` yoksa (öksüz widget) → AP varsa çiz.
3. **`markup.rs` (+~20 satır, ≤340):** `/Widget` dışlaması kalkar, `classify`
   çağrılır; `Draw` ise mevcut yol aynen işler (Print/Hidden süzgeci,
   `/AP /N` + `/AS`, `C = P·A`, kırpma, ExtGState, `/OC`). `AcroCtx` belge
   başına bir kez kurulur. 4096 sayacı ortak.
4. **Hata politikası:** bozuk `/Parent`/`/FT`/`/Ff` yalnız o widget'ı atlar;
   `/AcroForm` okunamazsa `need_appearances = false`; çizilecek yoksa çıktı
   bayt bayt değişmez.
5. **Testler** `crates/pdfcore/tests/nup_widget_test.rs` (≤300, 15 vaka):
   checkbox `/AS On`/`Off`, `/AS` yok → atla, `/Tx` NeedAppearances yok → çiz,
   `/Tx`+`/Btn` NeedAppearances açık → yalnız Btn, `/Sig` imzasız → atla,
   `/Sig` imzalı → çiz, `/FT` yalnız `/Parent`'ta → kalıtım, `/Parent` döngüsü →
   asılmaz, Print bayrağı yok → atla, pushbutton, `/OC` → BDC, markup+widget
   karışık sıra, formsuz belge bayt bayt aynı, bozuk `/Ff` → yalnız o atlanır.
   Mevcut 34 N-up + 17 markup testi değişmeden geçer.
6. **Bütçe:** `widget.rs` ≤180 · `markup.rs` ≤340 · `nup.rs` 360 (değişmez) ·
   yeni test ≤300. `nup.rs`'e dokunmamak için `mod widget;` satırı `markup.rs`
   içinde `#[path]` ile mi yoksa `nup.rs`'te mi duracak — uygularken ratchet'e
   göre karar verilecek (nup.rs'te +1 satır olursa doc'tan 1 satır kazanılır).
7. **Kapsam dışı:** AP üretimi (`/DA`, `/MK`, NeedAppearances yeniden üretimi),
   XFA, çıktıda AcroForm'u etkileşimli tutma/temizleme, imzasız imza alanı yer
   tutucusu, `/NoZoom`, `/Watermark /FixedPrint`, etiketli PDF.
8. **Sonraki iki kalem:** (1) **v7.9.0 sürüm treni** (iki girdi; v7.8.0
   doğrulama listesi, versionCode 7009000), (2) geometri tabanlı AP üretimi
   dilim 1 (yalnız `/QuadPoints` vurguları ve `/C`,`/IC`,`/BS` şekilleri;
   FreeText/font ayrı dilim).

- **DURUM:** uygulandı. `crates/pdfcore/src/nup/markup/widget.rs` **102 satır**
  (`AcroCtx`, `/Parent` zincirinde `/FT`+`/V` kalıtımı, `classify`),
  `crates/pdfcore/src/nup/markup/install.rs` **98 satır** (yazma yarısı),
  `markup.rs` 313 → **234** (ratchet düştü), `nup.rs` 360 değişmedi.
  Testler: **16 yeni** (`nup_widget_test.rs`) — checkbox On/Off, `/AS` yok,
  `/Tx` güncel, `NeedAppearances` açıkken yalnız düğme, imzasız imza atlanır,
  imzalı basılır, `/Parent` kalıtımı (hem güncel hem bayat durumda), `/Parent`
  döngüsü, Print bayrağı yok, pushbutton, `/OC`+`/CA`, markup+widget sırası,
  formsuz belge değişmez, sarkan `/Parent`, bozuk `/FT`. Toplam N-up testi
  **34 + 17 + 16 = 67**; lib 197. Kapılar: fmt, clippy `-D warnings`,
  `files:check`, `docs:stats`, `claims:check` (388 iddia / 494 işaret) temiz.
  **Opus planından sapma:** `/Ff` okunmuyor (sınıflandırma `/FT` + `/V` +
  `NeedAppearances` ile yapılıyor); "bozuk `/Ff`" testi yerine "bozuk `/FT`"
  testi var ve bozuk tip görünüm akışına düşüyor. `mod widget;` `nup.rs` yerine
  `markup.rs` içinde (nup.rs 360'ta kaldı).
- **PR #635 birleşti** (`a1c75ec`), 18 kontrol yeşil, dal silindi; `target/`
  temizlendi (4.2 GiB), disk %60.
- **v7.9.0 YAYINLANDI ve doğrulandı** (2026-11-06): PR #636 → `74b9af5`, tag
  `v7.9.0` → `74b9af5`, Release + Android + auto-tag + Desktop yeşil, **18
  varlık**, `SHA256SUMS.txt` 8 satır (değişmez), `latest.json` sürüm **7.9.0**
  + imza dolu, release gövdesi CHANGELOG `[7.9.0]` bölümüyle **birebir** (1425
  karakter), Android `versionName 7.9.0` / `versionCode **7009000**`.
- **DURUM (görünüm akışı sentezi, Opus planı `4adbb2a9`):** uygulandı.
  `nup/markup/appearance.rs` **292 satır** (renk `/C`/`/IC`, `/BS`/`/Border`
  kalınlığı, `Square`, `Circle` 4 Bézier, `Line` — `/LL` varsa atla, `Ink`),
  `nup/markup/appearance/quads.rs` **199 satır** (`QuadPoints` normalizasyonu,
  `Highlight` tek dolgu + Multiply, `Underline`/`StrikeOut`/`Squiggly`),
  `markup.rs` 234 → **259** (`Look { Stream, Synth }`, "`/AP` varsa asla
  sentezleme" kuralı), `install.rs` 98 → **133** (sentezlenen içerik için kendi
  Form XObject'i; Multiply durumu formun kendi `/Resources`'unda), `nup.rs`
  **360** (değişmedi). Testler: **14 yeni** (`nup_appearance_test.rs`, 397
  satır) — highlight dolgu+blend, quad sırası, underline/strike, squiggly bandı,
  square stroke+fill, dolgu-only, circle 4 Bézier, line, leader atlanır, ink
  çoklu yol, bozuk `/AP` sentezlenmez, bozuk geometri yalnız kendini atlar,
  Hidden/Print yok, döndürülmüş sayfada aynı operatörler. Toplam N-up testi
  **67 + 14 = 81**; lib 197. Kapılar: fmt, clippy `-D warnings`, `files:check`/
  `files:update`, `docs:stats`, `claims:check` (389 iddia / 498 işaret) temiz.
  **Opus planından sapmalar:** (1) sentez için ayrı `install` yolu değil, kendi
  Form XObject'i — planın B seçeneği; `w` ölçeklenmesi ve kaynak izolasyonu
  için. (2) `QuadPoints` sırası "üst kenar daha yüksek olan" kuralıyla
  normalize ediliyor (Z ve CCW sıraları aynı yolu veriyor); döndürülmüş metin
  quads'ı bilinen sınır. (3) Quads kodu ayrı alt modülde (dosya tavanı).
- **PR #637 birleşti** (`b925d0e`), 18 kontrol yeşil (coverage dahil), dal
  silindi; `target/` temizlendi (4.5 GiB), disk %60. Yol boyunca iki CI
  düzeltmesi: coverage ratchet'i (birim testleri eklendi, `appearance.rs` ve
  `quads.rs` tabanları yükseltildi) ve README `docs:stats` bloğu.
- **v7.10.0 YAYINLANDI ve doğrulandı** (2026-11-06): PR #638 → `2b7bb51`, tag
  `v7.10.0` → `2b7bb51`, Release + Android + auto-tag yeşil, **18 varlık**,
  `SHA256SUMS.txt` 8 satır, `latest.json` sürüm **7.10.0** + imza dolu, release
  gövdesi CHANGELOG `[7.10.0]` bölümüyle **birebir** (843 karakter), Android
  `versionName 7.10.0` / `versionCode **7010000**`.

---

# Seçilen iş kalemi: N-up FreeText sentezi, A dilimi (standart 14 font) — **Opus planı `000577b5`**

Bugün `/AP`'siz `FreeText` açıklamaları N-up'ta kayboluyor (kullanıcının yazdığı
metin). Riskin büyük kısmı gömülü/`/DR` fontları ve CID kodlamadan geldiği için
A dilimi **yalnız standart 14 fontla** yapılıyor; çıktıda her zaman kendi
ürettiğimiz Type1 sözlüğü kullanılıyor, genişlikler Core14 AFM tablolarından
geliyor, yeni bağımlılık yok. Türkçe (ğ ş ı İ Ğ Ş) WinAnsi'de olmadığı için sabit
bir `/Differences` tablosuyla çözülüyor.

## Plan
1. **`crates/pdfcore/src/nup/markup/appearance/std14.rs`** (≤260):
   `Family { Helvetica, Times, Courier }`, `Std14 { family, bold, italic }`,
   `base_font()`, `width(code)`, `ascent()`/`descent()`; WinAnsi 32..255
   genişlik tabloları (AFM'den üretilir), Courier sabit 600;
   `EXTRA_GLYPHS` (kod 1..31, Türkçe 6 + Latin-A) — aksanlı glif taban harfle
   aynı genişlikte.
2. **`appearance/textfit.rs`** (≤220): `decode_text_string` (UTF-16BE BOM, UTF-8
   BOM, PDFDocEncoding), `encode(font, s)` (kodlanamayan → `None`), `wrap(...)`
   (açgözlü kelime kırma, taşan tek kelime karakter bazında, `
` tek kırılım,
   `max_lines`).
3. **`appearance/freetext.rs`** (≤320): `/DA` çözümleme (`/Ad size Tf`, son renk
   operatörü `g`/`rg`/`k`; kaynak sırası annot → `/AcroForm /DA` → `Helv 12 0 g`),
   `/Q` hizalama (0/1/2), `/C` arka plan, kenar (`/BS /W` → `/Border`), `/RD`
   içe çekme, `/CL` callout (4 veya 6 sayı), satır yerleşimi
   `b₁ = ty1 − size·ascent/1000`, `lead = 1.2·size`, metin alanı 2 pt pad.
4. **İçerik akışı:** arka plan `re f`, kenar `re S`, callout `m/l S`, metin
   kutusu `re W n`, `BT /FOmni0 size Tf <renk> 1 0 0 1 x b Tm <hex> Tj … ET`;
   dizeler her zaman hex (kaçış riski yok).
5. **`appearance.rs`** dispatch'e `b"FreeText"` kolu (+≤3 satır);
   **`install.rs`** sentezlenen formun `/Resources`'una `/Font` (+≤8 satır).
   `nup.rs`, `markup.rs`, `quads.rs` dokunulmaz.
6. **Hata politikası:** kodlanamayan karakter (CJK/emoji) → **açıklamanın
   tamamı atlanır** (`?` ikamesi yok); `/Rect` geçersiz, boyut >1000, bozuk
   `/DA` (fallback'ler tükendiyse) → atla; `/Contents` boş → yalnız kutu.
7. **Testler:** birim (AFM genişlikleri, `EXTRA_GLYPHS`, `/DA` çözümleme, aile
   eşleme, kodlama/çözme, ölçü, `wrap`, hizalama) + entegrasyon
   `tests/nup_freetext_test.rs` (12 vaka: `/AP`'siz metin çizilir + `/Differences`,
   `/AP` varsa sentez yok, `/Q 2` sağa yaslar, `/C` arka plan, `/BS 0` kenarsız,
   `/RD` içe çekme, callout 4/6/5 sayı, CJK+Latin karışık sayfada yalnız Latin,
   boş içerik yalnız kutu, taşan metin kırpılır, AcroForm `/DA` fallback,
   determinizm). Mevcut 81 N-up testi + lib 197 kırılmamalı.
8. **Kapılar:** `cargo test -p pdfcore` (lib + 10 N-up dosyası), fmt, clippy
   `-D warnings`, `files:check`/`files:update`, `docs:stats`, `claims:check`;
   sonunda `cargo clean` + `df -h /`. Tek PR `feat/nup-freetext-std14`.

## Kapsam dışı (B dilimi)
`/DR`/gömülü font yeniden kullanımı, Type0/CID, `/RC`, `/DS`, RTL/CJK/emoji,
otomatik boyut, callout `/LE` uçları, kesikli kenar, `/BE`, `/Rotate`.

- **DURUM:** uygulandı. `std14.rs` **197 satır** (genişlik tabloları **Adobe
  Core14 AFM** dosyalarından üretildi — matplotlib'in `pdfcorefonts` dağıtımı,
  kaynak yorumda; Courier sabit 600; ascent/descent AFM'den), `textfit.rs`
  **242** (UTF-16BE/UTF-8/PDFDocEncoding çözme, WinAnsi + 36 özel glif kodlama,
  ölçü, açgözlü `wrap`), `freetext.rs` **316** (`/DA` → `/AcroForm /DA` →
  `Helv 12 0 g`, `/Q`, `/C`, `/BS`·`/Border`, `/RD`, `/CL`, satır yerleşimi),
  `appearance.rs` 363 → **371** ve `markup.rs` 259 → **261** (ratchet'ler
  bilinçli yükseltildi: `FreeText` dispatch + `Synth.font`), `install.rs` 161
  (sentez formuna `/Font` + `/Differences`). Testler: **10 entegrasyon**
  (`nup_freetext_test.rs`) + **14 birim** (std14/textfit/freetext); lib 197 →
  **211**, toplam N-up testi **91**. Kapılar: fmt, clippy `-D warnings`,
  `files:check`/`files:update`, `docs:stats`, `claims:check` (390 iddia / 501
  işaret) temiz.
  **Opus planından sapmalar:** (1) `std14.rs` tabloları elle değil AFM'den
  üretildi (uydurma genişlik yok); (2) `wrap`/`encode` ayrı `textfit.rs`'te,
  callout ve `/RD` `freetext.rs` içinde; (3) kodlanamayan karakter **tüm
  açıklamayı** düşürüyor (plan gereği), test bunu sabitliyor.
- **PR #639 birleşti** (`37a0c8c`), 18 kontrol yeşil (coverage dahil), dal
  silindi; `target/` temizlendi (4.8 GiB), disk %60.
- **DURUM (dilim 1, Opus planı `4a676a26`):** uygulandı. Yeni
  `appearance/border.rs` **187** (`Stroke { width, dash }`, `/BS /S /D` +
  varsayılan `[3]`, eski `/Border` 4. elemanı, bozuk desen → düz) ve
  `appearance/paths.rs` **208** (`/L`, `/InkList`, `/Vertices`; `polygon`
  kapalı/açık, `Polyline`'da `/IC` yol dolgusu değil); `appearance.rs` 371 →
  **253** (line/ink/border taşındı), `freetext.rs` **316** (çerçeve ve callout
  artık dash taşıyor). **11 yeni entegrasyon testi**
  (`nup_shape_polish_test.rs`) + border/paths birim testleri; lib 211 → **219**,
  N-up testi **102**. Kapılar: fmt, clippy `-D warnings`, `files:check`/
  `files:update`, `docs:stats`, `claims:check` (391 iddia / 505 işaret) temiz.
  **Opus planından sapma:** `/LE` çizgi uçları bu PR'a alınmadı (Opus'un önerisi
  gibi ikinci PR'a bırakıldı); `callout()` `freetext.rs`'te kaldı (ends.rs
  açılmadı).
- **PR #640 birleşti** (`7c98fa5`), 18 kontrol yeşil, dal silindi; `target/`
  temizlendi (4.5 GiB), disk %60.
- **DURUM (dilim 2):** uygulandı. Yeni `appearance/ends.rs` **338** (`Ending`
  10 tür, `/LE` çift/tek okuma, `outward` yinelenen noktaları atlar,
  `size = 3·max(w,2)`, tüm uç geometrileri, ters oklar, `q [] 0 d … Q` bloğu,
  bbox payı); `paths.rs` line/polyline uçları (`paint_ends`), `freetext.rs`
  callout'ta tek uç. Ratchet'ler bilinçli yükseltildi: appearance.rs 253 → 254,
  freetext.rs 316 → 332, paths.rs 208 → 232 (yeni özellik kodu).
  **11 yeni entegrasyon testi** (`nup_line_ends_test.rs`) + **10 birim testi**;
  lib 219 → **229**, N-up testi **113**. Kapılar: fmt, clippy `-D warnings`,
  `files:check`/`files:update`, `docs:stats`, `claims:check` (392 iddia / 509
  işaret) temiz.
- **PR #641 birleşti** (`67faa4c`), 18 kontrol yeşil, dal silindi; `target/`
  temizlendi (4.6 GiB), disk %60.
- **v7.11.0 YAYINLANDI ve doğrulandı** (2026-11-07): PR #642 → `38a5bbe`, tag
  `v7.11.0` → `38a5bbe`, Release + Android + auto-tag yeşil, **18 varlık**,
  `SHA256SUMS.txt` 8 satır, `latest.json` sürüm **7.11.0** + imza dolu, release
  gövdesi CHANGELOG `[7.11.0]` bölümüyle **birebir** (1983 karakter), Android
  `versionName 7.11.0` / `versionCode **7011000**`.

---

# Seçilen iş kalemi: FreeText B dilimi — `/DR` basit font yeniden kullanımı — **Opus planı `c8a4bbba`**

Gömülü fontlu formlarda N-up çıktısı Helvetica'ya düşüyor; hem görünüm hem satır
kırma ölçüsü bozuluyor. B dilimi `/DR`'deki **basit fontları** (Type1/TrueType,
`/Widths` + WinAnsi/`/Differences`) yeniden kullanır; Type0/CID, Type3,
MacRoman/Standard kodlamalar kapsam dışı. Her belirsizlikte A dilimine
(standart 14) düşülür — bugünkü çıktıdan kötü sonuç çıkmaz.

## Plan
1. **Ölçüm arayüzü** (`textfit.rs`, +~30): `trait FontMetrics { encode(char) -> Option<u8>, width(u8) -> f32, ascent() -> f32, descent() -> f32 }`; `Std14Metrics` mevcut davranışı birebir sarar (113 test değişmeden). Ölçüm: `size/1000 · Σ width(code)`.
2. **`appearance/drfont.rs`** (≤400, hedef ~260): `resolve_dr_font(doc, da_name)` — `Catalog /AcroForm /DR /Font /<ad>` (önce annotation `/DR`); kabul: `/Subtype` Type1|MMType1|TrueType, `/Widths` + `/FirstChar`, kodlama WinAnsi ya da `/Differences`'lı sözlük; Type0/Type3/MacRoman/Standard/symbolic-TrueType → `None`. `DrFont { name, widths, first_char, missing_width, fwd[256], rev, ascent, descent }`; genişlik `W[c-FC]` (aralık dışı → `MissingWidth`), `LastChar` üst sınır; glyph adı → Unicode (`AGL` alt kümesi, `uniXXXX`, `uXXXX`), ters tabloda en küçük kod; `w==0` (boşluk hariç) = "glif yok" → A yolu; ascent/descent `/FontDescriptor`'dan, makul değilse Helvetica.
3. **`freetext.rs`** (+~25): `/DA` adı → `resolve_dr_font`; tüm karakterler kodlanabiliyor ve `w>0` ise B yolu, değilse **tamamen** A yolu (karakter bazında karıştırma yok); içerik akışında kaynak adı `/DA`'daki ad, metin hex `<…> Tj`; `/Q`, `/RD`, callout, `/LE` aynı `&dyn FontMetrics` üzerinden.
4. **Font nesnesi**: sentez aynı belgede yapılıyorsa `/DR` referansı doğrudan `/Resources /Font`'a; değilse mevcut kopyalayıcı + oturum önbelleği; `install.rs` ≤+15 satır, `nup.rs` 360 (değişmez).
5. **Testler**: yeni `tests/nup_freetext_dr.rs` (~14 vaka): `/DR` Type1 WinAnsi çizilir + doğru hex · dar `/Widths` satır kırmayı değiştirir · `/Differences` ile Türkçe · kodlanamayan karakter → A yolu · `w==0` → A · Type0/Type3 → A · `/Widths` yok → A · `/DA` adı yok → A · bozuk `FontDescriptor` → güvenli çıktı · iki sayfada aynı font tek nesne · `0 Tf` kutuya sığar · callout/`/LE` gerilemez · `/AP` varsa dokunulmaz. + `drfont` birim testleri (~8). Hedef: lib ~237, N-up **127**.
6. **Bütçe**: `drfont.rs` yeni · `freetext.rs` 332 → ~357 · `textfit.rs` 242 → ~272 · `std14.rs` 197 → ~210 · `install.rs` ≤176 · `appearance.rs` +1 (`mod`).
7. **Kapılar**: fmt, clippy `-D warnings`, `cargo test -p pdfcore`, `files:check`/`update`, `docs:stats`, `claims:check`; sonunda `cargo clean` + `df -h /`. Tek PR `feat/nup-freetext-dr`.

## Kapsam dışı
Type0/CID/Identity-H · Type3 · MacRoman/Standard/yerleşik kodlama · font dosyasını ayrıştırıp glif doğrulama · karakter bazında font karıştırma · `/RC`, `/DS` · font gömme/alt kümeleme.

- **DURUM:** uygulandı. `textfit.rs` **308** (`FontMetrics` trait + `Std14Metrics` + `wrap_with` + `win_ansi_symbol` + `glyph_symbol`), yeni `drfont.rs` **305** (`DrFont`, `resolve_font`, `/Widths`+`/FirstChar`, WinAnsi + `/Differences`, `uniXXXX`/`uXXXX`, `can_show`, `/FontDescriptor` ascent, reddetme kuralları), `freetext.rs` **383** (`Face { Standard, Document }`, B→A tam düşüş, kaynak adı `/DA`'dan), `markup.rs` **271** (`FontSpec { Standard, Resource }`), `install.rs` **?** (Resource font sözlüğü). Testler: **10 entegrasyon** (`nup_freetext_dr.rs`) + **4 birim** (`drfont`); lib **233**, N-up testi **123**. Kapılar: fmt, clippy `-D warnings`, `files:check`/`files:update`, `docs:stats`, `claims:check` (393 iddia / 513 işaret) temiz.
  **Opus planından sapmalar:** (1) font nesnesi kopyalanmıyor — `/DR`'deki **referans** doğrudan forma yazılıyor (sentez aynı belgede yapılıyor), bu yüzden kopyalama/önbellek adımı gerekmedi; (2) inline (dolaylı olmayan) font sözlüğü reddedilip A yoluna düşülüyor; (3) `descent` trait'ten çıkarıldı (kullanılmıyor).
- **PR #643 birleşti** (`336d7ce`), 18 kontrol yeşil (coverage dahil), dal silindi; `target/` temizlendi (5.3 GiB), disk %60.
- **DURUM (Opus planı `f8a72fc4`, PR-A'nın leader kısmı):** uygulandı.
  `paths.rs` **~300**: `LEADER_SIDE` (sol normal), `leaders()` (`/LL` kaydırma,
  `/LLE` uzatma, `/LLO` boşluk, negatif `/LL` taraf değiştirir), `line()`
  artık leader'lı çizgiyi **atlamıyor** — kaydırılmış çizgi + iki bacak tek
  yolda, `/LE` uçları kaydırılmış çizgiye, bbox bacakları kapsıyor, dash
  bacaklara da uygulanıyor; çakışık uçlu (yönsüz) çizgi atlanıyor.
  **10 yeni entegrasyon testi** (`nup_polish_test.rs`) + **3 birim testi**;
  `nup_appearance_test::a_line_with_a_leader_is_skipped` davranış değişikliği
  nedeniyle `a_line_with_a_leader_is_drawn_beside_it` oldu. lib 233 → **236**,
  N-up testi **123 → 133**. Kapılar: fmt, clippy `-D warnings`, `files:check`/
  `files:update`, `docs:stats`, `claims:check` (394 iddia / 517 işaret) temiz.
  **Opus planından sapma (bilinçli bölme):** PR-A'nın yalnız **leader** kısmı
  yapıldı; `/BS /S /B/I/U` stilleri, `/Cap` caption ve `/NoZoom` sonraki
  dilimlere bırakıldı (Opus da üç ayrı commit/PR önermişti; `/Cap` ve stiller
  ayrı PR olarak planlanacak).
- **PR #644 birleşti** (`e57e84a`), 18 kontrol yeşil, dal silindi; `target/`
  temizlendi (4.8 GiB), disk %60.
- **DURUM (Opus `f8a72fc4` §2.2–2.3):** uygulandı. `border.rs` **~250**
  (`Edge { Plain, Beveled, Inset, Underline }`, `/S` ayrıştırma, `lighter`/
  `darker`, `k = 0.25`), yeni `bevel.rs` **~250** (`rect_frame` Square+FreeText,
  `ellipse_frame` Circle — 15°'lik yay segmentleri, `polygon_frame` Polygon —
  shoelace ile normal, kenar başına ton; küçük şekil → Plain; Line/PolyLine/Ink
  düz), `appearance.rs` **~300**, `paths.rs` **~310**, `freetext.rs` **~390**.
  **9 yeni entegrasyon testi** (`nup_border_styles_test.rs`) + **7 birim testi**
  (`bevel.rs`); lib 236 → **243**, N-up **133 → 142**. Kapılar: fmt, clippy
  `-D warnings`, `files:check`/`files:update`, `docs:stats`, `claims:check`
  (395 iddia / 521 işaret) temiz.
  **Opus planından sapmalar:** (1) B/I bantları L-şekli yerine dört dikdörtgen
  bandı olarak boyanıyor (aynı görünüm, daha az kod; köşe önceliği belgelendi);
  (2) Circle bantları Bézier yerine 15°'lik poligon segmentleriyle (belgelendi);
  (3) FreeText çerçevesi `bevel`'e taşındı ama metin iç boşluğu `2w` yerine
  `w + 2` kaldı (davranış testle sabit).
- **PR #645 birleşti** (`9f7613f`), 18 kontrol yeşil, dal silindi; `target/`
  temizlendi (4.9 GiB), disk %60.
- **NEREDE KALDIM / SIRADAKİ ADIM:** dal `chore/release-v7.12.0` açıldı.
  Sıradaki iş **v7.12.0 sürüm treni** — `[Unreleased]` **iki** girdi taşıyor
  (`/LL` leader'ları #644, `/BS` bevel/inset/underline #645). Adımlar v7.11.0
  ile aynı: sürüm 7.12.0, `tauri.properties` versionCode **7012000**, CHANGELOG
  `[Unreleased]` → `[7.12.0] - 2026-11-07`, README + indirme bağlantıları,
  `RELEASE_READINESS` başlığı, fastlane `en-US/7012000.txt` + `tr-TR`, PR →
  merge → auto-tag → 18 asset doğrulaması. Sonra `/Cap` caption ya da
  `/NoZoom` dilimi (Opus'a sorulacak).

---

# Seçilen iş kalemi: LAN istek imzası v2 (`offset`/`total`/`sha256`) + features anlaşması — **Opus planı `f6a28999`**

`canonical()` imzası bugün `offset`, `total` ve `sha256` alanlarını kapsamıyor;
`get_chunk` isteği hangi aralığın ve hangi dosya sürümünün istendiğini
bağlamıyor. TLS sabitleme ve tekrar koruması riski düşürüyor ama bu, savunmayı
tek katmana bırakmak demek. Chief'in açık talebi; v7.13.0'a sessiz bir güvenlik
dilimi olarak giriyor.

## Plan (dilim A — v1 açık kalır)
1. **Ön okuma:** `crates/synccore/src/lan.rs` (`canonical`/`sign`/`verify`
   kapsamı), imza algoritması (Ed25519 mi HMAC-SHA256 mi — mevcut bağımlılık),
   `lan_sync.rs` el sıkışması (`protocol_version`, `deny_unknown_fields` yok mu),
   `begin_get`/`get_chunk` (`begin_put`/`put_chunk` da kapsama), eşleştirilmiş
   cihaz kaydının yeri.
2. **`crates/synccore/src/lan/sig.rs` (≤220):** `SigVersion { V1, V2 }`;
   `canonical_v1` bugünkü baytları **birebir** korur; `canonical_v2` alan
   ayrımı önekiyle (`b"omnioffice-lan-sig/v2\0"`) ve **uzunluk önekli/sabit
   genişlikli** alanlarla: op, sender_id, receiver_id, transfer_id, file_id,
   offset (u64be), length (u64be), total (u64be; bilinmiyorsa `u64::MAX`),
   sha256 (32 bayt; bilinmiyorsa sıfır), nonce, timestamp (i64be).
3. **Kablo (geriye uyumlu):** isteklere `#[serde(default)] sig_v: u8` (yoksa 1);
   `get_chunk`, `begin_get` yanıtındaki `total`+`sha256`'yı geri yollar ve
   imzalar; sunucu oturumla karşılaştırır. El sıkışmaya
   `#[serde(default)] features: Vec<String>`; yeni cihaz `"sig-v2"` ilan eder.
   `protocol_version` **artırılmaz**.
4. **Anlaşma:** kesişim V2 ise V2; `features` el sıkışma transkriptine girer
   (düşürme engeli). **TOFU sabitleme:** bir eşten bir kez geçerli V2 gelirse
   kayda `min_sig: 2` yazılır, o eşten sonraki V1 reddedilir.
5. **Hata politikası:** imza tutmazsa `BadSignature` (hangi alan olduğu karşıya
   söylenmez, `debug`); `offset > total` / `offset+length > total` /
   oturumla uyuşmayan `total`/`sha256` → `RangeMismatch` (imza doğrulandıktan
   sonra); sürüm düşürme → `SigDowngrade` + bağlantı kapanır; V1 eşle sessizce
   devam (`info`, oturum başına bir kez).
6. **Testler (34 → ~52 `lan::`):** golden v1 (değişmedi) + golden v2, alan
   mutasyonları (offset/length/total/sha256/transfer/receiver/op/nonce/ts),
   uzunluk-öneki belirsizliği, v1↔v2 ayrımı; anlaşma matrisi (yeni/yeni,
   yeni/eski, eski/yeni, sabitlenmiş eş, bilinmeyen özellik, eski JSON);
   indirme: tam, devam, offset/total/sha kurcalama, eski eş V1, tekrar.
7. **Bütçe:** `sig.rs` ≤220; `lan.rs` net ≤+40; `lan_sync.rs` ≤+60; test sayısı
   ratchet'i güncellenir.
8. **Kapılar:** `cargo fmt`, `cargo clippy -p synccore -- -D warnings`,
   `cargo test -p synccore` + CI tamamı; protokol belgesi (v2 biçimi + golden
   vektör + anlaşma tablosu) ve CHANGELOG "Güvenlik"; sonunda `cargo clean` +
   `df -h /`. Tek PR: `feat(synccore): LAN istek imzası v2`.

## Kapsam dışı
Parça başına MAC, V1'in kaldırılması/zorunlu V2 (dilim B), anahtar döndürme,
TLS sabitleme değişikliği, UI/ayar, `protocol_version` artışı.

## Olay: v7.12.0 sürüm commit'inde CHANGELOG bölümü eksikti (2026-11-07)

- Sürüm commit'i `bcffd8a` CHANGELOG'da `## [7.12.0]` bölümü **olmadan** çıktı
  (düzenleme, sürüm hazırlığı sırasında kayboldu; aynı şekilde `en-US/7012000.txt`
  fastlane dosyası da commit'e girmemişti).
- Sonuç: `Verify and publish Android packages` "changelog-section: no
  '## [7.12.0]'" ile düştü, APK/AAB yüklenmedi; Release işi de aynı adımda
  düşecekti. Release sayfası hiç oluşmadı → kullanıcı etkilenmedi.
- **Kurtarma (Opus `28ad3bf0` kararı, seçenek 1):** düzeltme PR #647
  (`132c8e4` CHANGELOG + `c2d8978` fastlane) → master `f503e88`; **tag `v7.12.0`
  bu commit'e taşındı** (`git tag -f` + force push, eski `bcffd8a`); tag push'u
  Release + Android'i yeniden tetikledi. Doğrulama: tag → `f503e88`, **18 asset**,
  `latest.json` **7.12.0** + imza, `SHA256SUMS.txt` 8 satır, release gövdesi
  CHANGELOG `[7.12.0]` bölümüyle **birebir** (1233 karakter), Android
  versionCode **7012000**.
- **Önleyici (sıradaki kalem adayı):** `release-guard` işi — sürüm değişen her
  PR'da `## [X.Y.Z]` bölümü var ve boş değil mi diye bakan zorunlu kontrol;
  ayrıca auto-tag tag atmadan önce aynı kontrolü çalıştırsın ve
  Release/Android'de bu adım derlemeden **önce** koşsun.

- **DURUM (Opus `f6a28999`, dilim A):** uygulandı. Yeni `lan/sig.rs` **230**
  (`SigVersion{V1,V2}`, `canonical_v1` bayt bayt aynı, `canonical_v2` önek +
  uzunluk-önekli alanlar: op/sender/receiver/transfer_id/path/offset/length/
  body_hash/total/sha256/nonce/timestamp, `sign`/`verify`, `features`
  temizleme) + `lan/auth.rs` **120** (`authenticated` buraya taşındı: sürüm
  seçimi, TOFU `min_sig=2` sabitleme, `bad-signature`/`sig-downgrade`/
  `range-mismatch` kodları, `refusal_error`) + `lan/sig_tests.rs` **230**
  (11 birim: v1/v2 golden vektörleri, alan mutasyonları, uzunluk-öneki
  belirsizliği, anlaşma matrisi) + `lan/sig_negotiation_tests.rs` **113**
  (4 loopback: özellik alışverişi, eski çift V1, düşürme reddi, pin).
  Kablo: `Request.sig_v`+`features`, `Response.code`+`features`,
  `PairedPeer.features`+`min_sig` (`#[serde(default)]`), `protocol_version`
  artmadı; `get_chunk` artık `total`+`sha256` yolluyor ve sunucu ilk parçada
  hash'i doğruluyor. `SyncError::{BadSignature,SigDowngrade,RangeMismatch}`
  eklendi. lib testi 91 → **107**, toplam Rust testi 1629 → 1646.
  **Opus planından sapmalar:** (1) `features` SPAKE2 transkriptine
  **katılmadı** — katılsaydı eski sürümle eşleşme ("the code does not match")
  kırılırdı; kanal zaten TLS pinle ve anahtar iki sertifikaya bağlı, düşürme
  engeli TOFU pini + `sig-downgrade` ile sağlanıyor (belgelendi);
  (2) `transfer_id` telde ayrı alan değil, `path`+`total`+`sha256`'dan türetilen
  32 bayt (yeni kablo yüzeyi açmamak için); (3) `length` = istek gövde uzunluğu;
  (4) sunucu `sha256`'yı her parçada değil yalnız ilk parçada doğruluyor (tam
  okuma maliyeti); (5) dosya bütçesi: `lan.rs` bölündü (1270 → 1252) ve testler
  ayrı dosyaya taşındı; `files:update --allow-raise` ile 5 küçük bütçe
  yükseltildi (download +7, pairing +10, lan_sync +3, store +2, sync +4).

- **PR #648 birleşti** (`6d497a3`), 17 kontrol yeşil + 2 beklenen skip
  (Benchmarks, Fuzz), dal silindi; `target/` temizlendi (3.2 GiB), disk %60.
  README Rust testi 1629 → **1646**; sürüm değişmedi (v7.12.0 güncel).
- **NEREDE KALDIM / SIRADAKİ ADIM:** sıradaki kalem için Opus kararı bekleniyor
  (aday: `release-guard` CI kontrolü — v7.12.0 CHANGELOG olayının önleyicisi;
  `/Cap` caption; sig-v2 dilim B).

---

# Seçilen iş kalemi: `release-guard` — sürüm notu olmayan sürümü CI'da durdurmak — **Opus kararı `fd9c9cc0`**

Opus üç adayı karşılaştırdı ve `release-guard`'ı seçti (v7.12.0 olayının
tekrarını önleyen tek kalem, VPS'te tamamen doğrulanabilir, ~1 CI turu).
**Not:** Opus bu oturumda dosya okuyamadı (Bash/Read kapalı) → workflow
ayrıntıları varsayımdı; aşağıdaki Adım 0 bulguları benim ölçümümdür.

## Adım 0 — ölçülen gerçekler (varsayımlar düzeltildi)
- Sürüm kaynakları: `package.json` `"version"`, `src-tauri/tauri.conf.json`
  `"version"`, `Cargo.toml` `[package] version`; ayrıca **üretilen**
  `src-tauri/gen/android/app/tauri.properties` (`versionName`, `versionCode`;
  repoda izleniyor, auto-tag ondan okuyor). `src-tauri/Cargo.toml`
  `version.workspace = true` → **guard bunu okumaz** (ek dolaylılık yok).
- `versionCode` formülü yalnız `scripts/build-android.ps1:176` (PowerShell) ve
  `scripts/fastlane-changelogs.mjs:38` içinde; **ikisi de export değil** ve
  `fastlane-changelogs.mjs` modül yüklenirken `process.argv` okuyor → import
  edilemez. Karar: formül guard'da kopyalanır ve testle sabitlenir
  (7.12.0 → 7012000); kaynak yorumda gösterilir.
- `scripts/changelog-section.mjs` fonksiyonu export etmiyor ve çıktısı release
  gövdesini üretiyor → **refactor edilecek**: `export function
  extractSection(text, version)` + `main` guard. CLI çıktısı bayt bayt aynı
  kalmalı (`node scripts/changelog-section.mjs 7.12.0` önce/sonra diff).
- `fastlane:check` `desktop.yml:65`'te (ESLint/claims/docs-stats/acl ile aynı,
  ilk iş). Guard adımı **oraya**, fastlane adımının hemen arkasına girer;
  yeni iş açılmaz (branch protection değişmesin).
- `auto-tag.yml` **checkout yapmıyor** (Scorecard Dangerous-Workflow): 4 dosyayı
  Contents API ile çekip bash'te sürüm + Android metadata karşılaştırıyor.
  Guard oraya **fetch tabanlı** girer: `CHANGELOG.md`, `scripts/release-guard.mjs`
  ve `fastlane/metadata/android/en-US/changelogs/<expected_code>.txt` de
  çekilir, sonra `node scripts/release-guard.mjs --root . --tag "v$version"`
  (kod `$((major*1000000+minor*1000+patch))` bash'te zaten var).
- `release.yml` / `android.yml`: tag'de checkout var → tag'in ilk işinin ilk
  adımı `node scripts/release-guard.mjs --tag "$TAG"` (env ile geçirilir),
  böylece 1 saatlik derlemeden önce saniyede düşer.
- Test koşucusu: `vite.config.ts` yalnız `src/**/*.test.ts(x)` topluyor →
  `scripts/release-guard.test.mjs` vitest'e girmez. Karar: `node:test`
  (`node --test scripts/release-guard.test.mjs`) ve **aynı CI adımında**
  guard'la birlikte koşulur (`… && node scripts/release-guard.mjs`).

## Dosyalar
1. **`scripts/release-guard.mjs`** (yeni, ~180): `readVersions(root)`,
   `versionCode(version)`, `sectionBody(changelog, version)`,
   `isMeaningful(body)`, `checkRelease({root, tag})`; hataları toplar
   (`::error file=…::…`), başarıda tek satır
   `release-guard: 7.12.0 (versionCode 7012000) OK`. Kontroller:
   (1) üç sürüm dosyası + `tauri.properties` versionName/versionCode aynı,
   (2) `X.Y.Z` biçimi, (3) `## [X.Y.Z]` bölümü var (changelog-section ile aynı
   ileti), (4) bölüm anlamlı (yalnız `###` başlık / `<!-- -->` / `TBD` değil),
   (5) `en-US/changelogs/<code>.txt` var ve boş değil, (6) `--tag` verilirse
   `v` soyulmuş hâli sürüme eşit.
2. **`scripts/changelog-section.mjs`** (+6): `extractSection` export + main
   guard; CLI davranışı aynı.
3. **`scripts/release-guard.test.mjs`** (yeni, ~200, `node:test`): normal
   geçiş; v7.12.0 bölümü yok (olay tekrarı, ileti `no '## [7.12.0]' section`);
   yalnız boş `###`; yalnız yorum/`TBD`; `[7.1.0]` ≠ `[7.1.10]`; bölüm bir
   sonraki `##`de biter (`###` bitirmez); tarih son ekli başlık; `[Unreleased]`
   yok sayılır; fastlane dosyası yok/boş; versionCode formülü; sürüm uyuşmazlığı
   (her dosya adı geçer); tag uyuşmazlığı; `v` öneki; **gerçek HEAD geçer**.
4. **`desktop.yml`** (+3): fastlane adımından sonra
   `run: node --test scripts/release-guard.test.mjs && node scripts/release-guard.mjs`.
5. **`auto-tag.yml`** (+8): fazladan üç `fetch` + tag'den önce guard adımı.
6. **`release.yml` / `android.yml`** (+4 each): ilk işte, checkout'tan sonra
   `release-guard --tag`, `TAG` env ile.
7. **CHANGELOG** `[Unreleased]` (CI/güvenilirlik girdisi) + sürüm belgesi
   (`grep -rln "changelog-section" docs/`): "sürüm PR'ı `## [X.Y.Z]` bölümünü
   ve `en-US/<code>.txt` dosyasını aynı PR'da getirmeli".

## Kabul / doğrulama
- `node scripts/release-guard.mjs` HEAD'de 0; `--tag v7.12.0` 0;
  `--tag v7.12.1` 1.
- Olay tekrarı: `git archive bcffd8a` (CHANGELOG bölümü yok) → 1 ve **iki**
  hata; `git archive f503e88` → 0.
- `changelog-section.mjs 7.12.0` çıktısı refactor öncesi/sonrası diff boş.
- `node --test scripts/release-guard.test.mjs` yeşil; `files:check`,
  `docs:stats:check`, `claims:check` (395/521), `fastlane:check` temiz.
- Rust'a dokunulmuyor → cargo derlemesi yok; yine de tur sonunda `df -h /`.

## Kapsam dışı
Sürüm yükseltme script'inin CHANGELOG bölümünü kendiliğinden üretmesi;
`release.yml`/`android.yml`/`auto-tag` adımlarının PR CI'da denenmesi (tag
yok); sig-v2 dilim B; `/Cap` caption.

## Riskler
- `auto-tag`/`release`/`android` adımları yalnız sürümde koşar: aynı tek satır
  çağrı ve **işin en başı** → kırılırsa yayın sessizce değil, hemen durur
  (force push gerekmez).
- Guard, master'da gerçek bir sürüm uyuşmazlığı bulursa düzeltme `src-tauri`
  gerektirir → o zaman genişletme yapılmaz, rapor edilir.

---

# Seçilen iş kalemi: imza v2'ye kendiliğinden yükselme (dilim B′) — **Opus kararı `88db6078`**

#648'in güvenlik kazancı yalnızca **yeni** eşleşmelere ulaşıyordu; eski çiftler
yeniden eşleşmeden v1'de kalıyordu. Opus `/Cap` caption'ı (yalnız `/Cap true`
+ `/AP`'siz Line'da görünür, değeri dar) ve "v2'yi zorunlu kıl" (güncellenmemiş
cihazlarla eşleşmeyi kırar) seçeneklerine karşı bunu seçti. **A0 doğrulandı:**
#648 ve #649 hiçbir tag'de yok (`v7.12.0` öncesi), yani bu dilim v2 ile aynı
sürümde çıkıyor → kullanıcı v2'yi hiç "eski çift" hâliyle görmez.

## Opus varsayımlarının düzeltilmesi (Adım 0)
- A2 ✔ `sig_v` istekte kendini ilan ediyor; A3 ✔ serde bilinmeyen alanı yok
  sayıyor; A5 ✔ replay penceresi ve v1 golden testleri var.
- A4 ✘ kod adları `bad-signature` / `sig-downgrade` / `range-mismatch`
  (`sig_version_required` yok) → mevcut adlar kullanıldı.
- A1 kısmen: `PairedPeer` ve doğrulama `synccore`'da; mağaza yazımı `src-tauri`'de
  ama #648'den beri pin **bellekte** tutulup uygulama katmanı peer listesini
  mağazaya kopyalıyor → `src-tauri` değişikliği gerekmedi.

## Tasarım (uygulandı)
- **Tel:** `Request.sig_v2` (`#[serde(default, skip_serializing_if = "String::is_empty")]`).
- **Gönderen** (`lan/auth.rs::sign_for`): eş `features`/`min_sig` v2 diyorsa
  yalnız v2; bilmiyorsa `mac`=v1 + `sig_v2`=v2 (çift imza).
- **Alan** (`authenticated`): `sig_v2` doluysa **yalnız** v2 doğrulanır, kabulde
  `pin_min_sig(2)`; boşsa eski kurallar (bilinmeyen sürüm → bad-signature,
  v2 sözleşmiş eşten v1 → sig-downgrade, v2 → TOFU kabul + pin, v1 → kabul).
  Bozuk sidecar yanındaki geçerli v1 onu **kurtarmaz** (kurcalanmış aralık durumu).
- **Yakınsama:** host sidecar'ı kabul edip pinleyince karşıya v2-only yollar →
  karşı taraf da pinler → çift tek turda v2-only olur. Eski build sidecar
  göndermez, v1 ile yanıtlanır. Tek kalan açık: ilk teklifi ağdaki saldırgan
  düşürebilir (yeniden eşleşme güçlü çözüm) — belgelendi.

**PR #650 birleşti** (`ff09b19`), 17 kontrol yeşil + 2 beklenen skip, dal
silindi; `target/` temizlendi (1.2 GiB), disk %60. README Rust testi 1646 →
**1650**. Sürüm değişmedi (v7.12.0).

## Dosyalar / testler
`lan/sig.rs` **229** (`verify_tag` dışa açıldı, `sign_for`/`verify_sidecar`
`auth.rs`'ye taşındı → bütçe 230'da kaldı), `lan/auth.rs` **139** (imzalama +
sidecar kuralı), `lan.rs` **1252** (bütçede), `docs/lan-sync.md` (yükselme
bölümü), CHANGELOG `### Changed`. Testler: `sig_negotiation_tests` 7 → **10**
(kendiliğinden yükselme + pin, çift imzanın v1 yarısının eski alıcıda okunması,
pinli eşten sidecar'sız v1 → sig-downgrade, bozuk sidecar → bad-signature,
eski build v1-only kabul + pin yok, v2-only'de sidecar yok). lib **107 → 111**,
toplam Rust 1646 → **1650**. Kapılar: fmt, clippy `-D warnings`, `cargo test
-p synccore`, `files:check` (bütçe yükseltmesi **yok**), `docs:stats`,
`claims:check` (395/521), `fastlane:check`.

---

# Sürüm treni: v7.13.0 — güvenlik sürümü (imza v2 + kendiliğinden yükselme + release guard)

Üç dilim birikti ve hiçbiri tag'de değil (#648, #649, #650). Kullanıcıya
ulaşması gereken asıl kazanç LAN istek imzası v2 ve eski çiftlerin yükselmesi;
ayrıca yeni `release-guard` ilk kez gerçek bir sürümde çalışacak.

## Adımlar (yapıldı)
- Sürüm **7.13.0**: `package.json`, `package-lock.json`, `Cargo.toml`
  `[workspace.package]`, `Cargo.lock` (7 workspace üyesi),
  `src-tauri/tauri.conf.json`, `src-tauri/gen/android/app/tauri.properties`
  (`versionName=7.13.0`, `versionCode=**7013000**`).
- CHANGELOG: `[Unreleased]` boşaltıldı, `## [7.13.0] - 2026-11-07` açıldı;
  **`### Security` başta** (store metni ilk girdiden üretiliyor), sonra
  `### Changed` (yükselme + release guard). #648'in "yeniden eşleşene kadar v1"
  cümlesi #650 yüzünden yanlış olduğu için metinden çıkarıldı.
- README: sürüm satırı, `What's new (6.2 to 7.13.0)`, 7.13.0 paragrafı,
  **indirme bağlantıları** — eskiden `v7.7.0` tag'ini gösteriyordu (kırık
  linkler) → `v7.13.0` + 7.13.0 dosya adları; `docs:stats` blok tazelendi.
- RELEASE_READINESS başlığı + döngü başlığı.
- fastlane: `en-US/7013000.txt` changelog'dan üretildi (497 bayt < 500),
  `tr-TR/7013000.txt` elle yazıldı (340 bayt).
- **Guard kendi işini yaptı:** dosyalar yokken `release-guard` 7.13.0'ı
  `fastlane/.../7013000.txt: file is missing` ile reddetti; dosyalar eklenince
  `release-guard: 7.13.0 (versionCode 7013000) OK`.
- Kapılar: `release-guard`, `fastlane:check` (26 changelog), `docs:stats:check`,
  `claims:check` (395/521, döngü 7.13.0), `files:check` (bütçe değişmedi).

**PR #651 birleşti** (`e690063`), 17 kontrol yeşil + 2 beklenen skip. Auto-tag
`v7.13.0`'ı Desktop yeşile dönünce atacak (guard tag'den önce koşuyor), ardından
Release + Android. Doğrulanacak: 18 asset, `latest.json` 7.13.0 + imza,
`SHA256SUMS.txt` 8 satır, release gövdesi `[7.13.0]` bölümüyle birebir,
versionCode 7013000.

---

# Sıradaki dilim adayı: sig-v2 kurcalama matrisi — **Opus kararı `c853876e` (koşullu dönüş)**

Opus sıradaki kalem olarak `/Cap` caption'ı önerdi, **ama** şu koşulla: "kanonik
v2 alanlarının her biri için uçtan uca kurcalama testi (offset/length/total/
sha256/transfer_id/receiver → `bad-signature`) ve bozuk `sig_v2` / `min_sig=2`
düşürme yolu testleri eksikse ve kod `synccore`'da ise, o zaman C3'e (test
sıkılaştırma) dön."

**Ölçüm (Q14):** eksik. Bugün `sig_tests.rs` alan bağlamasını **birim**
düzeyinde kanıtlıyor (`version_two_binds_*` mutasyonları), ama *uçtan uca*
(matris + host'un `bad-signature` yanıtı) yok; `download_tests` yalnız
sha256/total yolunu, `sig_negotiation_tests` yalnız bozuk sidecar'ı kapsıyor.
Kod `crates/synccore` içinde → **C3 yapılacak**; `/Cap` sonraya kalır.

## Plan (dilim: yalnız test + belge, davranış değişmez)
- Yeni `crates/synccore/tests/sig_v2_tamper.rs` (yeni dosya, <400 satır):
  - tablo güdümlü matris: imzalandıktan sonra **her alan** değiştirildiğinde
    (offset, `body_len`/length, total, sha256, path/transfer_id, op, sender,
    receiver, nonce, timestamp) host `bad-signature` döndürmeli ve istek
    işlenmemeli (dosya yazılmamalı/okunmamalı);
  - bozuk `sig_v2` biçimleri: boş olmayan çöp, tek karakter eksik, `sig_v2`
    varken `sig_v=2` (iki şekil aynı anda), sidecar'ı başka eşin anahtarıyla
    imzalanmış;
  - sidecar'sız v1 + pinli eş → `sig-downgrade`; v1 → v2 yükselmesi sonrası
    aynı istek reddedilmeli;
  - receiver bağı: X için imzalanmış v2, Y'ye gönderilince reddedilmeli;
  - replay: aynı nonce iki kez reddedilmeli (v2 ve sidecar yollarında).
- `docs/lan-sync.md`'ye tek paragraf: "hangi alanlar bağlı, kurcalanırsa ne
  olur" tablosu (testlerin adlarıyla).
- CHANGELOG `[Unreleased] ### Security`: "isteğe bağlı: kurcalama matrisi ile
  doğrulanıyor" cümlesi **yok** — kullanıcıya görünen bir değişiklik olmadığı
  için CHANGELOG'a yazılmaz; yalnız plan + doküman güncellenir.
- Kapılar: `cargo fmt`, `cargo clippy -p synccore --all-targets -D warnings`,
  `cargo test -p synccore`, `files:check` (yeni dosya <400 → bütçe yok),
  `docs:stats`, `claims:check`.

**DURUM (uygulandı):** yeni `crates/synccore/src/lan/sig_tamper_tests.rs` **221**
satır, 7 test: alan matrisi (offset/total/sha256/op/path), kimlik+nonce+tazelik
(receiver bağı ayrı cihaz kimliğiyle imzalanan etiketle), replay ("already
used"), bozuk sidecar biçimleri (hex değil/kısa/uzun/sıfır/boşluklu) →
`bad-signature`, başka anahtarla imzalanmış sidecar → `bad-signature`, pinli
eşten v1 → `sig-downgrade` + kurcalanmış v2 → `bad-signature`, ve reddedilen
isteğin hiçbir şeyi değiştirmediği (bozuk sidecar pin yazmaz, geçerli olan
yazar). lib **111 → 118**, toplam Rust **1650 → 1657**. `lan.rs` bütçede
kaldı (1254 → **1252**, başlık yorumları sıkıştırıldı), yeni dosya <400.
Kapılar: fmt, clippy `-D warnings`, `cargo test -p synccore`, `files:check`,
`docs:stats`, `claims:check` (395/521), `release-guard` (7.13.0 OK).

**PR #653 birleşti** (`8ffebca`), 20 kontrol (18 yeşil + 2 beklenen skip; pdfcore
değiştiği için `Fuzz changed parsers` de koştu ve yeşil), dal silindi.
README Rust testi 1657 → **1678**. Sürüm 7.14.0 treni açıldı.

**PR #652 birleşti** (`cf90af3`), 17 kontrol yeşil + 2 beklenen skip. İlk CI
turunda yalnız `rustfmt` düştü (modül bildirimlerinin sırası) → `cargo fmt` +
tek satırlık düzeltme commit'i.

---

# SÜRÜM: v7.13.0 yayınlandı ve doğrulandı (2026-11-07)

- Tag **`v7.13.0` → `e690063`**; `Release` + `Android` **başarılı**, **18 asset**.
- `latest.json` **7.13.0**, `windows-x86_64` platformunda **444 baytlık imza** ve
  imzalı `OmniOffice_7.13.0_x64-setup.exe` bağlantısı; `SHA256SUMS.txt` **8 satır**;
  `release body` == CHANGELOG `[7.13.0]` bölümü (2648 = 2648 karakter).
- Android versionCode **7013000**; `release-guard` ilk kez gerçek sürümde koştu
  (auto-tag tag'den önce) ve PR aşamasında eksik fastlane dosyasını yakaladı.
- README indirme bağlantıları düzeltildi: `v7.7.0` tag'ini gösteriyorlardı
  (7.12.0 assetleri için kırıktı) → artık `v7.13.0` + 7.13.0 dosya adları.
- README Rust testi 1650 → **1657**; `target/` temizlendi, disk %60.

## Sıradaki adaylar
1. **`/Cap` caption** (Opus `c853876e`'nin ilk önerisi; sig-v2 test koşulu
   artık karşılandı) — yalnız `/Cap true` + `/AP`'siz Line'da, Helvetica/WinAnsi.
2. Sig-v2 **dilim B**: `protocol_version` artışı yerine isteğe bağlı zorunlu v2
   (birkaç sürüm sonra, UI "yeniden eşleşin" mesajıyla).
3. Sürüm sonrası bakım: Scorecard/Renovate düşük kontrolleri.

---

# Seçilen iş kalemi: Line `/Cap` caption'ı N-up sentezinde — **Opus `c853876e` §2/§4**

## Adım 0 (ölçüldü)
- `/Cap`, `/CP`, `/CO` `crates/pdfcore/src/**` içinde **hiç işlenmiyor** ✔.
- Line sentezleyicisi: `crates/pdfcore/src/nup/markup/appearance/paths.rs`
  `line()`; `/LL` sonrası çizilen uç noktalar `Leaders { start, end, legs }`.
- `Synth { ops, bbox, multiply, font }`; metin fontu `FontSpec::Standard`
  (`install.rs` `/FOmni0` kaynağını yazıyor), hazır altyapı:
  `std14.rs` (Helvetica AFM + WinAnsi + Türkçe `/Differences`, `ascent()`),
  `textfit::encode(&str) -> Option<Vec<u8>>` (kodlanamayan karakter → `None`),
  `textfit::width_of`, `number_text`, `union`, `padded`, `Color::fill_op`.
- `/Contents` çözücü hazır: `docutil::pdf_text_value` (UTF-16BE/LE + UTF-8).
- `paths.rs` bütçesi **327**; skill "tavanı yükseltme, alt modüle taşı" diyor →
  yeni `appearance/caption.rs`, `paths.rs`'te yalnız ~9 satır bağlama ve
  karşılığında yorum sıkıştırma.

## Tasarım
- **Yalnız** `/Cap` `true` **ve** `/AP` yokken (bu fonksiyon zaten yalnız o
  durumda çağrılıyor) caption çizilir; diğer her durumda çıktı **bayt bayt
  aynı** (golden test).
- Metin: `/Contents` → düz tek satır (kontrol karakterleri → boşluk, boşluklar
  tek); `textfit::encode` `None` dönerse **caption çizilmez** (skill: yerine
  `?` basma) — Türkçe harfler `/Differences` ile kodlanabildiği için kaybolmaz.
- Font: Helvetica (standart-14), boyut **9 pt** (Line'da `/DA` yok, spec boyut
  vermiyor → sabit, belgelenir); renk: çizginin `/C` dolgu rengi.
- `/CP`: `Top` ise çizginin üstünde (baseline = çizgi + `stroke/2 + descent +
  1pt`), değilse `Inline`: metin çizgiye dikey ortalı, çizgi metin genişliği +
  `2·1pt` kadar **kesilir** (iki segment); metin çizgiden uzunsa kesme yok.
- `/CO` `[h v]`: orta noktadan itibaren `u·h + n·v`; bozuk/eksik → `(0,0)`,
  `±14400`'e kırpılır.
- `/BBox`: caption kuadı (baseline ± ascent/descent) çizgi/leader/uç kutusuyla
  birleştirilir → hiçbir şey kırpılmaz.
- Sıfır uzunluklu çizgi veya `|/CO|` taşması → caption yok.

## Testler (caption.rs içinde)
golden "caption yok → aynı baytlar" · `/Cap false` ve `1` (bool değil) · `/CP`
bilinmeyen → Inline · `/CO` bozuk biçimler → (0,0) ve kırpma · `/Contents` boş →
None · kodlanamayan karakter → None · WinAnsi/Türkçe kodlama · genişlik
("Hello" = 20.502) · Inline kesme aralığı · Top baseline · diyagonal `Tm` ·
`/CO` kaydırması · çizgiden uzun metin → kesme yok · sıfır uzunluk → None ·
BBox kuadı kapsıyor · `/AP` varsa dokunulmaz (mevcut yol) · 2-up entegrasyon.

**PR #654 birleşti** (`ab45f4f`), 17 kontrol yeşil + 2 beklenen skip.

## SÜRÜM: v7.14.0 yayınlandı ve doğrulandı (2026-11-07)
- Tag **`v7.14.0` → `ab45f4f`** (auto-tag, guard tag'den **önce** koştu);
  `Release` + `Android` başarılı, **18 asset**.
- `latest.json` **7.14.0**, `windows-x86_64` girdisinde **444 baytlık imza**;
  `SHA256SUMS.txt` **8 satır**; release gövdesi CHANGELOG `[7.14.0]` bölümüyle
  **birebir** (705 = 705 karakter); Android versionCode **7014000**.
- `target/` temizlendi (12.6 GiB), disk %60.
- **Not:** `Scorecard` işi bu iki master commit'inde kırmızı; sebep bizim kod
  değil — `rekor.sigstore.dev` 500 döndü ("error signing scorecard results"),
  analiz tamamlanmış (skor 6.5). Dış servis kesintisi; tekrar denemek yeterli.
  Skorcard'ın işaret ettiği gerçek kalemler (sonraki adaylar): 21 OSV uyarısı
  (bağımlılık güncellemesi), "releases lack provenance" (attestation asset'i
  yalnız bir dosya için), Code-Review 0/30 (insan onayı yok — yapısal).

---

# Sıradaki dilim: Type0/CID metin — **Opus kararı `f7f8ef5c`** + Adım 0 düzeltmesi

Opus, N-up sentezinde WinAnsi dışı metnin (Türkçe ğ/ı/ş, CJK) **tamamen
kaybolduğunu** gerekçesiyle Type0/CID metni seçti; `/BE` bulut kenarlık (kozmetik
fallback), `/NoZoom` (çok küçük), redaksiyon düzleştirme (tek dilimde güvenli
değil) elendi.

## Adım 0 (ölçüldü) — planı değiştiren bulgu
- **ToUnicode okuyucusu yok** (`ToUnicode`/`bfrange` yalnız testlerde) → Opus'un
  önerdiği "belgenin Type0 fontunu `/ToUnicode`'dan ters çevir, `/W` ile
  ölç, referansla" yolu ~700 satır yeni ayrıştırıcı demek.
- **Ama yazıcı zaten var:** `crates/pdfcore/src/textfont.rs` (**418**)
  `TextFont::create(&mut Document, &BTreeSet<char>)` — gömülü Liberation Sans'ı
  Identity-H Type0 + `/ToUnicode` olarak **alt kümeleyip** yazıyor, Türkçe
  repertoarın tamamı var; `encode()` hex üretiyor, `measure(text, size)` var,
  `reference()` ve `ensure_page_font_named()` ile kaynak sözlüğüne giriyor.
  (Başlık/altbilgi, Bates, form görünümleri bunu kullanıyor.)
- Yani doğru dilim: **belgenin fontunu okumak değil, WinAnsi'ye sığmayan metni
  kendi alt küme fontumuzla basmak** — mevcut ve test edilmiş kod yeniden
  kullanılır, CMap ayrıştırıcısı gerekmez. Türkçe kazanç tam; CJK yalnız
  Liberation Sans'ta varsa (yok → metin yine atlanır, kural korunur).

## Tasarım taslağı (uygulanacak)
- `Synth`/`FontSpec`'e üçüncü hal: `FontSpec::Embedded { key, id }` (zaten
  `Resource{key,id}` var; ayrım yalnız belgeleme) veya `TextFont` referansı.
- **İki faz sorunu:** `carry_markup` önce **okur** (`&Document`), sonra yazar;
  `TextFont::create` ise `&mut Document` istiyor. Çözüm: `Draw`'a ertelenmiş
  metin (`DeferredText { chars, template }`) ekle; `install` (yazma fazı) fontu
  bir kez oluşturup (iş başına `BTreeMap<BTreeSet<char>, ObjectId>` önbelleği)
  hex kodları yerleştirsin.
- Uygulama sırası: (1) caption (en küçük, satır kaydırma yok), (2) FreeText
  (sarma/auto-size için `FontMetrics` impl'i: `advances[glyph]·size/upem`).
- Türkçe testi: caption/FreeText `Ölçüm ğ ş ı İ` → `BT … <…> Tj` çıkmalı ve
  `/Resources /Font <key>` alt küme fontuna referans olmalı; WinAnsi yolu
  **bayt bayt aynı** kalmalı (golden).
- Kapılar: fmt, clippy `-D warnings`, `cargo test -p pdfcore`, files:check
  (yeni modül ≤400, `textfont.rs` bütçede), docs:stats, claims:check,
  release-guard.

## Adım 0-bis (dosya/bütçe ölçümü — uygulama için kesinleşti)
- Bütçeler: `textfont.rs` **418** (tavan), `nup/markup.rs` **269** (tavan),
  `nup/markup/install.rs` **169** (izlenmiyor → büyüyebilir),
  `appearance/caption.rs` **399** (yeni dosya tavanı 400).
- **Seam:** `textfont.rs` içindeki `parse_cmap`/`parse_metrics`/`write_font`/
  `FontMetrics` yeni **`crates/pdfcore/src/textfont/subset.rs`**'ye taşınır
  (~60 satır çıkar → `textfont.rs` küçülür, `mod subset;` + `pub use` eklenir);
  orada **`SubsetFace`** doğar: `bundled()`, `can_encode(&str)`, `encode(&str)
  -> String` (`<hex>`, 2 bayt glyph id), `measure(&str, size)`, ve
  `write(doc, &BTreeSet<char>) -> PdfResult<ObjectId>` (alt küme fontu).
  `TextFont` mevcut genel API'sini korur, içini `SubsetFace`'e devreder
  (başlık/altbilgi, Bates, form görünümleri etkilenmez).
- **Oluşturma fazı:** `install::synthesized_form(doc, synth)` (imza `doc` alır;
  `install.rs` izlenmiyor) — `FontSpec::Subset { chars: BTreeSet<char> }`
  görürse sayfa başına `BTreeMap<BTreeSet<char>, ObjectId>` önbelleğiyle
  fontu **bir kez** yazar, formun `/Resources /Font`'una `/OmniTr <ref>`
  koyar. Kodsuz metin (WinAnsi) yolu değişmez.
- **Okuma fazı:** `SubsetFace::bundled()` saf (doc istemez) → caption/FreeText
  ölçü ve kodlamayı okuma fazında yapar; `FontSpec::Subset` yalnız "bu metin
  alt küme fontuyla yazıldı" işaretidir, deterministik (aynı karakter kümesi →
  aynı font).
- **Bu PR'ın kapsamı:** mekanizma + **caption** (`/Cap`), FreeText sonraki PR
  (skill: bir PR'da tek mekanizma). `caption.rs` 399'da olduğu için testler
  `appearance/caption_tests.rs`'ye taşınır (`read`/`offset_of` `pub(super)`).

**PR #655 birleşti** (`d0640d8`), 20 kontrol (18 yeşil + 2 beklenen skip; pdfcore
değiştiği için `Fuzz changed parsers` de koştu ve yeşil). README Rust testi
1678 → **1686**. `textfont.rs` 418 → **87**, yeni `textfont/subset.rs` **386**,
`caption.rs` **241** + `caption_tests.rs` **230**. Sürüm **7.15.0** treni açıldı
(yalnız caption kapsanıyor — FreeText sonraki dilim, metin buna göre yazıldı).

## Sıradaki dilim (plan): FreeText'i alt küme yüzüne geçirmek
- `freetext.rs` (393, bütçe tavanı) + `textfit.rs` (335). Engel: `FontMetrics`
  trait'i `encode(&self, char) -> Option<u8>` — tek bayt; alt küme glyph id'leri
  **u16**. İki yol: (a) trait'i `Vec<u8>`/`String` döndürecek şekilde genişlet
  (drfont + std14metrics + wrap_with + freetext dokunur, en temiz), (b) yerleşimi
  Helvetica metrikleriyle yap (Liberation Sans Helvetica ile metrik-uyumlu) ve
  yalnız **emisyonda** alt küme fontunu kullan (daha küçük, ama auto-size
  genişliği yaklaşık). Karar Opus'a sorulacak; bütçe nedeniyle (a) seçilirse
  `textfit.rs`'teki trait kodu ayrı bir `appearance/metrics.rs`'ye taşınmalı.

---

# SÜRÜM: v7.15.0 yayınlandı ve doğrulandı (2026-11-07)
- Tag **`v7.15.0` → `233884d`** (auto-tag, guard tag'den önce); `Release` +
  `Android` başarılı, **18 asset**.
- `latest.json` **7.15.0**, `windows-x86_64` girdisinde **444 baytlık imza**;
  `SHA256SUMS.txt` **8 satır**; release gövdesi CHANGELOG `[7.15.0]` ile
  **birebir** (763 = 763 karakter); Android versionCode **7015000**.
- İçerik: standart fontun kodlayamadığı **caption** metni (Kiril/Yunan/geniş
  Latin) alt küme yüzüyle basılıyor.

**PR #657 birleşti** (`f4e1ba1`), 20 kontrol (18 yeşil + 2 beklenen skip;
pdfcore değiştiği için `Fuzz changed parsers` de yeşil). FreeText notları da
artık WinAnsi'nin taşıyamadığı metinde gömülü yüzün **slot eşlemeli** alt
kümesiyle basılıyor; `textfit.rs`/`drfont.rs` değişmedi, `freetext.rs`
393 → **353**, yeni `face.rs` 238. lib **261 → 266**, tam suite 662/7 (7 =
libpdfium yok). `nup_freetext_dr.rs` bütçesi **+3** (UTF-16 fixture, bilinçli).
README Rust testi 1686 → **1699**. `target/` temizlendi (23.5 GiB), disk %61.

## Sıradaki adaylar
1. **v7.16.0 sürüm treni** (FreeText alt küme; tek kullanıcı-görünür değişiklik).
2. Opus'a yeni kalem: `/BE` bulut kenarlıklar, `/NoZoom`, belgenin Type0 fontunu
   okumak (Identity-V/predefined CMap), redaksiyon düzleştirme, OSV bağımlılık
   güncellemeleri (Scorecard "21 vulnerabilities"), provenance attestation.

---

# GÜVENLİK BULGUSU (sıradaki dilim adayı): calamine CFB/OLE2 DoS — RUSTSEC-2026-0436

Scorecard'ın "21 OSV" listesindeki en ciddi kalem: `calamine 0.37.0`
(`crates/officecore`) **CFB/OLE2 ayrıştırıcısında** (`src/cfb.rs`, `.xls`,
`.xlsb` ve VBA çıkarma yolu) hazırlanmış girdiyle **panic veya sınırsız
allocation** yapıyor; public reader API'sinden (`open_workbook`, `Xls::new`,
`Xlsb::new`) erişilebilir → kullanıcının açtığı dosyayla uygulama çökebilir.
Düzeltme: **0.38.0**. DoS, bellek bozulması yok.

**Engel:** repo `vendor/calamine` altında 0.37'yi **yerel iki yamayla** tutuyor
(`[patch.crates-io]`): (1) `cell_ref_to_row_col` taşma (`checked_mul/add`),
(2) `get_dimension` ters `ref` alt taşması (`saturating_*`). Bunlar kendi
fuzz'ımızın bulduğu düzeltmeler; regresyon testleri
`crates/officecore/tests/calamine_dimension_test.rs` + `fuzz/corpus/xlsx/`.

Düz requirement'ı 0.38'e çıkarmak vendor yamasını **düşürür**
(`[[patch.unused]]` — ölçüldü), yani iki yerel düzeltme kaybolur. Doğru iş:
**0.38'i vendor'la ve iki yamayı üzerine yeniden uygula** (upstream 0.38'de
zaten yoksa), `vendor/calamine/README.md`'yi güncelle, fuzz regresyonlarını
koştur. Ayrıntılı plan Opus'a sorulacak.

**PR #658 birleşti** (`146d268`), 20 kontrol (18 yeşil + 2 beklenen skip;
`Dependency audit`, `MSRV` ve `Fuzz changed parsers` de yeşil — calamine 0.38
MSRV 1.88 ✔). officecore **440 test** + yeni `calamine_hostile_input` 4/4.
README Rust testi 1699 → **1703**. Sürüm **7.16.0** treni açıldı (içerik:
FreeText alt küme metni + RUSTSEC-2026-0436 düzeltmesi).

---

# SÜRÜM: v7.16.0 yayınlandı ve doğrulandı (2026-11-07)
- Tag **`v7.16.0` → `7309b82`** (auto-tag, guard tag'den önce); `Release` +
  `Android` başarılı, **18 asset**. `Scorecard` bu kez **yeşil** (Rekor kesintisi
  geçti).
- `latest.json` **7.16.0**, `windows-x86_64` girdisinde **444 baytlık imza**;
  `SHA256SUMS.txt` **8 satır**; release gövdesi CHANGELOG `[7.16.0]` ile
  **birebir** (1434 = 1434); Android versionCode **7016000**.
- İçerik: RUSTSEC-2026-0436 düzeltmesi (crafted `.xls`/`.xlsb` DoS) + FreeText
  notlarının alt küme metni.

## Sıradaki dilim: SLSA provenance — **Opus planı `34742bfd` (c)**
Yeni `scripts/provenance-subjects.mjs` (`hash`/`encode`/`check-published`/
`coverage` alt komutları) + `scripts/provenance-subjects.test.mjs` (node:test,
~19 vaka), `release.yml`'e hash toplama + `generator_generic_slsa3` job'ı
(`*.intoto.jsonl` asset'i), `docs/release.md` + `SECURITY.md` doğrulama
bölümü, `latest.json` hariç tutulur (updater imzasıyla korunuyor). Asset sayısı
18 → 19 olur; `docs-stats`/`files:check` etkisi ölçülecek.

**DURUM (SLSA provenance, uygulandı):** `scripts/provenance-subjects.mjs`
**~250** (4 alt komut: `hash --paths-json|--dir`, `encode`, `check-published`,
`coverage`; 0/1/2 çıkış kodları; Windows yolları için `\\` ayracı), testleri
`scripts/provenance-subjects.test.mjs` **~300** (20 vaka: hash/hash-dir,
birleştirme+sıralama+tekilleştirme, CRLF/`*`/büyük hex/yol öneki, `file:line`
hatası, aynı ada iki digest, `latest.json`+`*.intoto.jsonl` dışlama, round-trip,
check-published pass/tamper/extra/bad-input, coverage v0.1+v1/kapsanmayan/bayat/
bozuk zarf). `release.yml`: `provenance` job'ı (windows+linux artefaktlarını
indirir, `hash --dir built` → `encode` → job output `subjects`; `gh release
download` + `check-published` ile **yayınlanan baytlarla** karşılaştırır) ve
`provenance-asset` job'ı (`slsa-framework/slsa-github-generator/.github/
workflows/generator_generic_slsa3.yml@v2.1.0`, `upload-assets: true`,
`provenance-name: omnioffice-<tag>-desktop.intoto.jsonl`; generator **tag** ile
referanslanır çünkü builder SHA ref'i reddediyor). `/tmp/relcheck` ile gerçek
v7.16.0 asset'leri üzerinde kuru çalıştırma: 3 subject (4 dosya − `latest.json`)
ve `check-published` **eşleşti**. Docs: `SECURITY.md` "Verifying a download"
(`slsa-verifier` + `gh attestation verify`), `docs/distribution.md` kural
tablosuna satır, CHANGELOG `### Security`. **Android** için aynı job ayrı dilim
(kendi `*.intoto.jsonl`'i); asset sayısı 18 → **19** (desktop zarfı), Android
zarfıyla 20.

**PR #660 birleşti** (`f3e2cf5`), 19 kontrol yeşil. **İlk CI turunda
`Frontend (tests, type check, build)` düştü:** `reproducibility.test.ts` her
`uses:` için **commit SHA** istiyor, SLSA generator'ı ise kendi dokümanı gereği
**tag** ile çağrılmalıydı. Çözüm: `check-reproducibility-inputs.mjs`'e tam ref
adını veren bir allow-list (`PINNED_BY_TAG`) + `docs/reproducibility.md`'de
"Action pinning, and the one exception" bölümü. İkinci turda hepsi yeşil.

## Sıradaki: v7.17.0 (provenance'ın ilk gerçek testi) — ama önce bir kullanıcı dilimi
Provenance job'ları **yalnız tag'de** koşuyor; Opus'un kuralı: sürüm yalnız build
meta verisi içeriyorsa mağaza sürümü çıkarma. Bu yüzden **önce `/BE` bulut
kenarlıklar** (küçük, kullanıcı-görünür, pdfcore içinde, yerelde doğrulanabilir),
sonra v7.17.0: içinde hem `/BE` hem provenance olur → tag'de
`omnioffice-v7.17.0-desktop.intoto.jsonl` (18 → **19** asset) ve
`slsa-verifier`/`coverage` ile doğrulama.

**`/BE` bulut kenarlık ertelendi (bütçe ölçümü):** `/BE` `border.rs` (+~8),
`bevel.rs` (+1 test literal) ve `appearance.rs` (+3 bağlama) dosyalarına dokunuyor;
üçü de **tam tavanında** (239/240/282). Skill kuralı "tavanı yükseltme, alt modüle
taşı" → doğru iş önce stroke ayrıştırmasını `border/` alt modülüne taşımak; bu,
Opus'un "değeri düşük" dediği bir özellik için orantısız. Bu yüzden sıradaki adım
**v7.17.0**: içeriği SLSA provenance asset'i (`omnioffice-v7.17.0-desktop.intoto.jsonl`,
18 → **19** asset) — kullanıcı için gerçek karşılığı "indirdiğini doğrulayabilme" ve
tag-only job'ların ilk gerçek testi. `/BE` daha sonra, stroke ayrıştırması kendi
modülüne taşındıktan sonra yapılacak.

---

# SÜRÜM: v7.17.0 yayınlandı ve doğrulandı (2026-11-07) — provenance'ın ilk gerçek testi
- Tag **`v7.17.0` → `9277ae1`**, **19 asset** (`omnioffice-v7.17.0-desktop.intoto.jsonl` dahil);
  provenance job'larının **hepsi yeşil** (`Build provenance (desktop assets)` +
  `Publish the provenance envelope / detect-env, generator, upload-assets, final`).
- Release gövdesi CHANGELOG `[7.17.0]` ile **birebir** (802/802); `latest.json`
  7.17.0 + **444 baytlık imza**; `SHA256SUMS.txt` **8 satır**; versionCode 7017000.
- Zarf: `_type` `Statement/v0.1`, `predicateType` `slsa.dev/provenance/v0.2`,
  **12 subject** — tüm masaüstü asset'leri (`latest.json` tasarım gereği dışta).

**Tag'in bulduğu gerçek hata (PR #662 → `68c4197`):** SLSA generator zarfı
**Sigstore bundle** olarak yazıyor (`verificationMaterial` + `dsseEnvelope`),
`coverage` ise çıplak DSSE zarfı bekliyordu → yayınlanan provenance **kontrol
edilemiyordu**. Düzeltme: `coverage` üç biçimi de okuyor (bundle / çıplak DSSE /
statement), 21 test. Gerçek zarfla doğrulandı.

**PR #663 birleşti** (`d7dd8f6`) — Android asset'leri de kendi zarfını alıyor
(`omnioffice-<tag>-android.intoto.jsonl`); `check-published` yalnız kendi
yayınladığı dosyalarla karşılaştırıyor. v7.18.0'da sürüm **20 asset** olmalı ve
iki zarf birlikte verildiğinde `coverage` **0** dönmeli.

## Sıradaki dilim: `/BE` bulut kenarlıklar — bütçeyi içeriden ödeyerek
Üç dosya da tam tavanında olduğu için dilim iki işi birlikte yapar:
1. **Ödeme (saf taşıma, davranış aynı):** `appearance.rs::border`/`shape`
   içindeki stroke/`/BE` ayrıştırmasını yeni `appearance/frame.rs`'e taşı;
   `border.rs` ve `bevel.rs` yalnız kendi işini yapsın. `cargo test` bu adımda
   **hiçbir golden değişmeden** yeşil olmalı.
2. **Özellik:** `/BE << /S /C /I >>` — Square/Circle çevresinde dilim (scallop)
   yayları (kübik Bézier), `/I` 0..2'ye kırpılır, `/S` yoksa veya `/S /S` ise
   **bayt-aynı**; dejenere rect / 2000+ dilim / sonsuz sayı → düz kenarlık;
   dolgu `/IC` aynı yoldan. Testler: yoklukta bayt-aynı, `/S /S` aynı, kapalı
   yol ve `/Rect` içinde kalma, dilim sayısı, `/I` kırpma, dejenere, tavan,
   `/IC` dolgusu.
3. Docs `docs/roadmap.md` + CHANGELOG `[Unreleased] Added`; kapılar + PR.

---

# SÜRÜM: v7.18.0 yayınlandı ve doğrulandı (2026-11-07) — iki zarf, tam kapsama
- Tag **`v7.18.0` → `9cc3115`**, **20 asset** (`-desktop` + `-android` zarfları).
  Provenance job'ları **iki workflow'da da** yeşil (Release: `Build provenance
  (desktop assets)` + 4 generator adımı; Android: `Build provenance (Android
  assets)` + 4 adım).
- **Zarf içerikleri (ölçüldü):** desktop **12 subject**, android **5 subject**;
  `Statement/v0.1` + `slsa.dev/provenance/v0.2`. Birlikte **17 = 20 −
  `latest.json` − 2 zarf**, ve subject listesi yayınlanan asset'lerle **birebir**
  (`missing: none`, `named but not published: none`).
- **Tüketici doğrulaması:** `node scripts/provenance-subjects.mjs coverage
  --release-dir /tmp/rel18 omnioffice-v7.18.0-desktop.intoto.jsonl
  omnioffice-v7.18.0-android.intoto.jsonl` → *"every published asset is covered
  by 2 envelope(s)"*, exit 0.
- Release gövdesi CHANGELOG `[7.18.0]` ile **birebir** (576/576); `latest.json`
  7.18.0 + **444 baytlık imza**; `SHA256SUMS.txt` **8 satır**; versionCode
  **7018000**.
- `target/` temizlendi (12.8 GiB), disk %61.

## Sıradaki adaylar
1. **Android zarfı için de `check-published`** zaten var; kalan iş yok.
2. Belgenin Type0/CID fontunu **okumak** (Identity-V, predefined CMaps, gömülü
   CMap) — Opus'un 3-4 PR'lık serisi; sentez metninde CJK için son boşluk.
3. `/NoZoom` (opt-in) ve redaksiyon düzleştirme (içerik kaldırmadan yapılmaz).
4. OSV kalan uyarıları (çoğu unmaintained/GTK3) — gerekçeli `ignore` + izleme.

---

# PR #666: `/ToUnicode` okuyucusu (kompozit font serisinin ilk mekanizması)
- `crates/pdfcore/src/cmap.rs` (311) + `cmap/tests.rs` (145, 14 test).
  `cmap::read(&[u8]) -> ToUnicode`; `code(char) -> Option<u16>` (en küçük kod
  kazanır), `covers(&str)`, `encode(&str) -> Option<Vec<u8>>` (iki bayt BE).
  Güvensiz girdi: bozuk giriş yalnız kendini kaybeder, akış ve harita
  sınırlanır (1 MiB / 65536), hedef tek kullanılabilir skaler değilse atlanır.
- **Fuzz gerçek bir hata buldu** (ilk koşuda, 7 dk'da): `increment`
  içinde, `bfrange` hedefi **sekiz baytın tamamını** kapladığında `u64`
  kendi genişliği kadar sağa kaydırılıyordu → `attempt to shift right with
  overflow`. Düzeltme: genişlik > 8 önce reddedilir, tam genişlikte uyum
  kaydırması hiç yapılmaz. Regresyon testi 1..=8 bayt genişliklerini tarar ve
  **hatalı kodla düştüğü doğrulandı** (`a_destination_of_any_width...`).
- Doğrulama: lib **286** test, clippy `-D warnings` temiz, `files:check`
  (yeni dosya 311 ≤ 400), `docs:stats`, `claims:check`, `release-guard`,
  `reproducibility`, YAML parse.
- **Opus planı `edaa33b8`** — uyum: birebir ("önce bağımsız ayrıştırıcı"
  adımı; `pdfcore`'da ToUnicode ayrıştırıcısı yoktu, ölçüldü).

---

# SERİ PR 1b: `DocCidFace` — belgenin kendi kompozit fontuyla çizmek
**Opus planı `fb7ca4c9`** (Opus 5.5, high effort). Ölçümle doğrulanan bilinmeyen:
paketli yüz CJK göstermiyor (`face.rs` testi: "no CJK glyphs in the bundled face")
→ **Branch A**: sıra WinAnsi → paketli yüz → belge CID → atla.

## Kararlar (Opus)
1. **Sıra:** bugün çizen her şey bayt-aynı kalır; `/ToUnicode` ters eşlemesiyle çizmek
   ancak daha güvenli hiçbir yol kalmadığında denenir.
2. **Slot katmanı:** tek `enum GlyphSource { Bundled(..), Document(DocCid) }`, `SlotMetrics`
   içinde; `Face`'e yeni varyant yok → tavanı dolu `freetext.rs`/`caption.rs`'e yeni
   match kolu girmiyor. Slot ataması ortak yardımcıda (boşluk 0x20'de sabit).
3. **Reddetme sırası:** yapısal kontroller önce, `/ToUnicode` çözümü en sonda. Bozuk `/W`
   **fontun tamamını** reddeder (tek girişi düşürmek o CID'ye sessizce `/DW` verirdi = tahmin).
4. **`/W`:** sıralı `Vec<Run{first,last,width}>`, ≤65 536 öğe, ikili arama, örtüşme reddi;
   bellek girdi boyutuyla büyür (HashMap `0 65535 w` gibi aralığı 65 536 girdiye açardı).
5. **Caption 1b'de yok** (Line `/Cap` annotasyonunda `/DA` yok → "belge fontu" kavramı yok;
   `caption.rs` tavanında); gerekirse 1c.
6. **`/CIDToGIDMap`** yalnız Type2 için: yok veya `/Identity` → kabul; akış → 128 KiB sınırıyla
   çözülür, GID 0 olan kod kullanılamaz; başka değer/başarısız çözüm/aşırı akış → reddet.
7. **`/CIDSystemInfo` bilinçli yok sayılır** (Identity-H + gömülü program varken yalnız
   ikame için önemli, biz ikame etmiyoruz).

## Reddetme listesi (R1–R16, sırayla)
`/DR`'de yok veya doğrudan sözlük (nesne kimliği yok → kopyalamak gerekirdi) · `/Subtype`
≠ Type0 · `/Encoding` yok · `/Identity-V` · başka ad (predefined CMap: kodu CID'e çevirecek
veri yok, kod 1–4 bayt) · `/Encoding` akış (gömülü CMap) · `/DescendantFonts` yok/boş/çok ·
öğe sözlük değil · alt font `/Subtype` ≠ CIDFontType0/2 · `/CIDToGIDMap` geçersiz ·
`/FontDescriptor` yok · `/FontFile{,2,3}` yok (gömülü program yoksa Identity-H CID'leri
hiçbir şey ifade etmez) · `/DW` [0,5000] dışı · `/W` bozuk · `/ToUnicode` yok/akış değil/
1 MiB üstü · harita boş.
**Nota özel:** karakterin kodu yok · kod 0 (CID 0 = `.notdef`) · GID 0 / akış dışı ·
genişlik 0 · nota 255'ten çok farklı karakter → nota bütün olarak düşer.

## Sapma (benim ölçümüm — gerekçeli)
Opus `drfont/fontdict.rs`, `drfont/cidfont.rs`, `freetext/select.rs` taşımaları önerdi.
Ölçüm: `appearance.rs` **190 satır** ve baseline'da **yok** (FLOOR 200 altı) → `mod` satırlarını
bedava taşır; `face.rs` `SlotMetrics`'i çıkarınca ~174'e iner. Bu yüzden daha az taşıma:
- yeni `appearance/cidfont.rs` (~230) + `appearance/cidfont/widths.rs` (~120) — `drfont.rs`'e
  **hiç dokunulmaz** (tavanında kalır),
- yeni `appearance/face/slots.rs` (SlotMetrics + GlyphSource, taşıma),
- `freetext.rs`: `Style`'a `name` alanı, `Face::can_show`/`Face::script_for` çağrıları →
  **net −3 satır** (tavanında kalır), `freetext/select.rs` taşıması gerekmez.
Davranış ve kabul ölçütleri Opus'un planıyla **aynı**.

## Kabul ölçütleri
1. Identity-H + tek CIDFont + `/W`/`/DW` + gömülü program + `/ToUnicode` → not o fontla
   çizilir, font **referansla** kullanılır (kopya yok).
2. Sıra: WinAnsi → paketli yüz → belge CID → atla.
3. Eksik glif/sıfır genişlik → nota bütün olarak düşer (karakter başına ikame yok).
4. Type0 fontu olmayan belge **bayt-aynı**; 5) Latin metin **bayt-aynı**.
6. İki annotasyon aynı `/DA` fontunu paylaşırsa çıktıda tek `/Type0` + tek descriptor +
   tek program (idempotent).

## NEREDE KALDIM (1b) — 2026-11-08 00:15 UTC
Branch `feat/doc-cid-face` (master `a294214` üstünde, **henüz push edilmedi**):
- `bb2e237` plan · `50b1095` slot taşıması (`face/slots.rs`, face 238→114, **saf taşıma**) ·
  `921229d` özellik (`cidfont.rs` 177 + `cidfont/widths.rs` 226) · `a5bf969` 20 okuyucu testi.
- Doğrulandı: `cargo test -p pdfcore --lib` **306** (master 286 + 20), clippy `-D warnings`
  temiz, `files:check` temiz (`face.rs` küçüldü, yeni dosyalar 226/177/224 ≤ 400).
- **PR #666 birleşti** (`a294214`, squash): 21/21 kontrol yeşil, fuzz hedefi ilk koşuda
  sekiz-bayt taşmasını buldu ve düzeltildi; dal silindi.

## SIRADAKİ ADIM (1b kalanı)
1. **N-up entegrasyon testi** (`crates/pdfcore/tests/nup_freetext_cid.rs`): Type0 belge →
   CJK not belge fontuyla çizilir (operand gerçek CID'ler), eksik glif → **bayt-aynı** çıktı,
   Latin metin değişmez, iki annotasyon tek font nesnesini paylaşır (idempotent).
2. `fuzz/fuzz_targets/cid_widths.rs` + `fuzz-pr.yml` adımı (`/W` sayısal ayrıştırıcı).
3. CHANGELOG `[Unreleased] Added` + `docs/roadmap.md`; `claims:check` her iddiaya test adı ister.
4. `npm run files:update` (face.rs 114, yeni dosyalar) → baseline sıkılaşır.
5. Push → PR (gövdede sıra tablosu, reddetme listesi, bütçe tablosu, kapsam dışı) → CI yeşilse merge.
6. Sonra **v7.19.0 sürüm treni** (1b içinde; 20 asset + iki zarf doğrulaması).
