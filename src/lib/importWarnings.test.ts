import { describe, expect, it } from "vitest";
import { makeTranslate } from "./i18n";
import { localizeImportWarnings, splitImportWarnings } from "./importWarnings";

// The exact text the Rust importers produce (xlsx.rs `import_limit_warning`).
const LIMIT = 'Import limit: sheet "Log" has cells beyond row 100000 or column 1000; they were not imported.';

describe("import limit warnings", () => {
  it("splits the budget notice from the other notes", () => {
    const split = splitImportWarnings(["Macros were not loaded.", LIMIT, "Cell formatting is limited."]);
    expect(split.limits).toEqual([{ sheet: "Log", rows: 100000, cols: 1000 }]);
    expect(split.notes).toEqual(["Macros were not loaded.", "Cell formatting is limited."]);
  });

  it("keeps sheet names with quotes, semicolons and non-ASCII letters", () => {
    const name = 'Q1 "özet"; 2025';
    const warning = `Import limit: sheet "${name}" has cells beyond row 100000 or column 1000; they were not imported.`;
    expect(splitImportWarnings([warning]).limits[0].sheet).toBe(name);
  });

  it("leaves an unrecognised warning untouched", () => {
    const odd = "Import limit: something else entirely.";
    const split = splitImportWarnings([odd]);
    expect(split.limits).toEqual([]);
    expect(split.notes).toEqual([odd]);
  });

  it("translates the notice into English and Turkish with the same placeholders", () => {
    const en = localizeImportWarnings([LIMIT], makeTranslate("en"));
    const tr = localizeImportWarnings([LIMIT], makeTranslate("tr"));
    expect(en).toHaveLength(1);
    expect(en[0]).toContain('"Log"');
    expect(en[0]).toContain("100000");
    expect(en[0]).toContain("1000");
    expect(en[0]).not.toMatch(/\{\w+\}/);
    expect(tr[0]).toContain('"Log"');
    expect(tr[0]).toContain("100000");
    expect(tr[0]).not.toMatch(/\{\w+\}/);
    expect(tr[0]).not.toBe(en[0]);
  });

  it("puts the translated notice before the other notes", () => {
    const shown = localizeImportWarnings(["Note one.", LIMIT], makeTranslate("en"));
    expect(shown[0]).toContain('"Log"');
    expect(shown[1]).toBe("Note one.");
  });
});
