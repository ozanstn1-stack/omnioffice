import { describe, expect, it } from "vitest";

import {
  TASK_PACKS,
  buildTaskPackQuestion,
  extractJsonObject,
  getTaskPack,
  verifyTaskPackOutput,
} from "./ai-task-packs";

const goodPetition = JSON.stringify({
  summary: "Dilekçe, hitap ve tarih dışında uygun.",
  issues: [
    { severity: "medium", title: "Hitap eksik", suggestion: "İlgili makama hitap ekleyin." },
    { severity: "low", title: "Noktalama", suggestion: "Cümle sonlarına nokta koyun." },
  ],
  corrected: "T.C. ... Sayın İlgili Makam ...",
});

const goodVat = JSON.stringify({
  rows: [
    { rate: 20, base: 1000, vat: 200 },
    { rate: 10, base: 500, vat: 50 },
  ],
  totalBase: 1500,
  totalVat: 250,
});

const goodDeadline = JSON.stringify({
  serviceDate: "2026-10-01",
  periodDays: 14,
  lastDay: "2026-10-15",
  reasoning: "Tebliğden sonraki günden itibaren iki hafta.",
});

const goodContract = JSON.stringify({
  summary: "İki taraflı hizmet sözleşmesi.",
  clauses: [
    { heading: "1. Konu", summary: "Hizmetin kapsamını tanımlar." },
    { heading: "3. Bedel", summary: "Ödeme koşullarını belirler.", risk: "Ödeme tarihi belirsiz." },
  ],
});

const codes = (issues: Array<{ code: string }>) => issues.map((issue) => issue.code);

describe("TASK_PACKS", () => {
  it("ships the seven Turkish packs (C7 + D5) with unique ids", () => {
    expect(TASK_PACKS.map((pack) => pack.id)).toEqual([
      "duzelt-dilekce",
      "ozetle-sozlesme",
      "kdv-ozeti",
      "tebligat-sure",
      "efatura-analiz",
      "kep-tebligat",
      "mahkeme-dilekce",
    ]);
    expect(new Set(TASK_PACKS.map((pack) => pack.id)).size).toBe(7);
    for (const pack of TASK_PACKS) {
      expect(pack.prompt.length).toBeGreaterThan(40);
      expect(pack.fields.length).toBeGreaterThan(0);
    }
  });
});

describe("buildTaskPackQuestion", () => {
  it("asks for JSON only and shows the expected shape", () => {
    const question = buildTaskPackQuestion(getTaskPack("kdv-ozeti"));
    expect(question).toContain("Yanıtı SADECE");
    expect(question).toContain('"rows"');
    expect(question).toContain('"totalBase"');
    expect(question).toContain("YYYY-AA-GG");
    expect(question).toContain("nokta ondalıkla");
  });

  it("includes the pack's own instruction and is deterministic", () => {
    const pack = getTaskPack("tebligat-sure");
    const first = buildTaskPackQuestion(pack);
    expect(first).toContain("tebliğ tarihini");
    expect(buildTaskPackQuestion(pack)).toBe(first);
  });
});

describe("extractJsonObject", () => {
  it("finds the object inside fences and prose", () => {
    expect(extractJsonObject('İşte sonuç:\n```json\n{"a":1}\n```\nTeşekkürler')).toEqual({ a: 1 });
  });

  it("returns null for prose or broken JSON", () => {
    expect(extractJsonObject("JSON yok")).toBeNull();
    expect(extractJsonObject("{bozuk json}")).toBeNull();
    expect(extractJsonObject("[1,2,3]")).toBeNull();
  });
});

