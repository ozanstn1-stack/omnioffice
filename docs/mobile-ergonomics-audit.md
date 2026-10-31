# Mobile ergonomics audit — D8 (v6.15.0)

Phone pass over every screen at **360 dp**, the narrowest width the app still
targets: primary actions within one thumb, safe areas clear of the notch and the
system navigation bar, the on-screen keyboard never covering a field or a
primary button, and a landscape layout that keeps the chrome off the page.

This is the D8 acceptance record. The matrix lists all 55 routes; the
device record sheet at the end is filled in on a phone pass, because a VPS
cannot take real 360 dp screenshots. What *is* pinned by CI are the structural
guarantees (action bar placement, safe-area padding, keyboard inset, landscape
rules) in `src/components/screen-actions.test.tsx`,
`src/components/mobile-ergonomics.test.ts` and `src/lib/viewport.test.ts`.

## What changed

1. **Thumb-reach primary actions.** The shared `Screen` shell keeps the
   actions in the header on a desktop window and moves them into a
   `.screen-action-bar` on a phone (< 900 px): a bottom bar in the flex column,
   above the bottom tabs, with 44 px buttons that share the row. 25 of
   55 routes pass header actions through it; the rest already keep their
   primary action in the page flow (`lib/viewport.ts`, `components/layout.tsx`).
2. **Keyboard overlap.** `useKeyboardInset()` publishes the covered pixels as
   `--keyboard-inset`; the action bar lifts itself by that amount instead of
   hiding Save behind the keyboard, and the bottom tabs still step aside. On top
   of that, every focused field is scrolled to the centre 250 ms after focus
   (`useFocusScrollIntoView`) and fields carry `scroll-margin-block: 128px`.
3. **Safe areas.** `viewport-fit=cover` reaches the notch, and the action bar,
   bottom tabs, drawer, top bar and screen content all pad by
   `env(safe-area-inset-*)`; landscape notch edges are padded on the content
   column.
4. **Landscape.** At a phone height (≤ 500 px) the action bar becomes a single
   scrolling row, the bottom tabs drop their labels, the drawer narrows to 60 vw
   and dialogs are capped to the viewport.
5. **Keyboard-navigation and unit tests.** New tests pin the 360 dp action bar,
   the safe-area/keyboard/landscape CSS and the scroll-into-view helper, so a
   regression fails CI instead of the next device pass.

## Rules the matrix applies

- **PASS** means the primary action is reachable with the thumb of the holding
  hand at 360 dp, no safe-area overlap, the keyboard has a defined behaviour and
  landscape keeps the chrome compact.
- **Device pass** means the structure is pinned by tests but the final visual
  confirmation on hardware is recorded in the sheet below.
- Editors (Office) are judged differently on purpose: their primary action is
  editing the canvas with both thumbs on the keyboard, not a single primary
  button; the format toolbar scrolls horizontally at the top and Calc's formula
  bar docks above the keyboard on Android.

## Acceptance matrix (en-US)

