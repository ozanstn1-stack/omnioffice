/**
 * Cell hyperlinks: the target allow-list (the same rule as `safe_link_target`
 * in the Rust model), internal targets, and the Insert link / Remove link
 * model operations.
 */
import { describe, expect, it } from "vitest";
import { emptyCell, newSheet, newWorkbook, type Workbook } from "../../lib/office-types";
import { applyCellEdit } from "./cells";
import {
  cellLinkTarget,
  clearCellLink,
  hyperlinkFormulaArgument,
  isExternalLink,
  linkKind,
  parsePlaceTarget,
  placeTarget,
  resolvePlaceTarget,
  safeLinkTarget,
  setCellLink,
  targetFromInput,
} from "./links";

function book(): Workbook {
  return { ...newWorkbook("Links"), sheets: [newSheet("Sheet1"), newSheet("My Sheet")] };
}

describe("the link target allow-list", () => {
  it("accepts web, mail and internal targets and trims them", () => {
    for (const allowed of [
      "https://example.org/report?a=1&b=2#top",
      "HTTP://EXAMPLE.ORG",
      "  https://example.org/padded  ",
      "mailto:ada@example.org?subject=Hi",
      "MAILTO:ada@example.org",
      "#Sheet2!A1",
      "#'My Sheet'!A1:B2",
      "#TaxRate",
    ]) {
      expect(safeLinkTarget(allowed), allowed).toBe(allowed.trim());
    }
  });

  it("refuses every other scheme, shares, relative paths, controls and absurd lengths", () => {
    for (const denied of [
      "",
      "   ",
      "#",
      "http://",
      "https:///",
      "mailto:",
      "file:///etc/passwd",
      "FILE:///C:/Windows/System32/calc.exe",
      "javascript:alert(1)",
      " javascript:alert(1)",
      "JaVaScRiPt:alert(1)",
      "vbscript:msgbox(1)",
      "data:text/html;base64,PHNjcmlwdD4=",
      "vnd.sun.star.script:Standard.Module1.Main?language=Basic&location=document",
      "ftp://example.org/file",
      "tel:+15551234567",
      "\\\\server\\share\\file.xlsx",
      "//server/share/file.xlsx",
      "C:\\Windows\\System32\\cmd.exe",
      "report.xlsx",
      "../secret.xlsx",
      "https://example.org/\r\nHost: evil",
      "https://example.org/\u0000",
      "https://example.org/\u0085",
      "\u0130http://example.org",
    ]) {
      expect(safeLinkTarget(denied), JSON.stringify(denied)).toBeNull();
    }
    expect(safeLinkTarget(`https://example.org/${"a".repeat(9_000)}`)).toBeNull();
    // The length is in bytes, like the Rust side: 3 000 three-byte characters are too long.
    expect(safeLinkTarget(`https://example.org/${"\u20ac".repeat(3_000)}`)).toBeNull();
    expect(safeLinkTarget(`https://example.org/${"\u20ac".repeat(100)}`)).not.toBeNull();
  });

  it("tells the kinds apart", () => {
    expect(linkKind("https://example.org")).toBe("web");
    expect(linkKind("MAILTO:a@b.c")).toBe("mail");
    expect(linkKind("#Sheet2!A1")).toBe("place");
    expect(isExternalLink("https://example.org")).toBe(true);
    expect(isExternalLink("#Sheet2!A1")).toBe(false);
  });

  it("completes what the user typed without opening a way around the list", () => {
    expect(targetFromInput("web", "example.org/a")).toBe("https://example.org/a");
    expect(targetFromInput("web", "http://example.org")).toBe("http://example.org");
    expect(targetFromInput("web", "ada@example.org")).toBe("mailto:ada@example.org");
    expect(targetFromInput("mail", "ada@example.org")).toBe("mailto:ada@example.org");
    expect(targetFromInput("mail", "MAILTO:ada@example.org")).toBe("MAILTO:ada@example.org");
    expect(targetFromInput("web", "   ")).toBe("");
    // A refused scheme stays as typed so the allow-list can refuse it.
    expect(safeLinkTarget(targetFromInput("web", "javascript:alert(1)"))).toBeNull();
    expect(safeLinkTarget(targetFromInput("web", "file:///etc/passwd"))).toBeNull();
  });
});