describe("verifyTaskPackOutput: düzelt dilekçe", () => {
  const pack = getTaskPack("duzelt-dilekce");

  it("accepts a complete answer", () => {
    const result = verifyTaskPackOutput(pack, goodPetition);
    expect(result.ok).toBe(true);
    expect(result.issues).toEqual([]);
    expect(result.parsed?.summary).toContain("Dilekçe");
  });

  it("reports a missing field, an empty text and a bad severity value", () => {
    expect(codes(verifyTaskPackOutput(pack, JSON.stringify({ issues: [], corrected: "x" })).issues)).toContain(
      "missingField",
    );
    const empty = verifyTaskPackOutput(pack, JSON.stringify({ summary: " ", issues: [], corrected: "x" }));
    expect(codes(empty.issues)).toContain("emptyField");
    const badSeverity = verifyTaskPackOutput(
      pack,
      JSON.stringify({
        summary: "x",
        issues: [{ severity: "acil", title: "x", suggestion: "y" }],
        corrected: "x",
      }),
    );
    expect(codes(badSeverity.issues)).toContain("wrongValue");
  });

  it("reports non-JSON output", () => {
    const result = verifyTaskPackOutput(pack, "Dilekçe güzel.");
    expect(result).toMatchObject({ ok: false, parsed: null });
    expect(codes(result.issues)).toEqual(["json"]);
  });
});

describe("verifyTaskPackOutput: sözleşme", () => {
  const pack = getTaskPack("ozetle-sozlesme");

  it("accepts an optional risk being absent", () => {
    expect(verifyTaskPackOutput(pack, goodContract).ok).toBe(true);
  });

  it("rejects an empty clause list", () => {
    const result = verifyTaskPackOutput(pack, JSON.stringify({ summary: "x", clauses: [] }));
    expect(result.ok).toBe(false);
    expect(codes(result.issues)).toContain("tooFew");
  });
});

describe("verifyTaskPackOutput: KDV", () => {
  const pack = getTaskPack("kdv-ozeti");

  it("accepts rows whose vat and totals add up", () => {
    expect(verifyTaskPackOutput(pack, goodVat).ok).toBe(true);
  });

  it("reports a row whose vat does not match base * rate", () => {
    const result = verifyTaskPackOutput(
      pack,
      JSON.stringify({ rows: [{ rate: 20, base: 1000, vat: 100 }], totalBase: 1000, totalVat: 100 }),
    );
    expect(result.ok).toBe(false);
    expect(codes(result.issues)).toContain("vatMath");
  });

  it("reports totals that do not add up and a rate out of range", () => {
    const totals = verifyTaskPackOutput(
      pack,
      JSON.stringify({ rows: [{ rate: 20, base: 1000, vat: 200 }], totalBase: 900, totalVat: 250 }),
    );
    expect(codes(totals.issues)).toContain("totalsMismatch");
    const rate = verifyTaskPackOutput(
      pack,
      JSON.stringify({ rows: [{ rate: 120, base: 100, vat: 120 }], totalBase: 100, totalVat: 120 }),
    );
    expect(codes(rate.issues)).toContain("range");
  });

  it("reports a wrong type in a row", () => {
    const result = verifyTaskPackOutput(
      pack,
      JSON.stringify({ rows: [{ rate: 20, base: "bin", vat: 200 }], totalBase: 0, totalVat: 200 }),
    );
    expect(codes(result.issues)).toContain("wrongType");
  });
});

describe("verifyTaskPackOutput: tebligat", () => {
  const pack = getTaskPack("tebligat-sure");

  it("accepts a deadline after the service date", () => {
    expect(verifyTaskPackOutput(pack, goodDeadline).ok).toBe(true);
  });

  it("reports a bad date format, an inverted range and a last day before service", () => {
    const format = verifyTaskPackOutput(
      pack,
      JSON.stringify({ serviceDate: "01.10.2026", periodDays: 14, lastDay: "2026-10-15", reasoning: "x" }),
    );
    expect(codes(format.issues)).toContain("dateFormat");
    const order = verifyTaskPackOutput(
      pack,
      JSON.stringify({ serviceDate: "2026-10-15", periodDays: 14, lastDay: "2026-10-01", reasoning: "x" }),
    );
    expect(codes(order.issues)).toContain("dateOrder");
    const range = verifyTaskPackOutput(
      pack,
      JSON.stringify({ serviceDate: "2026-10-01", periodDays: 0, lastDay: "2026-10-01", reasoning: "x" }),
    );
    expect(codes(range.issues)).toContain("range");
  });
});
