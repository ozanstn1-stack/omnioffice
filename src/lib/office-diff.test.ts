import { describe, expect, it } from "vitest";

import {
  MAX_WRITER_CHANGES,
  diffDecks,
  diffIsEmpty,
  diffOfficeDocuments,
  diffSegments,
  diffTextDocuments,
  diffWorkbooks,
} from "./office-diff";
import {
  defaultParaProps,
  defaultRun,
  emptyCell,
  newDeck,
  newSlide,
  newSlideObject,
  newTextDocument,
  newWorkbook,
  type Block,
  type Slide,
  type TableData,
  type TextDocument,
} from "./office-types";

function paragraph(text: string): Block {
  return { type: "paragraph", props: defaultParaProps(), runs: [defaultRun(text)] };
}

function table(rows: string[][]): Block {
  const data: TableData = {
    rows: rows.map((cells, index) => ({
      cells: cells.map((text) => ({
        blocks: [paragraph(text)],
        colspan: 1,
        rowspan: 1,
        background: null,
        align: "left",
        valign: "top",
        widthPt: null,
      })),
      heightPt: null,
      header: index === 0,
    })),
    columnWidthsPt: [200, 200],
    borders: true,
    borderColor: "#000",
    align: "left",
  };
  return { type: "table", table: data };
}

function writer(texts: string[], extra: Block[] = []): TextDocument {
  const document = newTextDocument("test");
  document.blocks = [...texts.map(paragraph), ...extra];
  return document;
}

function slide(slideTitle: string, lines: string[] = []): Slide {
  const created = newSlide();
  const object = newSlideObject("text", 0, 0, 600, 100);
  object.text = {
    paragraphs: [slideTitle, ...lines].map((text) => ({
      text,
      level: 0,
      bold: false,
      italic: false,
      underline: false,
      sizePt: null,
      color: null,
      align: "left",
      bullet: false,
      runs: [],
    })),
    valign: "top",
    font: null,
    sizePt: null,
    color: null,
    align: "left",
  };
  created.objects = [object];
  return created;
}

describe("diffSegments", () => {
  it("marks the words that changed while keeping the rest", () => {
    const segments = diffSegments("Toplam 100 TL", "Toplam 120 TL");
    expect(segments.some((segment) => segment.kind === "removed" && segment.text.includes("100"))).toBe(true);
    expect(segments.some((segment) => segment.kind === "added" && segment.text.includes("120"))).toBe(true);
    expect(segments.map((segment) => segment.text).join("")).toContain("Toplam");
  });

  it("handles Turkish letters without splitting them", () => {
    const segments = diffSegments("İstanbul 3. Asliye", "İstanbul 4. Asliye");
    expect(segments.map((segment) => segment.text).join("")).toContain("İstanbul");
    expect(segments.some((segment) => segment.kind === "added" && segment.text.includes("4."))).toBe(true);
  });

  it("returns one same segment for equal text and nothing for empty input", () => {
    expect(diffSegments("aynı", "aynı")).toEqual([{ kind: "same", text: "aynı" }]);
    expect(diffSegments("", "")).toEqual([]);
  });
});

describe("diffTextDocuments", () => {
  it("pairs the paragraphs between anchors and marks them changed", () => {
    const base = writer(["Birinci", "İkinci", "Üçüncü"]);
    const current = writer(["Birinci", "İkinci güncel", "Dördüncü"]);
    const diff = diffTextDocuments(base, current);
    expect(diff.kind).toBe("writer");
    expect(diff.changes.every((change) => change.kind === "changed")).toBe(true);
    expect(diff.changes).toHaveLength(2);
    const changed = diff.changes.find((change) => change.text.includes("güncel"))!;
    expect(changed.segments?.map((segment) => segment.text).join("")).toContain("güncel");
    expect(diff.summary.changed).toBe(2);
  });

  it("reports a paragraph only in the base as removed and only in the current as added", () => {
    const removed = diffTextDocuments(writer(["A", "B"]), writer(["A"]));
    expect(removed.changes).toHaveLength(1);
    expect(removed.changes[0]).toMatchObject({ kind: "removed", text: "B" });
    const added = diffTextDocuments(writer(["A"]), writer(["A", "B"]));
    expect(added.changes).toHaveLength(1);
    expect(added.changes[0]).toMatchObject({ kind: "added", text: "B" });
  });

  it("marks table cells with their coordinates", () => {
    const base = writer(
      [],
      [
        table([
          ["Ad", "Tutar"],
          ["Ayşe", "100"],
        ]),
      ],
    );
    const current = writer(
      [],
      [
        table([
          ["Ad", "Tutar"],
          ["Ayşe", "120"],
        ]),
      ],
    );
    const diff = diffTextDocuments(base, current);
    expect(diff.changes).toHaveLength(1);
    expect(diff.changes[0]).toMatchObject({ kind: "changed", cell: { row: 2, col: 2 } });
  });

  it("includes header and footer paragraphs", () => {
    const base = writer(["Gövde"]);
    base.header = [paragraph("Eski başlık")];
    const current = writer(["Gövde"]);
    current.header = [paragraph("Yeni başlık")];
    const diff = diffTextDocuments(base, current);
    expect(diff.changes[0]).toMatchObject({ kind: "changed", area: "header" });
  });

  it("returns no changes for the same document", () => {
    const document = writer(["A", "B"]);
    expect(diffIsEmpty(diffTextDocuments(document, structuredClone(document)))).toBe(true);
  });

  it("bounds a document that differs everywhere", () => {
    const baseTexts = Array.from({ length: MAX_WRITER_CHANGES + 20 }, (_, index) => `eski satır ${index}`);
    const currentTexts = Array.from({ length: MAX_WRITER_CHANGES + 20 }, (_, index) => `yeni satır ${index}`);
    const diff = diffTextDocuments(writer(baseTexts), writer(currentTexts));
    expect(diff.truncated).toBe(true);
    expect(diff.changes).toHaveLength(MAX_WRITER_CHANGES);
  });
});

