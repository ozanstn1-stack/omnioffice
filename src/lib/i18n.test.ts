import { beforeAll, describe, expect, it } from "vitest";
import { ensureLanguage, makeTranslate } from "./i18n";

const en = makeTranslate("en");
// Turkish is a lazily loaded chunk, so load it before resolving its keys.
let tr: ReturnType<typeof makeTranslate>;
beforeAll(async () => {
  await ensureLanguage("tr");
  tr = makeTranslate("tr");
});

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