| Screen | Primary action | 360 dp placement | Safe areas | Keyboard | Landscape |
|---|---|---|---|---|---|
| Home (`home`) | Open a tool or recent document | Page flow, lower half | Content padded | Fields scroll clear | Content only, compact chrome |
| Read (`reader`) | Turn pages, search in the document | Page flow, lower half | Content padded | Fields scroll clear | Content only, compact chrome |
| AI Assistant (`ai`) | Ask the question / run the action | Bottom action bar (D8) | Bar + tabs padded | Bar lifts above keyboard | Compact single row |
| AI Library (`aiLibrary`) | Open or delete a saved answer | Bottom action bar (D8) | Bar + tabs padded | Bar lifts above keyboard | Compact single row |
| Merge (`merge`) | Merge the picked PDFs | Bottom action bar (D8) | Bar + tabs padded | Bar lifts above keyboard | Compact single row |
| Organize Pages (`organize`) | Apply the new page order | Bottom action bar (D8) | Bar + tabs padded | Bar lifts above keyboard | Compact single row |
| Split (`split`) | Split the document | Bottom action bar (D8) | Bar + tabs padded | Bar lifts above keyboard | Compact single row |
| Compress (`compress`) | Compress at the chosen level | Bottom action bar (D8) | Bar + tabs padded | Bar lifts above keyboard | Compact single row |
| OCR (`ocr`) | Run OCR | Bottom action bar (D8) | Bar + tabs padded | Bar lifts above keyboard | Compact single row |
| PDF → Images (`pdfToImages`) | Convert to images | Bottom action bar (D8) | Bar + tabs padded | Bar lifts above keyboard | Compact single row |
| Images → PDF (`imagesToPdf`) | Build the PDF | Bottom action bar (D8) | Bar + tabs padded | Bar lifts above keyboard | Compact single row |
| Password Protect (`protect`) | Set the password | Bottom action bar (D8) | Bar + tabs padded | Bar lifts above keyboard | Compact single row |
| Unlock PDF (`unlock`) | Remove the password | Bottom action bar (D8) | Bar + tabs padded | Bar lifts above keyboard | Compact single row |
| Watermark (`watermark`) | Apply the watermark | Bottom action bar (D8) | Bar + tabs padded | Bar lifts above keyboard | Compact single row |
| Add Text / Image (`annotate`) | Place the text or image on the page | Page flow, lower half | Content padded | Fields scroll clear | Content only, compact chrome |
| Redact (`redact`) | Apply the redactions | Bottom action bar (D8) | Bar + tabs padded | Bar lifts above keyboard | Compact single row |
| Compare (`compare`) | Compare the two versions | Bottom action bar (D8) | Bar + tabs padded | Bar lifts above keyboard | Compact single row |
| Inspect (`inspect`) | Inspect the document | Bottom action bar (D8) | Bar + tabs padded | Bar lifts above keyboard | Compact single row |
| Metadata (`metadata`) | Save or clear the metadata | Page flow, lower half | Content padded | Fields scroll clear | Content only, compact chrome |
| Page Tools (`pageTools`) | Apply the page tool | Bottom action bar (D8) | Bar + tabs padded | Bar lifts above keyboard | Compact single row |
| Batch Processing (`batch`) | Run the batch job | Bottom action bar (D8) | Bar + tabs padded | Bar lifts above keyboard | Compact single row |
| History (`history`) | Restore or compare a snapshot | Bottom action bar (D8) | Bar + tabs padded | Bar lifts above keyboard | Compact single row |
| Settings (`settings`) | Change a setting | Page flow, lower half | Content padded | Fields scroll clear | Content only, compact chrome |
| Document Info (`info`) | Read the document facts | Page flow, lower half | Content padded | Fields scroll clear | Content only, compact chrome |
| Office (`office`) | Edit the open document | Page flow, lower half | Content padded | Fields scroll clear | Content only, compact chrome |
| Documents (`documents`) | Open a Writer document | Page flow, lower half | Content padded | Fields scroll clear | Content only, compact chrome |
| Spreadsheets (`spreadsheets`) | Open a Calc workbook | Page flow, lower half | Content padded | Fields scroll clear | Content only, compact chrome |
| Presentations (`presentations`) | Open an Impress deck | Page flow, lower half | Content padded | Fields scroll clear | Content only, compact chrome |
| Notes (`notes`) | Write or open a note | Page flow, lower half | Content padded | Fields scroll clear | Content only, compact chrome |
| Templates (`templates`) | Open a template | Page flow, lower half | Content padded | Fields scroll clear | Content only, compact chrome |
| Convert (`converter`) | Convert the document | Page flow, lower half | Content padded | Fields scroll clear | Content only, compact chrome |
| Invoices (`invoices`) | Read or export the invoice | Page flow, lower half | Content padded | Fields scroll clear | Content only, compact chrome |
| E-mail (.eml) (`eml`) | Open the e-mail and its attachments | Page flow, lower half | Content padded | Fields scroll clear | Content only, compact chrome |
| Scan (`scanner`) | Take a page photo / build the PDF | Page flow, lower half | Content padded | Fields scroll clear | Content only, compact chrome |
| Recipes (`recipes`) | Pick a recipe and open its tool | Page flow, lower half | Content padded | Fields scroll clear | Content only, compact chrome |
| Mail merge (`mailMerge`) | Run the merge | Page flow, lower half | Content padded | Fields scroll clear | Content only, compact chrome |
| Cleaner (`cleaner`) | Clean the document | Page flow, lower half | Content padded | Fields scroll clear | Content only, compact chrome |
| Draw (`draw`) | Draw on the canvas | Page flow, lower half | Content padded | Fields scroll clear | Content only, compact chrome |
| Planner (`planner`) | Edit the plan | Page flow, lower half | Content padded | Fields scroll clear | Content only, compact chrome |
| Data (`data`) | Edit the data table | Page flow, lower half | Content padded | Fields scroll clear | Content only, compact chrome |
| PDF Forms (`pdfForms`) | Fill and flatten the form | Page flow, lower half | Content padded | Fields scroll clear | Content only, compact chrome |
| PDF Studio (`pdfStudio`) | Apply a studio operation | Page flow, lower half | Content padded | Fields scroll clear | Content only, compact chrome |
| Document Vault (`vault`) | Index a folder or search it | Bottom action bar (D8) | Bar + tabs padded | Bar lifts above keyboard | Compact single row |
| Cloud Sync (`sync`) | Link a cloud folder and sync | Bottom action bar (D8) | Bar + tabs padded | Bar lifts above keyboard | Compact single row |
| Compatibility (`compat`) | Read the compatibility report | Page flow, lower half | Content padded | Fields scroll clear | Content only, compact chrome |
| Plugins (`plugins`) | Install or run a plugin | Bottom action bar (D8) | Bar + tabs padded | Bar lifts above keyboard | Compact single row |
| Jobs (`jobs`) | Inspect or cancel a job | Bottom action bar (D8) | Bar + tabs padded | Bar lifts above keyboard | Compact single row |
| Petition Wizard (`petition`) | Fill the petition wizard | Page flow, lower half | Content padded | Fields scroll clear | Content only, compact chrome |
| Official letter (`officialLetter`) | Produce the official letter | Page flow, lower half | Content padded | Fields scroll clear | Content only, compact chrome |
| Notification deadline (`deadline`) | Calculate the deadline | Page flow, lower half | Content padded | Fields scroll clear | Content only, compact chrome |
| Evidence bundle (`evidence`) | Build the evidence bundle | Page flow, lower half | Content padded | Fields scroll clear | Content only, compact chrome |
| KVKK Compliance (`kvkk`) | Run the KVKK check | Page flow, lower half | Content padded | Fields scroll clear | Content only, compact chrome |
| Document Health (`documentHealth`) | Run the health check | Bottom action bar (D8) | Bar + tabs padded | Bar lifts above keyboard | Compact single row |
| Verify signature (`verify`) | Verify the signature | Bottom action bar (D8) | Bar + tabs padded | Bar lifts above keyboard | Compact single row |
| AI task packs (`aiPacks`) | Run a task pack | Bottom action bar (D8) | Bar + tabs padded | Bar lifts above keyboard | Compact single row |