describe("internal targets", () => {
  it("writes a plain sheet name bare and quotes the rest", () => {
    expect(placeTarget("Sheet2", "b4")).toBe("#Sheet2!B4");
    expect(placeTarget("My Sheet", "A1")).toBe("#'My Sheet'!A1");
    expect(placeTarget("Bob's", "A1")).toBe("#'Bob''s'!A1");
  });

  it("reads the sheet and the reference back", () => {
    expect(parsePlaceTarget("#Sheet2!A1")).toEqual({ sheet: "Sheet2", reference: "A1" });
    expect(parsePlaceTarget("#'My Sheet'!A1:B2")).toEqual({ sheet: "My Sheet", reference: "A1:B2" });
    expect(parsePlaceTarget("#'Bob''s'!C3")).toEqual({ sheet: "Bob's", reference: "C3" });
    expect(parsePlaceTarget("#A1")).toEqual({ sheet: null, reference: "A1" });
    expect(parsePlaceTarget("#TaxRate")).toEqual({ sheet: null, reference: "TaxRate" });
    expect(parsePlaceTarget("#'open")).toBeNull();
    expect(parsePlaceTarget("#Sheet2!")).toBeNull();
    expect(parsePlaceTarget("https://example.org")).toBeNull();
  });

  it("round-trips through the quoting", () => {
    for (const name of ["Sheet2", "My Sheet", "Bob's", "Q1 'x' y"]) {
      expect(parsePlaceTarget(placeTarget(name, "A1"))).toEqual({ sheet: name, reference: "A1" });
    }
  });

  it("resolves a cell, a range start, a cell of the current sheet and a defined name", () => {
    const workbook: Workbook = {
      ...book(),
      names: [
        { name: "Rate", definition: "'My Sheet'!$B$2", sheet: null },
        { name: "Local", definition: "$C$3", sheet: null },
        { name: "Dead", definition: "Gone!A1", sheet: null },
      ],
    };
    expect(resolvePlaceTarget(workbook, 0, "#'My Sheet'!C5")).toEqual({ sheetIndex: 1, row: 4, col: 2 });
    expect(resolvePlaceTarget(workbook, 0, "#sheet1!$D$2:$E$4")).toEqual({ sheetIndex: 0, row: 1, col: 3 });
    expect(resolvePlaceTarget(workbook, 1, "#A2")).toEqual({ sheetIndex: 1, row: 1, col: 0 });
    expect(resolvePlaceTarget(workbook, 0, "#rate")).toEqual({ sheetIndex: 1, row: 1, col: 1 });
    expect(resolvePlaceTarget(workbook, 1, "#Local")).toEqual({ sheetIndex: 1, row: 2, col: 2 });
    expect(resolvePlaceTarget(workbook, 0, "#Dead")).toBeNull();
    expect(resolvePlaceTarget(workbook, 0, "#Nowhere!A1")).toBeNull();
    expect(resolvePlaceTarget(workbook, 0, "#Unknown")).toBeNull();
  });
});

describe("HYPERLINK formulas", () => {
  it("finds the first argument, however it is written", () => {
    expect(hyperlinkFormulaArgument('=HYPERLINK("https://example.org","Site")')).toBe('"https://example.org"');
    expect(hyperlinkFormulaArgument("=hyperlink(A1)")).toBe("A1");
    expect(hyperlinkFormulaArgument('=HYPERLINK("https://x.org/?a,b"&B2, "n")')).toBe('"https://x.org/?a,b"&B2');
    expect(hyperlinkFormulaArgument('=HYPERLINK(IF(A1>1,"https://a.org","https://b.org"),"go")')).toBe(
      'IF(A1>1,"https://a.org","https://b.org")',
    );
  });

  it("ignores every other formula", () => {
    expect(hyperlinkFormulaArgument("=SUM(A1:A3)")).toBeNull();
    expect(hyperlinkFormulaArgument('=IF(A1,HYPERLINK("https://example.org"),"")')).toBeNull();
    expect(hyperlinkFormulaArgument(null)).toBeNull();
    expect(hyperlinkFormulaArgument('=HYPERLINK("unterminated')).toBeNull();
  });
});

