import { describe, expect, it, vi } from "vitest";
import type { Block, Sheet, TextDocument, Workbook } from "../lib/office-types";
import { computeWorkbookValues } from "./calc/cells";
import { isError } from "./calc/scalars";
import { orderTemplates, TEMPLATES, templatesFor, type OfficeTemplate } from "./templates";

const invoke = vi.fn(async (command: string, args?: { kind?: string; model?: unknown; path?: string }) => {
  if (command === "office_export_pdf") return { path: String(args?.path ?? ""), warnings: [] };
  return null;
});
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...(args as [string])) }));

import { exportPdf } from "../lib/office-api";

/**
 * The built-in templates seed real documents, so their contents are part of
 * the product contract: ids stay unique (React keys, tab identity), names are
 * present, and each build() returns the model kind its card promises.
 */
describe("built-in office templates", () => {
  it("has unique ids and complete metadata", () => {
    const ids = TEMPLATES.map((template) => template.id);
    expect(new Set(ids).size).toBe(ids.length);
    for (const template of TEMPLATES) {
      expect(template.name.trim().length).toBeGreaterThan(0);
      expect(template.description.trim().length).toBeGreaterThan(0);
      expect(["writer", "calc", "impress"]).toContain(template.kind);
    }
  });

  it("builds a model that matches the template kind", () => {
    for (const template of TEMPLATES) {
      const model = template.build();
      if (template.kind === "writer") {
        expect("blocks" in model).toBe(true);
      } else if (template.kind === "calc") {
        expect("sheets" in model).toBe(true);
      } else {
        expect("slides" in model).toBe(true);
      }
    }
  });

  it("filters by kind without dropping templates", () => {
    const writer = templatesFor("writer");
    const calc = templatesFor("calc");
    const impress = templatesFor("impress");
    expect(writer.length + calc.length + impress.length).toBe(TEMPLATES.length);
    expect(writer.every((template) => template.kind === "writer")).toBe(true);
    expect(calc.every((template) => template.kind === "calc")).toBe(true);
    expect(impress.every((template) => template.kind === "impress")).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// Turkish templates
// ---------------------------------------------------------------------------

function template(id: string): OfficeTemplate {
  const found = TEMPLATES.find((candidate) => candidate.id === id);
  if (!found) throw new Error(`missing template ${id}`);
  return found;
}

function blockText(block: Block): string {
  if (block.type === "paragraph") return block.runs.map((run) => run.text).join("");
  if (block.type === "table")
    return block.table.rows
      .map((row) => row.cells.map((cell) => cell.blocks.map(blockText).join(" ")).join(" | "))
      .join("\n");
  return "";
}

function documentText(id: string): string {
  return (template(id).build() as TextDocument).blocks.map(blockText).join("\n");
}

function headings(id: string): string[] {
  return (template(id).build() as TextDocument).blocks
    .filter((block) => block.type === "paragraph" && /^Heading\d$/.test(block.props.style))
    .map(blockText);
}

function columnNumber(letters: string): number {
  return [...letters].reduce((total, letter) => total * 26 + letter.charCodeAt(0) - 64, 0);
}

function columnLetters(column: number): string {
  let out = "";
  for (let value = column; value > 0; value = Math.floor((value - 1) / 26)) {
    out = String.fromCharCode(65 + ((value - 1) % 26)) + out;
  }
  return out;
}

/** Every cell a formula reads, with ranges expanded. */
function referencedCells(formula: string): string[] {
  const out: string[] = [];
  const source = formula.replace(/"[^"]*"/g, "");
  for (const match of source.matchAll(/\$?([A-Z]{1,3})\$?(\d+)(?::\$?([A-Z]{1,3})\$?(\d+))?/g)) {
    const [, fromColumn, fromRow, toColumn = fromColumn, toRow = fromRow] = match;
    for (let column = columnNumber(fromColumn); column <= columnNumber(toColumn); column += 1)
      for (let row = Number(fromRow); row <= Number(toRow); row += 1) out.push(`${columnLetters(column)}${row}`);
  }
  return out;
}

function sheetOf(id: string): { workbook: Workbook; sheet: Sheet } {
  const workbook = template(id).build() as Workbook;
  return { workbook, sheet: workbook.sheets[0] };
}

/** The official TCKN checksum, so a placeholder scan can prove none is real. */
function tcknIsValid(value: string): boolean {
  if (!/^[1-9]\d{10}$/.test(value)) return false;
  const digits = [...value].map(Number);
  const odd = digits[0] + digits[2] + digits[4] + digits[6] + digits[8];
  const even = digits[1] + digits[3] + digits[5] + digits[7];
  const tenth = (odd * 7 - even) % 10;
  const eleventh = digits.slice(0, 10).reduce((total, digit) => total + digit, 0) % 10;
  return digits[9] === tenth && digits[10] === eleventh;
}

/** The IBAN mod-97 check, so a placeholder scan can prove none is real. */
function ibanIsValid(value: string): boolean {
  const raw = value.replace(/\s/g, "").toUpperCase();
  if (!/^TR\d{24}$/.test(raw)) return false;
  const rearranged = raw.slice(4) + raw.slice(0, 4);
  const numeric = [...rearranged]
    .map((character) => (character >= "A" && character <= "Z" ? String(character.charCodeAt(0) - 55) : character))
    .join("");
  let remainder = 0;
  for (const character of numeric) remainder = (remainder * 10 + Number(character)) % 97;
  return remainder === 1;
}

function setNumber(sheet: Sheet, address: string, value: number) {
  const cell = sheet.cells[address];
  if (!cell || cell.formula) throw new Error(`${address} is not an input cell`);
  cell.value = { kind: "number", value };
}

describe("Turkish templates", () => {
  const turkish = TEMPLATES.filter((candidate) => candidate.language === "tr");

  it("ships the original five Turkish templates plus the roadmap's seven", () => {
    expect(turkish.map((candidate) => candidate.name)).toEqual(
      expect.arrayContaining([
        "Dilekçe",
        "Özgeçmiş",
        "Toplantı Tutanağı",
        "Fatura",
        "Bütçe Tablosu",
        "Resmî Yazı",
        "İhtarname",
        "Kira Sözleşmesi",
        "İzin Dilekçesi",
        "İstifa Dilekçesi",
        "Bordro",
        "İrsaliye",
      ]),
    );
    expect(turkish).toHaveLength(12);
    expect(turkish.map((candidate) => [candidate.name, candidate.kind])).toEqual(
      expect.arrayContaining([
        ["Resmî Yazı", "writer"],
        ["İhtarname", "writer"],
        ["Kira Sözleşmesi", "writer"],
        ["İzin Dilekçesi", "writer"],
        ["İstifa Dilekçesi", "writer"],
        ["Bordro", "calc"],
        ["İrsaliye", "writer"],
      ]),
    );
    for (const candidate of turkish) {
      expect(candidate.description, candidate.id).toMatch(/[çğıöşüİ]/);
      const model = candidate.build();
      expect(model.title).toBe(candidate.name);
      expect(candidate.kind === "writer" ? "blocks" in model : "sheets" in model).toBe(true);
    }
  });

  const ROADMAP_IDS = [
    "trOfficialLetter",
    "trNotice",
    "trRentContract",
    "trLeaveRequest",
    "trResignation",
    "trPayroll",
    "trDeliveryNote",
  ];

  it("uses {{alan}} placeholders in every roadmap template", () => {
    for (const id of ROADMAP_IDS) {
      const model = template(id).build();
      expect(model.title.trim().length, id).toBeGreaterThan(0);
      expect(JSON.stringify(model), id).toMatch(/\{\{[a-z_]+\}\}/);
    }
  });

  it("carries no real personal data: no valid TCKN, IBAN, phone or personal e-mail", () => {
    for (const candidate of TEMPLATES) {
      const json = JSON.stringify(candidate.build());
      // Whole 11-digit tokens only: a substring of a longer number (a ms
      // timestamp is 13 digits) is not a TCKN and must not be scanned, or the
      // check flakes when that slice happens to satisfy the checksum.
      for (const match of json.match(/\b\d{11}\b/g) ?? []) {
        expect(tcknIsValid(match), `${candidate.id} looks like a real TCKN: ${match}`).toBe(false);
      }
      for (const match of json.match(/TR\d{24}/gi) ?? []) {
        expect(ibanIsValid(match), `${candidate.id} looks like a real IBAN: ${match}`).toBe(false);
      }
      expect(json, candidate.id).not.toMatch(/\+90\s?5\d{2}/);
      for (const match of json.match(/[\w.+-]+@[\w-]+\.[\w.-]+/g) ?? []) {
        const domain = match.split("@")[1].toLowerCase();
        expect(
          /(^|\.)(example|ornek)\.(com|org|net)$/.test(domain) || domain.endsWith(".example"),
          `${candidate.id} uses a real e-mail domain: ${match}`,
        ).toBe(true);
      }
    }
  });

  it("lays out the petition: date top right, bold centred addressee, closing, signature and attachments", () => {
    const document = template("trPetition").build() as TextDocument;
    const paragraphs = document.blocks.filter((block) => block.type === "paragraph");
    const [date, addressee] = paragraphs;
    expect(date.props.align).toBe("right");
    expect(blockText(addressee)).toMatch(/MÜDÜRLÜĞÜNE$/);
    expect(addressee.props.align).toBe("center");
    expect(addressee.runs.every((run) => run.bold)).toBe(true);
    const text = documentText("trPetition");
    for (const part of ["Gereğini bilgilerinize arz ederim.", "Ad Soyad", "İmza", "Adres:", "Telefon:", "Ekler:"]) {
      expect(text).toContain(part);
    }
    // The attachments are a real numbered list.
    expect(paragraphs.filter((block) => block.props.list?.kind === "number").length).toBeGreaterThanOrEqual(2);
  });

  it("covers the CV and minutes sections", () => {
    expect(headings("trCv")).toEqual([
      "Kişisel Bilgiler",
      "Eğitim",
      "İş Deneyimi",
      "Yetenekler",
      "Diller",
      "Referanslar",
    ]);
    expect(headings("trMinutes")).toEqual(["Katılımcılar", "Gündem", "Alınan Kararlar", "İmzalar"]);
    const minutes = documentText("trMinutes");
    for (const part of ["Tarih:", "Saat:", "Yer:", "Karar | Sorumlu | Tarih"]) expect(minutes).toContain(part);
  });

  it("only references cells that exist and evaluates every formula without an error", () => {
    for (const id of ["trInvoice", "trBudget", "trPayroll"]) {
      const { workbook, sheet } = sheetOf(id);
      const formulas = Object.entries(sheet.cells).filter(([, cell]) => cell.formula);
      expect(formulas.length, id).toBeGreaterThan(0);
      for (const [address, cell] of formulas) {
        for (const reference of referencedCells(cell.formula ?? "")) {
          expect(sheet.cells[reference], `${id} ${address} ${cell.formula} -> ${reference}`).toBeDefined();
          expect(reference, `${id} ${address} refers to itself`).not.toBe(address);
        }
      }
      const values = computeWorkbookValues(workbook);
      for (const [address] of formulas) {
        const value = values.get(`${sheet.name}!${address}`);
        expect(isError(value), `${id} ${address} = ${String(value)}`).toBe(false);
        expect(typeof value, `${id} ${address}`).toBe("number");
      }
    }
  });

  it("computes the invoice subtotal, 20% VAT and grand total in Turkish lira", () => {
    const { workbook, sheet } = sheetOf("trInvoice");
    for (const header of ["Açıklama", "Miktar", "Birim Fiyat", "Tutar", "Ara Toplam", "KDV (%20)", "Genel Toplam"]) {
      expect(Object.values(sheet.cells).some((cell) => cell.value.kind === "text" && cell.value.value === header)).toBe(
        true,
      );
    }
    setNumber(sheet, "B13", 2);
    setNumber(sheet, "C13", 1250);
    setNumber(sheet, "B14", 3);
    setNumber(sheet, "C14", 100);
    const values = computeWorkbookValues(workbook);
    expect(values.get("Fatura!D13")).toBe(2500);
    expect(values.get("Fatura!D19")).toBe(2800);
    expect(values.get("Fatura!D20")).toBeCloseTo(560);
    expect(values.get("Fatura!D21")).toBeCloseTo(3360);
    for (const address of ["C13", "D13", "D19", "D20", "D21"]) {
      expect(sheet.cells[address].style.numberFormat, address).toContain("₺");
    }
  });

  it("totals the budget per month and for the year", () => {
    const { workbook, sheet } = sheetOf("trBudget");
    expect(sheet.cells.B1.value).toEqual({ kind: "text", value: "Ocak" });
    expect(sheet.cells.M1.value).toEqual({ kind: "text", value: "Aralık" });
    expect(sheet.cells.N1.value).toEqual({ kind: "text", value: "Yıllık Toplam" });
    setNumber(sheet, "B3", 30000);
    setNumber(sheet, "C3", 30000);
    setNumber(sheet, "B4", 5000);
    setNumber(sheet, "B10", 12000);
    setNumber(sheet, "C11", 8000);
    const values = computeWorkbookValues(workbook);
    const total = (label: string) => {
      const row = Object.entries(sheet.cells).find(
        ([address, cell]) => address.startsWith("A") && cell.value.kind === "text" && cell.value.value === label,
      )?.[0];
      if (!row) throw new Error(`no row ${label}`);
      return Number(row.slice(1));
    };
    const income = total("Toplam Gelir");
    const expenses = total("Toplam Gider");
    expect(values.get(`Bütçe!B${income}`)).toBe(35000);
    expect(values.get(`Bütçe!N3`)).toBe(60000);
    expect(values.get(`Bütçe!N${income}`)).toBe(65000);
    expect(values.get(`Bütçe!B${expenses}`)).toBe(12000);
    expect(values.get(`Bütçe!N${expenses}`)).toBe(20000);
    expect(values.get(`Bütçe!N${expenses + 2}`)).toBe(45000);
    expect(sheet.cells[`N${income}`].style.numberFormat).toContain("₺");
  });

  it("computes the payroll gross, deductions and net", () => {
    const { workbook, sheet } = sheetOf("trPayroll");
    setNumber(sheet, "B10", 30000);
    setNumber(sheet, "B11", 2000);
    setNumber(sheet, "B12", 1000);
    setNumber(sheet, "B13", 500);
    setNumber(sheet, "B14", 500);
    const values = computeWorkbookValues(workbook);
    expect(values.get("Bordro!B15")).toBe(34000);
    expect(values.get("Bordro!B19")).toBeCloseTo(4760);
    expect(values.get("Bordro!B20")).toBeCloseTo(340);
    expect(values.get("Bordro!B21")).toBeCloseTo(28900);
    expect(values.get("Bordro!B22")).toBeCloseTo(4335);
    expect(values.get("Bordro!B23")).toBeCloseTo(258.06);
    expect(values.get("Bordro!B24")).toBeCloseTo(9693.06);
    expect(values.get("Bordro!B26")).toBeCloseTo(24306.94);
    expect(sheet.cells.B26.style.numberFormat).toContain("₺");
  });

  it("hands every roadmap template to the PDF exporter as a plain model", async () => {
    for (const id of ROADMAP_IDS) {
      const candidate = template(id);
      const model = candidate.build();
      // The exporter receives JSON over the IPC boundary: it must be lossless.
      expect(JSON.parse(JSON.stringify(model)), id).toEqual(model);
      await exportPdf(candidate.kind, model, `C:/out/${id}.pdf`);
      expect(invoke).toHaveBeenLastCalledWith(
        "office_export_pdf",
        expect.objectContaining({ kind: candidate.kind, path: `C:/out/${id}.pdf` }),
      );
    }
  });

  it("lists Turkish templates first only for a Turkish interface", () => {
    const turkishFirst = orderTemplates(TEMPLATES, "tr");
    expect(turkishFirst.slice(0, turkish.length)).toEqual(turkish);
    expect(turkishFirst).toHaveLength(TEMPLATES.length);
    // The rest keeps the catalogue order.
    expect(turkishFirst.slice(turkish.length)).toEqual(TEMPLATES.filter((candidate) => candidate.language !== "tr"));
    expect(orderTemplates(TEMPLATES, "en")).toEqual(TEMPLATES);
    expect(orderTemplates(templatesFor("calc"), "tr")[0].name).toBe("Fatura");
    expect(orderTemplates(templatesFor("calc"), "en")[0].language).toBeUndefined();
  });
});
