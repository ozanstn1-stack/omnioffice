# ACTIVE PLAN — v7.5.0

## Durum notu (2026-11-02)
Hibrit protokolün 1. maddesi gereği `subagent_claude_opus` çağrıldı; o sırada
araç **product failure** (`Claude Code; query-run; exit code 1`) ile plan
üretmeden düştü — sebebi profildeki Claude Code 2.1.263'ün Opus 5.5'i
desteklememesiydi (API 400, "requires 2.1.280 or newer"). **Opus artık
erişilebilir:** binary 2.1.316'ya yükseltildi ve SDK testi `INIT model:
claude-opus-5-5` + `end_turn` verdi. Bu iş kaleminin planı o kesinti yüzünden
benim tarafımdan yazıldı; **sonraki iş kaleminden itibaren Opus planı yeniden
denenecek** (yalnız plan, dosya değiştirtmeden).

## Seçilen iş kalemi
**Rust kapsam ratchet'i (E4.1'in açık kalan yarısı).**

Gerekçe: kapsam işi ölçüm üretiyor ama hiçbir eşik yok — bugün bir gerileme
(ör. testlerin bir kısmının silinmesi) kimseyi durdurmaz. Ölçüm zaten her PR'da
`rust-coverage` işinde koşuyor, yani eşiği bağlamak ek CI süresi getirmez.
İlk ölçüm (2026-11-02): `%55.23` bölge, `%56.37` fonksiyon, `%56.30` satır.

Neden diğerleri değil (kısa):
- Scanner E2E akışı: pdfium gerektirdiği için yalnız Windows E2E işinde koşar;
  tasarımı yapılabilir ama geri bildirim döngüsü yavaş (son dilim 6 tur aldı).
- Motor önbelleğinin release/beta/nightly'a taşınması: PR CI'da doğrulanamaz
  (yalnız tag/dispatch). Bir sonraki sürüme kadar sessiz kalır.
- `Field label=` i18n taraması: değerli ama büyük bir çeviri kuyruğu açar;
  makine kısmı yapılınca yüzlerce etiket çıkabilir.
- Android ABI matrisi: `workflow_dispatch` doğrulaması 30-40 dk sürer.
- Scorecard/Renovate kararı: belge+config; kapsam ratchet'i ondan daha somut.

## Uygulama planı
1. `scripts/check-rust-coverage.mjs` (yeni)
   - `cargo llvm-cov report --summary-only` çıktısındaki `TOTAL` satırını okur
     (bölge / fonksiyon / satır yüzdeleri).
   - `scripts/coverage-baseline.json` ile karşılaştırır (şema: `{version:1, rust:{regions,functions,lines}}`).
   - Ölçüm tabanın **altına** düşerse `exit 1` ve hangi metrik kaç puan düştüğünü yazar.
   - `--update` yalnızca **yukarı** taşır (ratchet kuralı: taban düşürülemez).
2. `scripts/coverage-baseline.json` (yeni): ilk ölçümden bir puan aşağısı
   (bölge 54, fonksiyon 55, satır 55) — koşular arası sapma yeşil bir PR'ı kırmasın,
   ama gerçek gerileme yakalanabilsin. Gerekçe dosyada yazılı.
3. `.github/workflows/desktop.yml` → `rust-coverage` işine yeni adım:
   özet dosyasını üret, `node scripts/check-rust-coverage.mjs` ile eşiği uygula.
4. Test: `src/lib/coverage-ratchet.test.ts` (yeni)
   - örnek `coverage-summary.txt` metniyle eşiğin altında/üstünde davranışı,
   - `--update`'in tabanı düşüremediği,
   - taban dosyasının şeması.
5. `src/lib/ci-workflow.test.ts`: kapsam işinin eşik adımını ve `coverage-rust`
   artefaktını pinleyen yeni vaka.
6. Doküman: `RELEASE_READINESS.md` (marker'lı iddia), `docs/plan/PHASE-E-PROPOSAL.md`
   (E4.1 Rust yarısı: ölçüm → kapı), `docs/performance-budgets.md` değil — kapsam
   ayrı bir sayfa istemiyor, readiness yeterli.
7. PR: dal → PR → CI yeşil → squash merge → #550'ye yorum.

## Kabul kriterleri
- `node scripts/check-rust-coverage.mjs` sentetik bir özetle hem geçer hem düşük
  ölçümde kırmızı olur (test bunu pinler).
- `npm run test`, `tsc --noEmit`, ESLint, Prettier, `claims:check`, `files:check`
  temiz; CI'da `rust-coverage` işi yeni adımla yeşil.
- Taban dosyası yalnız yükselebilir; test bunu doğrular.

## Riskler / geri alma
- Koşular arası sapma eşiği kırabilir → taban bir puan aşağıda tutuldu; kırılırsa
  `--allow-raise` benzeri bir yükseltme değil, önce ölçümü tekrar etmek gerekir.
- `cargo llvm-cov` sürüm değişimi sayıları kaydırabilir → taban dosyası ve commit
  mesajı bunu gerekçelendirir; geri alma tek commit revert.

## NEREDE KALDIM / SIRADAKI ADIM
- **Tamamlandı (2026-11-02):** Rust kapsam ratchet'i — PR #584, squash `b935b54`.
  `scripts/check-rust-coverage.mjs` + `scripts/coverage-baseline.json` (54/55/55),
  CI adımı, `src/lib/coverage-ratchet.test.ts` (5 vaka), `ci-workflow.test.ts`
  pini, RELEASE_READINESS + PHASE-E güncellemesi, #550'ye yorum.
- **Repo durumu:** master `b935b54`, çalışma ağacı temiz, disk %59 (40 GB boş),
  `target/` yok.
- **Sıradaki adaylar (değer/risk):**
  1. Scanner E2E akışı — pdfium istiyor, yalnız Windows E2E işinde koşabilir;
     tasarım hazır sayılır, geri bildirim döngüsü yavaş.
  2. `Field label=` i18n taraması — makine kısmı küçük, çeviri kuyruğu büyük.
  3. Android ABI matrisi — `workflow_dispatch` doğrulaması 30-40 dk.
  4. Motor önbelleğinin release/beta/nightly'a taşınması — PR CI'da doğrulanamaz.
  5. OpenSSF Scorecard / Renovate kararı — belge + config ağırlıklı.
- **Opus:** erişilebilir (2.1.316). Sıradaki iş kaleminin planı **Opus'tan**
  istenecek (yalnız plan, dosya değiştirtmeden), sonra bu dosyaya yazılacak.
