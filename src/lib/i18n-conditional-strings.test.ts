import { describe, expect, it } from "vitest";
import en from "./i18n/en.json";
import tr from "./i18n/tr.json";

// The strings this slice moved out of hand-written conditionals must read
// exactly as before in Turkish: a translation that "improves" the wording is a
// silent UI change, and these are the ones the audit found by measuring
// expressions rather than literals.
const MOVED: [string, string, string][] = [
  ["common.hide", "Hide", "Gizle"],
  ["common.show", "Show", "Göster"],
  ["history.date", "Date", "Tarih"],
  ["studio.noPrivateKey", "no private key", "özel anahtar yok"],
  [
    "merge.localNote",
    "Merging happens fully offline; your documents never leave this device.",
    "Birleştirme tamamen yerel olarak yapılır; dosyalarınız cihazdan ayrılmaz.",
  ],
  [
    "ocr.textLayerHint",
    "This document already contains a text layer — OCR is mainly useful for scanned pages.",
    "Bu belgede zaten bir metin katmanı var — yalnızca taranmış sayfalar için OCR önerilir.",
  ],
  ["compress.estimateExact", "exact", "tam"],
  [
    "petition.subtitle",
    "Create standards-compliant DOCX and PDF petitions for submission to official authorities.",
    "Resmî makamlara sunulmak üzere standartlara uygun DOCX ve PDF dilekçe oluşturun.",
  ],
  [
    "petition.subtitleUdf",
    "Create standards-compliant UDF, DOCX and PDF petitions for submission to official authorities.",
    "Resmî makamlara sunulmak üzere standartlara uygun UDF, DOCX ve PDF dilekçe oluşturun.",
  ],
];

describe("strings moved out of conditional branches", () => {
  it("keep their exact wording in both languages", () => {
    for (const [key, english, turkish] of MOVED) {
      expect((en as Record<string, string>)[key], `${key} en`).toBe(english);
      expect((tr as Record<string, string>)[key], `${key} tr`).toBe(turkish);
    }
  });

  it("gives the page-break marker its own key", () => {
    // `writer.pageBreak` used to be defined twice, and the second definition
    // won: the ribbon button asking for it showed "— page break —". The marker
    // keeps the decorated text, the button keeps the plain label.
    expect((en as Record<string, string>)["writer.pageBreak"]).toBe("Page break");
    expect((en as Record<string, string>)["writer.pageBreakMarker"]).toBe("— page break —");
    expect((tr as Record<string, string>)["writer.pageBreak"]).toBe("Sayfa sonu");
    expect((tr as Record<string, string>)["writer.pageBreakMarker"]).toBe("— sayfa sonu —");
  });
});
