/**
 * TCKN (Türkiye Cumhuriyeti Kimlik Numarası) Mod-11 Algoritması Doğrulaması.
 */
export function validateTCKN(tckn: string): boolean {
  const clean = tckn.trim();
  if (!/^[1-9]\d{10}$/.test(clean)) {
    return false;
  }

  const digits = clean.split("").map((d) => parseInt(d, 10));

  // 1, 3, 5, 7, 9. basamakların toplamı
  const oddSum = digits[0] + digits[2] + digits[4] + digits[6] + digits[8];
  // 2, 4, 6, 8. basamakların toplamı
  const evenSum = digits[1] + digits[3] + digits[5] + digits[7];

  // 10. basamak: ((oddSum * 7) - evenSum) mod 10
  const tenthDigit = (oddSum * 7 - evenSum) % 10;
  const expectedTenth = tenthDigit < 0 ? tenthDigit + 10 : tenthDigit;

  if (digits[9] !== expectedTenth) {
    return false;
  }

  // 11. basamak: ilk 10 basamağın toplamının mod 10'u
  const firstTenSum = digits.slice(0, 10).reduce((acc, curr) => acc + curr, 0);
  if (digits[10] !== firstTenSum % 10) {
    return false;
  }

  return true;
}

export interface PetitionData {
  recipientAuthority: string; // Makam (örn. İSTANBUL VALİLİĞİNE)
  subject: string; // Konu
  bodyText: string; // Olay Anlatımı ve Açıklamalar
  demands: string; // Sonuç ve İstem
  attachments: string; // Ekler (opsiyonel)
  fullName: string; // Adı Soyadı
  tckn: string; // TCKN
  address: string; // Adres
  phone: string; // Telefon
  date: string; // Tarih (GG.AA.YYYY)
}

/**
 * Generates an internal TextDocument from petition form data.
 */
import { defaultParaProps, newTextDocument, type TextDocument } from "./office-types";

export function createPetitionDocument(data: PetitionData): TextDocument {
  const doc = newTextDocument(data.subject || "Dilekçe");

  // Kenar boşlukları resmî yönetmeliğe uygun (2.5 cm = 70.87 pt)
  doc.page.marginTopPt = 70.87;
  doc.page.marginBottomPt = 70.87;
  doc.page.marginLeftPt = 70.87;
  doc.page.marginRightPt = 70.87;

  const makeRun = (text: string, bold = false) => ({
    text,
    bold,
    italic: false,
    underline: false,
    strike: false,
    color: null,
    highlight: null,
    font: "Times New Roman",
    sizePt: 12,
    link: null,
    comment: null,
    superscript: false,
    subscript: false,
  });

  doc.blocks = [
    // Tarih (Sağa dayalı)
    {
      type: "paragraph",
      props: { ...defaultParaProps(), align: "right", spaceAfterPt: 24 },
      runs: [makeRun(data.date)],
    },
    // Makam Başlığı (Ortalı ve Kalın)
    {
      type: "paragraph",
      props: { ...defaultParaProps(), align: "center", spaceAfterPt: 18 },
      runs: [makeRun(data.recipientAuthority.trim().toLocaleUpperCase("tr-TR"), true)],
    },
    // Konu (Sola dayalı, Kalın başlık)
    {
      type: "paragraph",
      props: { ...defaultParaProps(), align: "left", spaceAfterPt: 14 },
      runs: [makeRun("KONU: ", true), makeRun(data.subject)],
    },
    // Olay Anlatımı / Metin (İki yana yaslı, ilk satır girintili)
    {
      type: "paragraph",
      props: { ...defaultParaProps(), align: "justify", firstLinePt: 35.4, spaceAfterPt: 12 },
      runs: [makeRun(data.bodyText)],
    },
    // Talepler (İki yana yaslı)
    {
      type: "paragraph",
      props: { ...defaultParaProps(), align: "justify", firstLinePt: 35.4, spaceAfterPt: 24 },
      runs: [makeRun(data.demands || "Gereğini bilgilerinize arz ederim.")],
    },
    // İmza Bloğu (Sağa dayalı)
    {
      type: "paragraph",
      props: { ...defaultParaProps(), align: "right", spaceAfterPt: 4 },
      runs: [makeRun(data.fullName.trim().toLocaleUpperCase("tr-TR"), true)],
    },
    {
      type: "paragraph",
      props: { ...defaultParaProps(), align: "right", spaceAfterPt: 24 },
      runs: [makeRun("(İmza)")],
    },
    // İletişim / Kimlik Bilgileri (Sola dayalı)
    {
      type: "paragraph",
      props: { ...defaultParaProps(), align: "left", spaceAfterPt: 2 },
      runs: [makeRun("T.C. Kimlik No: ", true), makeRun(data.tckn)],
    },
    {
      type: "paragraph",
      props: { ...defaultParaProps(), align: "left", spaceAfterPt: 2 },
      runs: [makeRun("Adres: ", true), makeRun(data.address)],
    },
    {
      type: "paragraph",
      props: { ...defaultParaProps(), align: "left", spaceAfterPt: 6 },
      runs: [makeRun("Telefon: ", true), makeRun(data.phone)],
    },
  ];

  if (data.attachments && data.attachments.trim()) {
    doc.blocks.push({
      type: "paragraph",
      props: { ...defaultParaProps(), align: "left", spaceBeforePt: 12 },
      runs: [makeRun("EKLER: ", true), makeRun(data.attachments)],
    });
  }

  return doc;
}
