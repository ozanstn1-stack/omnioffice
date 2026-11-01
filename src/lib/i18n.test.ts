import { describe, expect, it } from "vitest";
import { makeTranslate } from "./i18n";

// The dictionaries are primed by `src/test-setup.ts` before this module loads
// (in production they are JSON assets fetched on first use).
const en = makeTranslate("en");
const tr = makeTranslate("tr");

describe("i18n dictionaries", () => {
  it("resolves a known English key", () => {
    expect(en("common.cancel")).toBe("Cancel");
  });

  it("resolves a known Turkish key", () => {
    expect(tr("common.cancel")).toBe("İptal");
  });

  it("substitutes parameters", () => {
    expect(en("office.unsavedBody", { name: "Report.docx" })).toContain("Report.docx");
    expect(tr("office.unsavedBody", { name: "Rapor.docx" })).toContain("Rapor.docx");
  });

  it("falls back to English for a key Turkish is missing", () => {
    // Any key that only exists in English must still render something readable
    // rather than the raw key.
    const fallback = tr("settings.enginePdfium");
    expect(fallback).not.toBe("settings.enginePdfium");
  });

  it("returns the key itself when nothing matches, rather than undefined", () => {
    expect(en("definitely.not.a.key")).toBe("definitely.not.a.key");
  });

  it("resolves the Calc name-manager and print-layout keys in both languages", () => {
    for (const key of [
      "calc.nameManager",
      "calc.nameTarget",
      "calc.nameThisSheetOnly",
      "calc.noNames",
      "calc.printSetup",
      "calc.paperSize",
      "calc.landscape",
      "calc.printGridlines",
    ]) {
      expect(en(key), `en ${key}`).not.toBe(key);
      expect(tr(key), `tr ${key}`).not.toBe(key);
      // A Turkish value that is byte-identical to the English one means the
      // key was copied rather than translated.
      expect(tr(key) === en(key), `${key} is not translated`).toBe(false);
    }
  });

  it("resolves the version-history and unsaved-changes keys in both languages", () => {
    for (const key of ["office.versionHistory", "office.unsavedTitle", "office.closeKeep", "office.recoveryKept"]) {
      expect(en(key), `en ${key}`).not.toBe(key);
      expect(tr(key), `tr ${key}`).not.toBe(key);
    }
  });
});

describe("count-aware strings", () => {
  it("picks the singular and plural English forms", () => {
    expect(en.plural("mailMerge.done", 1, { count: 1 })).toBe("1 document produced");
    expect(en.plural("mailMerge.done", 3, { count: 3 })).toBe("3 documents produced");
    expect(en.plural("invoices.duplicatesFound", 1, { count: 1 })).toBe("1 duplicate invoice found!");
    expect(en.plural("invoices.duplicatesFound", 2, { count: 2 })).toBe("2 duplicate invoices found!");
  });

  it("keeps the Turkish sentence the same for every count", () => {
    // Turkish does not pluralise the noun after a numeral.
    expect(tr.plural("mailMerge.done", 1, { count: 1 })).toBe("1 belge üretildi");
    expect(tr.plural("mailMerge.done", 4, { count: 4 })).toBe("4 belge üretildi");
  });

  it("falls back to the base key when a string has no variants", () => {
    expect(en.plural("common.cancel", 2)).toBe("Cancel");
  });
});
