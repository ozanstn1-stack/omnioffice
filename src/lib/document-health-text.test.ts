import { describe, expect, it } from "vitest";

import en from "./i18n/en.json";
import tr from "./i18n/tr.json";
import type { HealthIssue } from "./document-health";

// The findings are built as dictionary keys plus parameters (E4.7), so the
// module never decides what language the user reads. This test holds the other
// half of the contract: the Turkish wording is exactly what the screen said
// before the move, and every key exists in both tables.
const OLD_TURKISH: Array<[string, string, string, Record<string, string | number>]> = [
  [
    "tckn",
    "T.C. Kimlik Numarası (TCKN) Tespit Edildi",
    "Belgede {count} adet geçerli TCKN tespit edildi. KVKK kapsamında yüksek risk taşır.",
    { count: 1 },
  ],
  ["phone", "Telefon Numarası Tespit Edildi", "Belgede {count} adet telefon numarası bulundu.", { count: 2 }],
  ["email", "E-Posta Adresi Tespit Edildi", "Belgede {count} adet e-posta adresi bulundu.", { count: 2 }],
  ["iban", "IBAN Numarası Tespit Edildi", "Belgede {count} adet banka hesap (IBAN) numarası bulundu.", { count: 1 }],
  [
    "metadata",
    "Gizli Belge Meta Verileri",
    "Belgede yazar, yazılım veya kurum bilgisi ifşa edebilecek {count} meta veri alanı mevcut: {fields}.",
    { count: 2, fields: "author, creator" },
  ],
  ["missingTitle", "Belge Başlığı Eksik", "Ekran okuyucular ve arama indeksleri için belge başlığı tanımlanmamış.", {}],
  [
    "missingLanguage",
    "Belge Dili Belirtilmemiş",
    "Ekran okuyucu sentezleyicileri için dil kodu (tr-TR / en-US) atanmamış.",
    {},
  ],
  [
    "fonts",
    "Gömülü Olmayan / Bozuk Fontlar",
    "Belgede {count} adet font tanımı eksik veya gömülmemiş. Farklı cihazlarda metin kayabilir.",
    { count: 3 },
  ],
  [
    "signature",
    "Belge Dijital Olarak İmzalanmamış",
    "Resmî geçerlilik veya değiştirilmezlik doğrulaması için PAdES / e-İmza bulunmuyor.",
    {},
  ],
];

/** `fill` from src/lib/i18n.ts, without pulling the runtime in. */
const fill = (template: string, params: Record<string, string | number>) =>
  template.replace(/\{\s*([A-Za-z0-9_]+)\s*\}/g, (whole, name: string) =>
    Object.hasOwn(params, name) ? String(params[name]) : whole,
  );

type Table = Record<string, string>;
const render = (table: Table, issue: Pick<HealthIssue, "titleKey" | "descriptionKey" | "params">) => ({
  title: fill(table[issue.titleKey] ?? issue.titleKey, issue.params ?? {}),
  description: fill(table[issue.descriptionKey] ?? issue.descriptionKey, issue.params ?? {}),
});

describe("document health findings as keys and parameters", () => {
  it("keeps the Turkish wording exactly as the screen said it before", () => {
    for (const [id, title, description, params] of OLD_TURKISH) {
      const issue = { ...issueKeys(id), params };
      expect(render(tr as Table, issue)).toEqual({
        title,
        description: fill(description, params),
      });
    }
  });

  it("gives every finding an English title and description, with no placeholder left", () => {
    for (const [id, , , params] of OLD_TURKISH) {
      const issue = { ...issueKeys(id), params };
      const text = render(en as Table, issue);
      expect(text.title).not.toContain("{");
      expect(text.description).not.toContain("{");
      // A missing key would render as the key itself.
      expect(text.title).not.toBe(issue.titleKey);
      expect(text.description).not.toBe(issue.descriptionKey);
    }
  });

  it("keeps both tables in step for every finding key", () => {
    for (const [id] of OLD_TURKISH) {
      const keys = issueKeys(id);
      for (const key of [keys.titleKey, keys.descriptionKey]) {
        expect(Object.hasOwn(en, key), `${key} missing in en`).toBe(true);
        expect(Object.hasOwn(tr, key), `${key} missing in tr`).toBe(true);
      }
    }
  });
});

/** The key pair `document-health.ts` builds for one finding. */
function issueKeys(stem: string): { titleKey: string; descriptionKey: string } {
  return { titleKey: `health.issue.${stem}Title`, descriptionKey: `health.issue.${stem}Body` };
}