## Device record sheet (fill on the pass)

| # | Scenario | Expected | Result |
|---|---|---|---|
| 1 | 360×800 portrait, every route | Primary action reachable with the holding thumb; no clipped header | |
| 2 | Notch / punch-hole top | Title and theme button clear of the cutout | |
| 3 | 3-button navigation bar | Bottom tabs and action bar fully above it | |
| 4 | Keyboard over a form (Settings, Metadata, Watermark) | Focused field visible; action bar above the keyboard | |
| 5 | Keyboard over a search field (Reader) | Field and results visible | |
| 6 | Landscape 800×360 | Action bar one compact row; tabs icon-only; no dialog taller than the screen | |
| 7 | Landscape with the drawer open | Drawer 60 vw, nav reachable, closes on overlay tap | |
| 8 | Split screen / small window (360 dp half) | Same layout as portrait; no horizontal scroll | |
| 9 | Turkish (tr-TR) copy on the longest labels | No clipped action labels; buttons wrap to a second row | |
| 10 | Office editor, portrait | Toolbar scrolls horizontally; save reachable; formula bar docked (Calc) | |
| 11 | Scanner, portrait | Take photo and Build PDF inside the lower half | |
| 12 | LAN sync card, portrait | Pair / sync / trash actions in the lower half | |
| 13 | Long document names | Ellipsis, no layout push | |
| 14 | System font scale +20 % | Action bar wraps instead of clipping | |

