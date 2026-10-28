import { describe, expect, it } from "vitest";
import { auditPeriod, isValidTaxNumber, isValidVkn, likelyPeriod, numberGaps } from "./period-audit";
import type { UblInvoice, UblParty } from "./office-api";

const OWN = "1234567890";
const CLIENT = "10000000146";

const party = (taxId: string): UblParty => ({
  name: taxId,
  taxId,
  taxIdScheme: taxId.length === 11 ? "TCKN" : "VKN",
  taxOffice: "",
  address: "",
  city: "",
});

function invoice(number: string, patch: Partial<UblInvoice> & { taxable?: number; vat?: number } = {}): UblInvoice {
  const taxable = patch.taxable ?? 1000;
  const vat = patch.vat ?? 200;
  return {
    number,
    ettn: `ettn-${number}`,
    issueDate: "2026-09-10",
    issueTime: "10:00:00",
    typeCode: "SATIS",
    profile: "TICARIFATURA",
    currency: "TRY",
    supplier: party(OWN),
    customer: party(CLIENT),
    lines: [],
    taxSubtotals: [{ rate: 20, taxable, tax: vat, name: "KDV", exemptionReason: "" }],
    withholdingSubtotals: [],
    lineExtensionAmount: taxable,
    taxExclusiveAmount: taxable,
    taxInclusiveAmount: taxable + vat,
    allowanceTotal: 0,
    taxAmount: vat,
    withholdingAmount: 0,
    payableAmount: taxable + vat,
    notes: [],
    ...patch,
  };
}

const codes = (report: ReturnType<typeof auditPeriod>) => report.findings.map((finding) => finding.code);

describe("tax numbers", () => {
  it("checks the VKN check digit and the TCKN, and leaves an empty number alone", () => {
    expect(isValidVkn("1234567890")).toBe(true);
    expect(isValidVkn("1234567891")).toBe(false);
    expect(isValidVkn("123456789")).toBe(false);
    expect(isValidTaxNumber(CLIENT)).toBe(true);
    expect(isValidTaxNumber("10000000147")).toBe(false);
    expect(isValidTaxNumber("")).toBe(true);
  });
});

describe("numbering gaps", () => {
  it("lists the numbers missing inside each series and folds a long run", () => {
    expect(numberGaps(["ABC2026000000001", "ABC2026000000004", "XYZ2026000000010", "XYZ2026000000011"])).toEqual([
      { series: "ABC2026", missing: ["ABC2026000000002", "ABC2026000000003"] },
    ]);
    expect(numberGaps(["ABC2026000000001", "ABC2026000000100"])[0].missing).toEqual([
      "ABC2026000000002…ABC2026000000099",
    ]);
    expect(numberGaps(["not-a-gib-number", "ABC2026000000001"])).toEqual([]);
  });
});

describe("period close audit", () => {
  it("passes a clean month whose figures match the declaration", () => {
    const report = auditPeriod({
      invoices: [invoice("ABC2026000000001"), invoice("ABC2026000000002", { issueDate: "2026-09-11" })],
      period: "2026-09",
      ownTaxId: OWN,
      declared: { salesTaxable: 2000, salesVat: 400, purchasesVat: 0 },
    });
    expect(report.findings).toEqual([]);
    expect(report.inPeriod).toBe(2);
    expect(report.comparisons).toEqual([
      { field: "salesTaxable", computed: 2000, declared: 2000, difference: 0 },
      { field: "salesVat", computed: 400, declared: 400, difference: 0 },
      { field: "purchasesVat", computed: 0, declared: 0, difference: 0 },
    ]);
  });

  it("finds an out-of-period invoice, a duplicate ETTN, a sales gap and a bad tax number", () => {
    const report = auditPeriod({
      invoices: [
        invoice("ABC2026000000001"),
        invoice("ABC2026000000003"),
        invoice("ABC2026000000099", { ettn: "ettn-ABC2026000000001" }),
        invoice("ABC2026000000050", { issueDate: "2026-08-31" }),
        invoice("DEF2026000000001", { supplier: party("1234567891"), customer: party(OWN) }),
      ],
      period: "2026-09",
      ownTaxId: OWN,
    });
    const found = codes(report);
    expect(found).toEqual(expect.arrayContaining(["outOfPeriod", "duplicate", "numberGap", "invalidTaxId"]));
    expect(report.findings.find((finding) => finding.code === "outOfPeriod")?.items).toEqual(["ABC2026000000050"]);
    expect(report.findings.find((finding) => finding.code === "invalidTaxId")?.detail).toBe("1234567891");
    // Errors (duplicate ETTN, bad tax number) lead the list.
    expect(report.findings[0].severity).toBe("error");
  });

  it("reports a declaration that differs from the invoices beyond rounding", () => {
    const report = auditPeriod({
      invoices: [
        invoice("ABC2026000000001"),
        invoice("ZZZ2026000000001", { supplier: party(CLIENT), customer: party(OWN), taxable: 500, vat: 100 }),
      ],
      period: "2026-09",
      ownTaxId: OWN,
      declared: { salesVat: 200.4, purchasesVat: 90 },
    });
    expect(report.comparisons.find((row) => row.field === "salesVat")?.difference).toBe(-0.4);
    expect(report.comparisons.find((row) => row.field === "purchasesVat")?.difference).toBe(10);
    expect(report.findings).toEqual([{ code: "mismatch", severity: "error", items: [], detail: "purchasesVat" }]);
  });

  it("marks invoices that are neither a sale nor a purchase of the own VKN", () => {
    const report = auditPeriod({
      invoices: [invoice("QQQ2026000000001", { supplier: party(CLIENT), customer: party(CLIENT) })],
      period: "2026-09",
      ownTaxId: OWN,
    });
    expect(codes(report)).toEqual(["unclassified"]);
  });

  it("suggests the month most invoices fall in", () => {
    expect(likelyPeriod([invoice("A"), invoice("B"), invoice("C", { issueDate: "2026-10-01" })])).toBe("2026-09");
    expect(likelyPeriod([])).toBe("");
  });
});
