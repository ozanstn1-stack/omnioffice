import { readFileSync } from "node:fs";
import path from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { loadedDictionary, primeDictionary } from "./i18n-runtime";
import { useOfficeTabs } from "./office-store";
import type { Workbook } from "./office-types";
import { useSettings } from "./store";
import { DEFAULT_SETTINGS } from "./types";

// The 6.4.9 device check found a new spreadsheet named "Untitled spreadsheet"
// with a "Sheet1" tab in the Turkish interface.
const tr = JSON.parse(readFileSync(path.resolve(__dirname, "i18n", "tr.json"), "utf8")) as Record<string, string>;
const english = loadedDictionary("en");

describe("new office documents are named in the interface language", () => {
  afterEach(() => {
    useOfficeTabs.setState({ tabs: [], activeId: null } as never);
    useSettings.setState({ settings: { ...DEFAULT_SETTINGS }, loaded: true });
  });

  it("uses Turkish names when the interface is Turkish", () => {
    primeDictionary("tr", tr);
    useSettings.setState({ settings: { ...DEFAULT_SETTINGS, language: "tr" }, loaded: true });
    const created = (kind: "writer" | "calc" | "impress") => {
      const id = useOfficeTabs.getState().create(kind);
      return useOfficeTabs.getState().tabs.find((tab) => tab.id === id)!;
    };

    const calc = created("calc");
    expect(calc.title).toBe(tr["office.untitledSpreadsheet"]);
    expect((calc.model as Workbook).sheets[0].name).toBe(`${tr["calc.sheetBase"]}1`);
    expect(created("writer").title).toBe(tr["office.untitledDocument"]);
    expect(created("impress").title).toBe(tr["office.untitledPresentation"]);
    expect(tr["calc.sheetBase"]).toBe("Sayfa");
  });

  it("keeps the English names in English", () => {
    expect(english).not.toBeNull();
    const id = useOfficeTabs.getState().create("calc");
    const tab = useOfficeTabs.getState().tabs.find((candidate) => candidate.id === id)!;
    expect(tab.title).toBe("Untitled spreadsheet");
    expect((tab.model as Workbook).sheets[0].name).toBe("Sheet1");
  });
});