Existing phone screenshots from the C11 device round:
`docs/screenshots/40-android-home.png`, `docs/screenshots/41-android-menu.png`.
The D8 pass stores one screenshot per route in the release checklist (personal
document names are cropped out), since the corpus run is manual.

## Known limits

- The pass above was verified structurally (CSS + DOM tests) and by code
  inspection on the VPS; the physical phone matrix is the manual step recorded
  in the sheet. No Google Play services or ML Kit are involved in any of it.
- Two-finger gestures in the editors (pinch-zoom on the canvas) stay two-finger
  by design; no *primary* action requires a second hand.

---

# Mobil ergonomi denetimi — D8 (v6.15.0)

Uygulamanın hedeflediği en dar genişlik olan **360 dp** için tüm ekranların telefon
denetimi: birincil eylemler tek başparmakla erişilebilir, güvenli alanlar çentik
ve sistem gezinme çubuğuyla çakışmıyor, sanal klavye hiçbir alanı veya birincil
düğmeyi örtmüyor ve yatay modda çevre birimleri sayfayı kapatmıyor.

Bu belge D8 kabul kaydıdır. Matris 55 rotanın tamamını listeler; sondaki
cihaz kayıt föyü telefon turunda doldurulur, çünkü VPS gerçek 360 dp ekran
görüntüsü alamaz. CI'nın garanti ettiği kısım yapısal kurallardır (eylem çubuğu
yerleşimi, güvenli alan dolguları, klavye payı, yatay mod kuralları):
`src/components/screen-actions.test.tsx`,
`src/components/mobile-ergonomics.test.ts` ve `src/lib/viewport.test.ts`.

## Neler değişti

1. **Başparmak erişimli birincil eylemler.** Ortak `Screen` kabuğu masaüstü
   penceresinde eylemleri başlıkta tutar, telefonda (< 900 px) `.screen-action-bar`
   alt eylem çubuğuna taşır: alt sekmelerin üstünde, 44 px düğmelerle satırı
   paylaşan bir çubuk. 55 rotanın 25 tanesi başlık eylemlerini
   buradan geçirir; kalanlar birincil eylemini zaten sayfa akışında tutuyor
   (`lib/viewport.ts`, `components/layout.tsx`).
2. **Klavye çakışması.** `useKeyboardInset()` klavyenin örttüğü pikselleri
   `--keyboard-inset` olarak yayınlar; eylem çubuğu bu kadar yukarı kalkar ve
   Kaydet düğmesi klavyenin arkasında kalmaz, alt sekmeler yine çekilir. Ayrıca
   odaklanan her alan 250 ms sonra ekranın ortasına kaydırılır
   (`useFocusScrollIntoView`) ve alanlarda `scroll-margin-block: 128px` vardır.
3. **Güvenli alanlar.** `viewport-fit=cover` çentiğe uzanır; eylem çubuğu, alt
   sekmeler, çekmece, üst çubuk ve ekran içeriği `env(safe-area-inset-*)` ile
   dolgulanır; yatay modda çentik kenarları içerik sütununda korunur.
4. **Yatay mod.** Telefon yüksekliğinde (≤ 500 px) eylem çubuğu tek satırda
   kaydırılır, alt sekmeler etiketlerini bırakır, çekmece 60 vw'ye iner ve
   diyaloglar ekran yüksekliğine sığdırılır.
5. **Testler.** 360 dp eylem çubuğu, güvenli alan/klavye/yatay mod CSS'i ve
   kaydırma yardımcısı yeni testlerle sabitlendi; bir gerileme cihaz turu yerine
   CI'da kırılır.

