# Phase F Proposal — v7.52.0 → v8.0.0

**Durum:** Onaylandı (`v7.51.0` sonrası).
**Hedef Yürütme:** Gemini 4 Hydrogen (Medium Effort) — her dilim tek, atomik PR olarak uygulanır.

## Yol Haritası Özeti

| Faz / Hedef Sürüm | Kapsam | Kaynak / Referans |
|---|---|---|
| **Faz 1 (`v7.52.0`)** | PDF Worker Süreç İzolasyonu Genişletmesi (`E2.1`) | `worker.rs`, `worker/jobs.rs`, `pdf_v3.rs`, `commands.rs`, `sign.rs` |
| **Faz 2 (`v7.53.0`)** | Sıfır `set-state-in-effect` Bastırma (`E3.8`), `PdfStudio`/`Ai` Bölme (`E3.2`) & TS Sıkılaştırma (`E3.4`) | `useTool.ts`, `PdfStudio.tsx`, `Ai.tsx`, `tsconfig.json`, `types.ts`, `office-types.ts` |
| **Faz 3 (`v7.54.0`)** | `Reader` ve `Organize` Sayfa Sanallaştırma (`E3.7`) & `ThumbGrid` Ortaklaştırma | `Reader.tsx`, `Organize.tsx`, `components/pages.tsx`, `lib/virtual-pages.ts` |
| **Faz 4 (`v7.55.0`)** | Ofis Sadakati Derinleştirme (`E6.1`): DOCX Yer İmleri, ODT Track Changes, Calc Gizli Boyut | `docx/reader.rs`, `docx/writer.rs`, `odf/text_read.rs`, `odf/text_write.rs`, `xlsx/` |
| **Faz 5 (`v7.56.0`)** | UYAP `.udf` Tam Desteği & Redline DOCX Karşılaştırma Çıktısı (`E6.4`) | `udf.rs`, `compat.rs`, `writer/compare.ts`, `Compare.tsx` |
| **Faz 6 (`v8.0.0`)** | Görsel PDF Form Tasarımcısı (AcroForm Builder), Belge Asistanı 3.0 & İzlenen Klasör Otomasyonu | `pdfcore::forms`, `PdfStudio.tsx`, `Ai.tsx`, `recipes.rs`, `Recipes.tsx` |

---

## Faz 1 (`v7.52.0`): PDF Worker Süreç İzolasyonu Genişletmesi (`E2.1`)

### Dilim 1.1: `pdf_list_form_fields`, `pdf_list_objects`, `pdf_list_text_runs` ve `pdfa_validate` İzolasyonu
- `src-tauri/src/worker.rs`: `Job` enum'una `PdfListFormFields { path }`, `PdfListObjects { path }`, `PdfListTextRuns { path }`, `PdfaValidate { path, level }` varyantlarını ve `args`/`parse`/`name` eşlemelerini ekle.
- `src-tauri/src/worker/jobs.rs`: `execute(job: &Job)` içinde `pdfcore::forms::list_fields_in_file`, `pdfcore::forms::list_page_objects_in_file`, `pdfcore::content::list_text_runs_in_file` ve `pdfcore::pdfa::validate_pdfa` çağrılarını ekle.
- `src-tauri/src/pdf_v3.rs`: `password.is_none()` durumunda `crate::worker::run_json_or_local` ile alt süreçte çalıştır.
- `src-tauri/src/worker_tests.rs`: Yeni 4 `Job` varyantı için CLI round-trip ve yürütme testleri ekle.

### Dilim 1.2: `page_tile` ve `pdf_verify_signatures` Worker İzolasyonu
- `src-tauri/src/worker.rs`: `PdfPageTile { path, page, scale_bits, x, y, width, height, jpeg_quality }` (`f32::to_bits`/`from_bits`) ve `PdfVerifySignatures { path }` varyantlarını ekle.
- `src-tauri/src/commands.rs` & `src-tauri/src/sign.rs`: Parolasız `page_tile` ve çevrimdışı `pdf_verify_signatures` çağrılarını `--worker` üzerinden geçir.

---

## Faz 2 (`v7.53.0`): Sıfır Hook Bastırma (`E3.8`), Büyük Ekran Bölme (`E3.2`) & TS Sıkılaştırma (`E3.4`)

### Dilim 2.1: Kalan 7 `react-hooks/set-state-in-effect` Bastırmasının Kaldırılması
- `src/lib/useTool.ts` (2 adet), `src/screens/PdfStudio.tsx` (4 adet), `src/screens/Ai.tsx` (1 adet) içindeki bastırmaları render-phase kimlik sıfırlaması ve asenkron `.then()` geri çağrılarıyla kaldır.
- Hedef: tüm depoda `react-hooks/set-state-in-effect` bastırması **0**.

### Dilim 2.2: `PdfStudio.tsx` ve `Ai.tsx` Modülerleştirilmesi
- `src/screens/studio/StudioGeometry.ts`, `SignaturesPanel.tsx`, `ObjectsFormsPanel.tsx` alt modüllerini çıkar (`PdfStudio.tsx` 1.904 → <1.100 satır).
- `src/screens/ai/AiChatTab.tsx`, `AiBatchTab.tsx` alt modüllerini çıkar (`Ai.tsx` 1.926 → <1.150 satır).
- `npm run files:update` ile `scripts/file-size-baseline.json` güncelle.