describe("diffWorkbooks", () => {
  it("reports changed cells, formulas and added/removed sheets", () => {
    const base = newWorkbook("test");
    base.sheets[0].name = "Veri";
    const a1 = emptyCell();
    a1.value = { kind: "text", value: "Ad" };
    base.sheets[0].cells = { A1: a1 };
    const current = structuredClone(base);
    const changed = emptyCell();
    changed.value = { kind: "text", value: "Ad Soyad" };
    current.sheets[0].cells.A1 = changed;
    const added = emptyCell();
    added.value = { kind: "text", value: "Yeni hücre" };
    current.sheets[0].cells.B2 = added;
    current.sheets.push({ ...structuredClone(current.sheets[0]), id: "sheet-2", name: "Yeni" });
    const diff = diffWorkbooks(base, current);
    expect(diff.sheetsAdded).toEqual(["Yeni"]);
    expect(diff.cells.some((cell) => cell.sheet === "Veri" && cell.address === "A1" && cell.kind === "changed")).toBe(
      true,
    );
    expect(diff.cells.some((cell) => cell.address === "B2" && cell.kind === "added")).toBe(true);
    expect(diff.summary.added).toBeGreaterThan(0);
  });

  it("compares a formula by its text", () => {
    const base = newWorkbook("test");
    const cell = emptyCell();
    cell.formula = "SUM(B1:B3)";
    cell.value = { kind: "number", value: 6 };
    base.sheets[0].cells = { A1: cell };
    const current = structuredClone(base);
    current.sheets[0].cells.A1.formula = "SUM(B1:B4)";
    const diff = diffWorkbooks(base, current);
    expect(diff.cells).toHaveLength(1);
    expect(diff.cells[0]).toMatchObject({ address: "A1", before: "=SUM(B1:B3)", after: "=SUM(B1:B4)" });
  });
});

describe("diffDecks", () => {
  it("reports a changed slide with word-level segments", () => {
    const base = newDeck("test");
    base.slides = [slide("Birinci"), slide("İkinci")];
    const current = newDeck("test");
    current.slides = [slide("Birinci"), slide("İkinci güncel")];
    const diff = diffDecks(base, current);
    expect(diff.slides).toHaveLength(1);
    expect(diff.slides[0].kind).toBe("changed");
    expect(diff.slides[0].segments?.map((segment) => segment.text).join("")).toContain("güncel");
  });

  it("reports added and removed slides by position", () => {
    const base = newDeck("test");
    base.slides = [slide("Birinci"), slide("İkinci")];
    const fewer = newDeck("test");
    fewer.slides = [slide("Birinci")];
    expect(diffDecks(base, fewer).slides.map((entry) => entry.kind)).toContain("removed");
    const more = newDeck("test");
    more.slides = [slide("Birinci"), slide("İkinci"), slide("Üçüncü")];
    expect(diffDecks(base, more).slides.map((entry) => entry.kind)).toContain("added");
  });

  it("returns empty for identical decks", () => {
    const deck = newDeck("test");
    deck.slides = [slide("Birinci")];
    expect(diffIsEmpty(diffDecks(deck, structuredClone(deck)))).toBe(true);
  });
});

describe("diffOfficeDocuments", () => {
  it("routes by document kind", () => {
    expect(diffOfficeDocuments("writer", writer(["A"]), writer(["B"])).kind).toBe("writer");
    expect(diffOfficeDocuments("calc", newWorkbook("a"), newWorkbook("b")).kind).toBe("calc");
    expect(diffOfficeDocuments("impress", newDeck("a"), newDeck("b")).kind).toBe("impress");
  });
});