describe("Insert link and Remove link", () => {
  it("links a cell and keeps its text when the text is not changed", () => {
    let workbook = applyCellEdit(book(), 0, 0, 0, "Docs");
    workbook = setCellLink(workbook, 0, "A1", { target: " https://example.org ", text: null, tooltip: " Open docs " });
    const cell = workbook.sheets[0].cells.A1;
    expect(cell).toMatchObject({ link: "https://example.org", linkTooltip: "Open docs", linkDisplay: null });
    expect(cell.value).toEqual({ kind: "text", value: "Docs" });
    expect(cellLinkTarget(cell)).toBe("https://example.org");
  });

  it("replaces the content with the text to display, or shows the target when it is empty", () => {
    let workbook = applyCellEdit(book(), 0, 0, 0, "=1+1");
    workbook = setCellLink(workbook, 0, "A1", { target: "https://example.org", text: "Example", tooltip: "" });
    expect(workbook.sheets[0].cells.A1.formula).toBeNull();
    expect(workbook.sheets[0].cells.A1.value).toEqual({ kind: "text", value: "Example" });
    expect(workbook.sheets[0].cells.A1.linkTooltip).toBeNull();

    workbook = setCellLink(workbook, 0, "B2", { target: "mailto:ada@example.org", text: "", tooltip: "" });
    expect(workbook.sheets[0].cells.B2.value).toEqual({ kind: "text", value: "mailto:ada@example.org" });
  });

  it("links a formula cell without touching the formula when the text is left alone", () => {
    let workbook = applyCellEdit(book(), 0, 0, 0, "=2*3");
    workbook = setCellLink(workbook, 0, "A1", { target: "#Sheet1!B2", text: null, tooltip: "" });
    expect(workbook.sheets[0].cells.A1.formula).toBe("=2*3");
    expect(workbook.sheets[0].cells.A1.link).toBe("#Sheet1!B2");
  });

  it("refuses an unsafe target and leaves the workbook as it was", () => {
    const workbook = applyCellEdit(book(), 0, 0, 0, "x");
    for (const target of ["javascript:alert(1)", "file:///etc/passwd", "report.xlsx", ""]) {
      expect(setCellLink(workbook, 0, "A1", { target, text: "x", tooltip: "" })).toBe(workbook);
    }
  });

  it("does not change the original workbook or the other sheets", () => {
    const original = applyCellEdit(book(), 0, 0, 0, "x");
    const linked = setCellLink(original, 0, "A1", { target: "https://example.org", text: null, tooltip: "" });
    expect(original.sheets[0].cells.A1.link).toBeNull();
    expect(linked.sheets[1]).toBe(original.sheets[1]);
  });

  it("removes the link and keeps the text; a cell that held only the link disappears", () => {
    let workbook = applyCellEdit(book(), 0, 0, 0, "Docs");
    workbook = setCellLink(workbook, 0, "A1", { target: "https://example.org", text: null, tooltip: "tip" });
    const cleared = clearCellLink(workbook, 0, "A1");
    expect(cleared.sheets[0].cells.A1).toMatchObject({ link: null, linkTooltip: null, linkDisplay: null });
    expect(cleared.sheets[0].cells.A1.value).toEqual({ kind: "text", value: "Docs" });

    const bare: Workbook = {
      ...book(),
      sheets: [{ ...newSheet("Sheet1"), cells: { C3: { ...emptyCell(), link: "https://example.org" } } }],
    };
    expect(clearCellLink(bare, 0, "C3").sheets[0].cells.C3).toBeUndefined();
  });

  it("keeps a link-only cell when its value is cleared", () => {
    let workbook = applyCellEdit(book(), 0, 0, 0, "Docs");
    workbook = setCellLink(workbook, 0, "A1", { target: "https://example.org", text: null, tooltip: "" });
    workbook = applyCellEdit(workbook, 0, 0, 0, "");
    expect(workbook.sheets[0].cells.A1?.link).toBe("https://example.org");
  });

  it("leaves a cell without a link as it is when removing", () => {
    const workbook = applyCellEdit(book(), 0, 0, 0, "x");
    expect(clearCellLink(workbook, 0, "A1")).toBe(workbook);
    expect(clearCellLink(workbook, 0, "Z9")).toBe(workbook);
  });

  it("hides a stored link that fails the allow-list from the grid", () => {
    const workbook = applyCellEdit(book(), 0, 0, 0, "x");
    const hostile = { ...workbook.sheets[0].cells.A1, link: "javascript:alert(1)" };
    expect(cellLinkTarget(hostile)).toBeNull();
    expect(cellLinkTarget(undefined)).toBeNull();
  });
});