## Matrisin uyguladığı kurallar

- **PASS**: birincil eylem 360 dp'de tutan elin başparmağıyla erişilebilir,
  güvenli alan çakışması yok, klavyenin tanımlı bir davranışı var ve yatay modda
  çevre birimleri kompakt.
- **Cihaz turu**: yapı testlerle sabit, son görsel onay aşağıdaki föyde kaydedilir.
- Editörler (Office) bilinçli olarak farklı değerlendirilir: birincil eylem tuvali
  iki başparmakla düzenlemektir, tek bir düğme değil; biçim araç çubuğu yatay
  kaydırılır ve Calc formül çubuğu Android'de klavyenin üstüne yerleşir.

## Kabul matrisi (tr-TR)

| Ekran | Birincil eylem | 360 dp yerleşim | Güvenli alanlar | Klavye | Yatay mod |
|---|---|---|---|---|---|
| Ana sayfa (`home`) | Araç veya son belge aç | Sayfa akışı, alt yarı | İçerik dolgulu | Alanlar görünür alana kayar | Yalnız içerik, kompakt çevre |
| Oku (`reader`) | Sayfa çevir, belgede ara | Sayfa akışı, alt yarı | İçerik dolgulu | Alanlar görünür alana kayar | Yalnız içerik, kompakt çevre |
| AI asistanı (`ai`) | Soruyu sor / eylemi çalıştır | Alt eylem çubuğu (D8) | Çubuk ve sekmeler dolgulu | Çubuk klavyenin üstüne kalkar | Tek satır, kompakt |
| AI kitaplığı (`aiLibrary`) | Kayıtlı yanıtı aç veya sil | Alt eylem çubuğu (D8) | Çubuk ve sekmeler dolgulu | Çubuk klavyenin üstüne kalkar | Tek satır, kompakt |
| Birleştir (`merge`) | Seçilen PDF’leri birleştir | Alt eylem çubuğu (D8) | Çubuk ve sekmeler dolgulu | Çubuk klavyenin üstüne kalkar | Tek satır, kompakt |
| Sayfaları düzenle (`organize`) | Yeni sayfa sırasını uygula | Alt eylem çubuğu (D8) | Çubuk ve sekmeler dolgulu | Çubuk klavyenin üstüne kalkar | Tek satır, kompakt |
| Böl (`split`) | Belgeyi böl | Alt eylem çubuğu (D8) | Çubuk ve sekmeler dolgulu | Çubuk klavyenin üstüne kalkar | Tek satır, kompakt |
| Sıkıştır (`compress`) | Seçilen düzeyde sıkıştır | Alt eylem çubuğu (D8) | Çubuk ve sekmeler dolgulu | Çubuk klavyenin üstüne kalkar | Tek satır, kompakt |
| OCR (`ocr`) | OCR çalıştır | Alt eylem çubuğu (D8) | Çubuk ve sekmeler dolgulu | Çubuk klavyenin üstüne kalkar | Tek satır, kompakt |
| PDF → Görsel (`pdfToImages`) | Görsele dönüştür | Alt eylem çubuğu (D8) | Çubuk ve sekmeler dolgulu | Çubuk klavyenin üstüne kalkar | Tek satır, kompakt |
| Görseller → PDF (`imagesToPdf`) | PDF oluştur | Alt eylem çubuğu (D8) | Çubuk ve sekmeler dolgulu | Çubuk klavyenin üstüne kalkar | Tek satır, kompakt |
| Parola koruması (`protect`) | Parolayı uygula | Alt eylem çubuğu (D8) | Çubuk ve sekmeler dolgulu | Çubuk klavyenin üstüne kalkar | Tek satır, kompakt |
| PDF kilidini aç (`unlock`) | Parolayı kaldır | Alt eylem çubuğu (D8) | Çubuk ve sekmeler dolgulu | Çubuk klavyenin üstüne kalkar | Tek satır, kompakt |
| Filigran (`watermark`) | Filigranı uygula | Alt eylem çubuğu (D8) | Çubuk ve sekmeler dolgulu | Çubuk klavyenin üstüne kalkar | Tek satır, kompakt |
| Metin / Görsel ekle (`annotate`) | Metni veya görseli sayfaya yerleştir | Sayfa akışı, alt yarı | İçerik dolgulu | Alanlar görünür alana kayar | Yalnız içerik, kompakt çevre |
| Karartma (`redact`) | Karartmayı uygula | Alt eylem çubuğu (D8) | Çubuk ve sekmeler dolgulu | Çubuk klavyenin üstüne kalkar | Tek satır, kompakt |
| Karşılaştır (`compare`) | İki sürümü karşılaştır | Alt eylem çubuğu (D8) | Çubuk ve sekmeler dolgulu | Çubuk klavyenin üstüne kalkar | Tek satır, kompakt |
| Denetle (`inspect`) | Belgeyi denetle | Alt eylem çubuğu (D8) | Çubuk ve sekmeler dolgulu | Çubuk klavyenin üstüne kalkar | Tek satır, kompakt |
| Metaveri (`metadata`) | Metaveriyi kaydet veya temizle | Sayfa akışı, alt yarı | İçerik dolgulu | Alanlar görünür alana kayar | Yalnız içerik, kompakt çevre |
| Sayfa araçları (`pageTools`) | Sayfa aracını uygula | Alt eylem çubuğu (D8) | Çubuk ve sekmeler dolgulu | Çubuk klavyenin üstüne kalkar | Tek satır, kompakt |
| Toplu işlem (`batch`) | Toplu işi çalıştır | Alt eylem çubuğu (D8) | Çubuk ve sekmeler dolgulu | Çubuk klavyenin üstüne kalkar | Tek satır, kompakt |
| Geçmiş (`history`) | Anlık görüntüyü geri yükle veya karşılaştır | Alt eylem çubuğu (D8) | Çubuk ve sekmeler dolgulu | Çubuk klavyenin üstüne kalkar | Tek satır, kompakt |
| Ayarlar (`settings`) | Bir ayarı değiştir | Sayfa akışı, alt yarı | İçerik dolgulu | Alanlar görünür alana kayar | Yalnız içerik, kompakt çevre |
| Belge bilgisi (`info`) | Belge bilgilerini oku | Sayfa akışı, alt yarı | İçerik dolgulu | Alanlar görünür alana kayar | Yalnız içerik, kompakt çevre |
| Ofis (`office`) | Açık belgeyi düzenle | Sayfa akışı, alt yarı | İçerik dolgulu | Alanlar görünür alana kayar | Yalnız içerik, kompakt çevre |
| Belgeler (`documents`) | Writer belgesi aç | Sayfa akışı, alt yarı | İçerik dolgulu | Alanlar görünür alana kayar | Yalnız içerik, kompakt çevre |
| Hesap tabloları (`spreadsheets`) | Calc çalışma kitabı aç | Sayfa akışı, alt yarı | İçerik dolgulu | Alanlar görünür alana kayar | Yalnız içerik, kompakt çevre |
| Sunumlar (`presentations`) | Impress sunumu aç | Sayfa akışı, alt yarı | İçerik dolgulu | Alanlar görünür alana kayar | Yalnız içerik, kompakt çevre |
| Notlar (`notes`) | Not yaz veya aç | Sayfa akışı, alt yarı | İçerik dolgulu | Alanlar görünür alana kayar | Yalnız içerik, kompakt çevre |
| Şablonlar (`templates`) | Şablon aç | Sayfa akışı, alt yarı | İçerik dolgulu | Alanlar görünür alana kayar | Yalnız içerik, kompakt çevre |
| Dönüştür (`converter`) | Belgeyi dönüştür | Sayfa akışı, alt yarı | İçerik dolgulu | Alanlar görünür alana kayar | Yalnız içerik, kompakt çevre |
| Faturalar (`invoices`) | Faturayı oku veya dışa aktar | Sayfa akışı, alt yarı | İçerik dolgulu | Alanlar görünür alana kayar | Yalnız içerik, kompakt çevre |
| E-posta (.eml) (`eml`) | E-postayı ve eklerini aç | Sayfa akışı, alt yarı | İçerik dolgulu | Alanlar görünür alana kayar | Yalnız içerik, kompakt çevre |
| Tara (`scanner`) | Sayfa fotoğrafı çek / PDF oluştur | Sayfa akışı, alt yarı | İçerik dolgulu | Alanlar görünür alana kayar | Yalnız içerik, kompakt çevre |
| Tarifler (`recipes`) | Tarif seç ve aracını aç | Sayfa akışı, alt yarı | İçerik dolgulu | Alanlar görünür alana kayar | Yalnız içerik, kompakt çevre |
| Adres mektup birleştirme (`mailMerge`) | Birleştirmeyi çalıştır | Sayfa akışı, alt yarı | İçerik dolgulu | Alanlar görünür alana kayar | Yalnız içerik, kompakt çevre |
| Temizleyici (`cleaner`) | Belgeyi temizle | Sayfa akışı, alt yarı | İçerik dolgulu | Alanlar görünür alana kayar | Yalnız içerik, kompakt çevre |
| Çizim (`draw`) | Tuvale çiz | Sayfa akışı, alt yarı | İçerik dolgulu | Alanlar görünür alana kayar | Yalnız içerik, kompakt çevre |
| Planlayıcı (`planner`) | Planı düzenle | Sayfa akışı, alt yarı | İçerik dolgulu | Alanlar görünür alana kayar | Yalnız içerik, kompakt çevre |
| Veri (`data`) | Veri tablosunu düzenle | Sayfa akışı, alt yarı | İçerik dolgulu | Alanlar görünür alana kayar | Yalnız içerik, kompakt çevre |
| PDF formları (`pdfForms`) | Formu doldur ve düzleştir | Sayfa akışı, alt yarı | İçerik dolgulu | Alanlar görünür alana kayar | Yalnız içerik, kompakt çevre |
| PDF Stüdyo (`pdfStudio`) | Stüdyo işlemini uygula | Sayfa akışı, alt yarı | İçerik dolgulu | Alanlar görünür alana kayar | Yalnız içerik, kompakt çevre |
| Belge Kasası (`vault`) | Klasörü dizinle veya ara | Alt eylem çubuğu (D8) | Çubuk ve sekmeler dolgulu | Çubuk klavyenin üstüne kalkar | Tek satır, kompakt |
| Bulut Eşitleme (`sync`) | Bulut klasörünü bağla ve eşitle | Alt eylem çubuğu (D8) | Çubuk ve sekmeler dolgulu | Çubuk klavyenin üstüne kalkar | Tek satır, kompakt |
| Uyumluluk (`compat`) | Uyumluluk raporunu oku | Sayfa akışı, alt yarı | İçerik dolgulu | Alanlar görünür alana kayar | Yalnız içerik, kompakt çevre |
| Eklentiler (`plugins`) | Eklenti kur veya çalıştır | Alt eylem çubuğu (D8) | Çubuk ve sekmeler dolgulu | Çubuk klavyenin üstüne kalkar | Tek satır, kompakt |
| İşler (`jobs`) | İşi incele veya iptal et | Alt eylem çubuğu (D8) | Çubuk ve sekmeler dolgulu | Çubuk klavyenin üstüne kalkar | Tek satır, kompakt |
| Dilekçe Sihirbazı (`petition`) | Dilekçe sihirbazını doldur | Sayfa akışı, alt yarı | İçerik dolgulu | Alanlar görünür alana kayar | Yalnız içerik, kompakt çevre |
| Resmî yazı (`officialLetter`) | Resmî yazıyı oluştur | Sayfa akışı, alt yarı | İçerik dolgulu | Alanlar görünür alana kayar | Yalnız içerik, kompakt çevre |
| Tebligat süresi (`deadline`) | Süreyi hesapla | Sayfa akışı, alt yarı | İçerik dolgulu | Alanlar görünür alana kayar | Yalnız içerik, kompakt çevre |
| Delil klasörü (`evidence`) | Delil klasörünü oluştur | Sayfa akışı, alt yarı | İçerik dolgulu | Alanlar görünür alana kayar | Yalnız içerik, kompakt çevre |
| KVKK Uyumluluk (`kvkk`) | KVKK denetimini çalıştır | Sayfa akışı, alt yarı | İçerik dolgulu | Alanlar görünür alana kayar | Yalnız içerik, kompakt çevre |
| Belge Sağlığı (`documentHealth`) | Sağlık denetimini çalıştır | Alt eylem çubuğu (D8) | Çubuk ve sekmeler dolgulu | Çubuk klavyenin üstüne kalkar | Tek satır, kompakt |
| İmza doğrula (`verify`) | İmzayı doğrula | Alt eylem çubuğu (D8) | Çubuk ve sekmeler dolgulu | Çubuk klavyenin üstüne kalkar | Tek satır, kompakt |
| AI görev paketleri (`aiPacks`) | Görev paketini çalıştır | Alt eylem çubuğu (D8) | Çubuk ve sekmeler dolgulu | Çubuk klavyenin üstüne kalkar | Tek satır, kompakt |

