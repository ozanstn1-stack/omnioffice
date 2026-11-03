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
- **Sıradaki kalem:** Android ABI matrisi + etiketle tetiklenen emülatör koşusu
  (tek `workflow_dispatch` oturumu). PR kapısı değil; bu pencerede doğrulanabilir.
  Başlamadan `subagent_claude_opus` planı alınacak.
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
