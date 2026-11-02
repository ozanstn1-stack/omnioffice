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

## NEREDE KALDIM / SIRADAKI ADIM
- **Tamamlandı (2026-11-02):** PR A — i18n prop taraması. PR #585, squash `f7e90d1`.
  `scripts/lib/i18n-jsx.mjs` (+`.d.mts`), audit v3 (`--props-report`, ayrı bölüm
  raporları), `i18n-jsx-audit.test.ts` (12 vaka), `ci-workflow.test.ts` v3 pini,
  `PetitionWizard` on etiketi sözlüğe taşındı, docs + RELEASE_READINESS.
  Taban v3: **77 metin / 9 öznitelik / 15 prop** (props 25 → 15).
- **Repo:** master `f7e90d1`, çalışma ağacı temiz, `target/` yok.
- **Sıradaki dilim (B):** `--props-report` kuyruğu — kalan 15 prop:
  `components/files.tsx` (Dismiss), `components/pages.tsx` (Previous/Next),
  `screens/Annotate.tsx` (Opacity ×2), `screens/Watermark.tsx` (Tile across page),
  `screens/KvkkWizard.tsx` (3 Türkçe etiket). `office/calc/ui/CalcRibbon.tsx`'teki
  SUM/AVERAGE/IF/COUNT/ROUND/VLOOKUP **formül adları**, çevrilmemeli — literal kalacak.
  Sonra `hint`/`description` prop'ları taramaya katılır.
- **Not:** `KvkkWizard` ve `PetitionWizard` etiketleri çevrilirken `hint` metinleri
  Türkçe kalıyor; `hint` taramaya katılınca o ekranlar bütün olarak çevrilecek.