### Dilim 2.3: `tsconfig.json` Sıkılaştırma & `specta.ts` Köprüsü
- `tsconfig.json` içine `"noImplicitReturns": true` ve `"noImplicitOverride": true` ekle.
- `src/lib/types.ts` ve `src/lib/office-types.ts` içindeki kopya tipleri `src/lib/generated/specta.ts` ile hizala, `src/lib/ipc-contract.test.ts` ile doğrula.

---

## Faz 3 (`v7.54.0`): Sanallaştırılmış Sayfa Listeleri (`E3.7`)

### Dilim 3.1: `Reader.tsx` Görünüm Penceresi Sanallaştırması
- `src/lib/virtual-pages.ts` içinde `computeVirtualPageRange(pageHeightsPx, gapPx, scrollTop, viewportHeight, overscanPages = 3)` saf yardımcısını ekle ve birim testlerini yaz.
- `src/screens/Reader.tsx` içinde yalnızca `[startIndex, endIndex]` aralığındaki `<ReaderPage>` bileşenlerini DOM'a bağla; pencere dışı sayfalar gerçek yükseklikte yer tutucu olsun.

### Dilim 3.2: `ThumbGrid` ve `OrganizeGrid` Birleştirme
- `src/components/pages.tsx` içindeki `ThumbGrid`'e `renderThumb` / `renderActions` prop'ları ekleyip `src/screens/Organize.tsx` içindeki kopya `OrganizeGrid`'i kaldır.

---

## Faz 4 (`v7.55.0`): Ofis Sadakati Derinleştirme (`E6.1`)

### Dilim 4.1: DOCX Yer İmleri (`w:bookmarkStart` / `w:bookmarkEnd`)
- `crates/officecore/src/docx/reader.rs` ve `writer.rs` içinde `_GoBack` hariç yer imlerini oku ve yaz.

### Dilim 4.2: ODT Değişiklik İzleme (`text:tracked-changes`) ve Liste Türü Ayrımı
- `crates/officecore/src/odf/text_write.rs` ve `text_read.rs` içinde `<text:tracked-changes>` gidiş-dönüşünü ve `ListInfo.kind` (`"number"` vs `"bullet"`) ayrımını uygula; `compat.rs` içinde ODT `trackChanges` durumunu `Partial` yap.

### Dilim 4.3: Calc Gizli Satır/Sütun Boyutu Koruma
- `crates/officecore/src/model.rs`, `xlsx/sheet_read.rs`, `xlsx/sheet_write.rs` içinde gizli satır/sütunların önceki `row_heights`/`col_widths` değerlerini koru.

---

## Faz 5 (`v7.56.0`): UYAP `.udf` Tam Desteği & Redline DOCX (`E6.4`)

### Dilim 5.1: UYAP `.udf` Görsel, RTF Parçası ve İmza Bilgisi
- `crates/officecore/src/udf.rs` içinde `<image imageData="..."/>` ve `sign.sgn` (CMS/PKCS#7 imzacı + zaman bilgisi) okumasını tamamla, `compat.rs` ve korpus testlerini güncelle.

### Dilim 5.2: Belge Karşılaştırma "Redline DOCX" Çıktısı
- `src/office/writer/compare.ts` ve `src/screens/Compare.tsx` içinde farkları izlenen değişiklikli (Track Changes) `.docx` olarak dışa aktarma ekle.

---

## Faz 6 (`v8.0.0`): Yeni Nesil Ürün Özellikleri

### Dilim 6.1: Görsel PDF Form Tasarımcısı (Interactive AcroForm Builder)
- `crates/pdfcore/src/forms/builder.rs`: `NewFormFieldSpec` (`text`, `checkbox`, `dropdown`, `signature`) ve `create_form_fields` (`/AcroForm` + `/Widget` + Türkçe `/AP /N`).
- `src-tauri/src/pdf_v3.rs` (`pdf_create_form_fields`), `ipc.rs`, `acl:sync`, `bindings:specta` ve `PdfStudio.tsx` üzerinde görsel alan çizme modu.

### Dilim 6.2: Belge Asistanı 3.0 — Çoklu Belge Karşılaştırma Matrisi
- `src/screens/Ai.tsx` ve `src-tauri/src/ai.rs` içinde çoklu belge üzerinde `[BelgeAdı, s. N]` atıflı karşılaştırma matrisi ve tek tıkla Calc dışa aktarımı.

### Dilim 6.3: Masaüstü İzlenen Klasör (Watched Folder) Tarif Otomasyonu
- `src-tauri/src/recipes.rs` ve `src/screens/Recipes.tsx` içinde `file_token_pick_directory` ile yetkilendirilmiş yerel klasörü SHA-256 parmak iziyle tekilleştirerek işleyen otomasyon modu.

---

## Zorunlu Kalite Kapıları (Her PR Dilimi İçin)
```bash
npm run lint && npm run i18n:audit && npx tsc --noEmit && npm test
npm run build && npm run bundle:check && npm run files:check
cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
npm run acl:check && npm run bindings:check && npm run docs:stats:check && npm run claims:check
```