## Cihaz kayıt föyü (turda doldurulur)

| # | Senaryo | Beklenen | Sonuç |
|---|---|---|---|
| 1 | 360×800 dikey, tüm rotalar | Birincil eylem tutan başparmakla erişilebilir; başlık kırpılmaz | |
| 2 | Çentik / delikli kamera (üst) | Başlık ve tema düğmesi çentiğin altında | |
| 3 | 3 düğmeli gezinme çubuğu | Alt sekmeler ve eylem çubuğu tamamen üstünde | |
| 4 | Form üzerinde klavye (Ayarlar, Metaveri, Filigran) | Odaklı alan görünür; eylem çubuğu klavyenin üstünde | |
| 5 | Arama alanında klavye (Oku) | Alan ve sonuçlar görünür | |
| 6 | Yatay 800×360 | Eylem çubuğu tek satır; sekmeler simge; ekrandan yüksek diyalog yok | |
| 7 | Çekmece açıkken yatay | Çekmece 60 vw, menü erişilebilir, karartmaya dokununca kapanır | |
| 8 | Bölünmüş ekran / küçük pencere (360 dp yarım) | Dikey mod gibi; yatay kaydırma yok | |
| 9 | Türkçe (tr-TR) en uzun etiketler | Eylem etiketleri kırpılmaz; düğmeler gerekirse ikinci satıra sarar | |
| 10 | Office editörü, dikey | Araç çubuğu yatay kaydırılır; kaydet erişilebilir; formül çubuğu yerleşik (Calc) | |
| 11 | Tarayıcı, dikey | Fotoğraf çek ve PDF oluştur alt yarıda | |
| 12 | LAN eşitleme kartı, dikey | Eşleştir / eşitle / çöp kutusu eylemleri alt yarıda | |
| 13 | Uzun belge adları | Üç nokta ile kısalır, yerleşimi itmez | |
| 14 | Sistem yazı ölçeği +%20 | Eylem çubuğu kırpılmak yerine sarar | |

C11 cihaz turundan mevcut telefon ekran görüntüleri:
`docs/screenshots/40-android-home.png`, `docs/screenshots/41-android-menu.png`.
D8 turu her rota için bir ekran görüntüsünü sürüm kontrol listesine kaydeder
(kişisel belge adları kırpılarak); korpus turu manueldir.

## Bilinen sınırlar

- Yukarıdaki tur VPS'te yapısal olarak (CSS + DOM testleri) ve kod incelemesiyle
  doğrulandı; fiziksel telefon matrisi föyde kaydedilen manuel adımdır. Hiçbir
  aşamada Google Play servisi veya ML Kit kullanılmaz.
- Editörlerdeki iki parmaklı hareketler (tuvalde pinch-zoom) tasarım gereği iki
  parmaktır; hiçbir *birincil* eylem ikinci el gerektirmez.
