import { describe, expect, it } from "vitest";
import { detectDuplicateInvoices } from "./duplicate-invoices";
import type { UblInvoice } from "./office-api";

const invoice1: UblInvoice = {
  number: "GIB2026000000001",
  ettn: "uuid-1234-5678",
  issueDate: "2026-10-10",
  issueTime: "10:00:00",
  typeCode: "SATIS",
  profile: "TICARIFATURA",
  currency: "TRY",
  supplier: { name: "Satıcı A", taxId: "1234567890", taxIdScheme: "VKN", taxOffice: "", address: "", city: "" },
  customer: { name: "Alıcı B", taxId: "9876543210", taxIdScheme: "VKN", taxOffice: "", address: "", city: "" },
  lines: [],
  taxSubtotals: [],
  withholdingSubtotals: [],
  lineExtensionAmount: 1000,
  taxExclusiveAmount: 1000,
  taxInclusiveAmount: 1200,
  allowanceTotal: 0,
  taxAmount: 200,
  withholdingAmount: 0,
  payableAmount: 1200,
  notes: [],
};

const duplicateEttn: UblInvoice = {
  ...invoice1,
  number: "GIB2026000000099", // Different number, same ETTN
};

const duplicateNumber: UblInvoice = {
  ...invoice1,
  ettn: "uuid-different-9999", // Different ETTN, same number + supplier
};

const cleanInvoice: UblInvoice = {
  ...invoice1,
  number: "GIB2026000000002",
  ettn: "uuid-clean-8888",
  issueDate: "2026-10-11",
};

describe("Duplicate Invoice Detection (v5.5.0 - Issue #261)", () => {
  it("detects exact duplicate ETTNs and deduplicates them", () => {
    const result = detectDuplicateInvoices([invoice1, duplicateEttn, cleanInvoice]);
    expect(result.hasDuplicates).toBe(true);
    expect(result.totalDuplicates).toBe(1);
    expect(result.uniqueInvoices.length).toBe(2);
    expect(result.groups.some((g) => g.reason === "ettn")).toBe(true);
  });

  it("detects duplicate invoice number for the same supplier", () => {
    const result = detectDuplicateInvoices([invoice1, duplicateNumber]);
    expect(result.hasDuplicates).toBe(true);
    expect(result.totalDuplicates).toBe(1);
    expect(result.uniqueInvoices.length).toBe(1);
    expect(result.groups.some((g) => g.reason === "invoice_number")).toBe(true);
  });

  it("reports no duplicates when invoices are unique", () => {
    const result = detectDuplicateInvoices([invoice1, cleanInvoice]);
    expect(result.hasDuplicates).toBe(false);
    expect(result.totalDuplicates).toBe(0);
    expect(result.uniqueInvoices.length).toBe(2);
  });

  it("flags suspect identical supplier+date+amount invoices and does not collapse unnumbered invoices", () => {
    const suspectTwin: UblInvoice = {
      ...invoice1,
      number: "GIB2026000000077",
      ettn: "uuid-suspect-7777",
    };
    const suspectResult = detectDuplicateInvoices([invoice1, suspectTwin]);
    expect(suspectResult.groups.some((g) => g.reason === "identical_amount_date")).toBe(true);

    const unnumberedA: UblInvoice = { ...invoice1, number: "", ettn: "uuid-u1", issueDate: "2026-10-01" };
    const unnumberedB: UblInvoice = { ...invoice1, number: "", ettn: "uuid-u2", issueDate: "2026-10-02" };
    const unnumberedResult = detectDuplicateInvoices([unnumberedA, unnumberedB]);
    expect(unnumberedResult.uniqueInvoices.length).toBe(2);
  });
});
