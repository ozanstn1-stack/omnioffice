import { describe, expect, it } from "vitest";
import { generateKdvDeclarationReport } from "./kdv-declaration";
import type { UblInvoice } from "./office-api";

const mockInvoices: UblInvoice[] = [
  {
    number: "GIB2026000000001",
    ettn: "uuid-1",
    issueDate: "2026-10-10",
    issueTime: "10:00:00",
    typeCode: "SATIS",
    profile: "TICARIFATURA",
    currency: "TRY",
    supplier: { name: "A", taxId: "1", taxIdScheme: "VKN", taxOffice: "", address: "", city: "" },
    customer: { name: "B", taxId: "2", taxIdScheme: "VKN", taxOffice: "", address: "", city: "" },
    lines: [],
    taxSubtotals: [
      { rate: 20, taxable: 1000, tax: 200, name: "KDV", exemptionReason: "" },
      { rate: 10, taxable: 500, tax: 50, name: "KDV", exemptionReason: "" },
      { rate: 1, taxable: 100, tax: 1, name: "KDV", exemptionReason: "" },
    ],
    withholdingSubtotals: [{ rate: 50, taxable: 1000, tax: 100, name: "Tevkifat", exemptionReason: "" }],
    lineExtensionAmount: 1600,
    taxExclusiveAmount: 1600,
    taxInclusiveAmount: 1851,
    allowanceTotal: 0,
    taxAmount: 251,
    withholdingAmount: 100,
    payableAmount: 1751,
    notes: [],
  },
  {
    number: "GIB2026000000002",
    ettn: "uuid-2",
    issueDate: "2026-10-20",
    issueTime: "12:00:00",
    typeCode: "SATIS",
    profile: "TICARIFATURA",
    currency: "TRY",
    supplier: { name: "A", taxId: "1", taxIdScheme: "VKN", taxOffice: "", address: "", city: "" },
    customer: { name: "C", taxId: "3", taxIdScheme: "VKN", taxOffice: "", address: "", city: "" },
    lines: [],
    taxSubtotals: [{ rate: 20, taxable: 2000, tax: 400, name: "KDV", exemptionReason: "" }],
    withholdingSubtotals: [],
    lineExtensionAmount: 2000,
    taxExclusiveAmount: 2000,
    taxInclusiveAmount: 2400,
    allowanceTotal: 0,
    taxAmount: 400,
    withholdingAmount: 0,
    payableAmount: 2400,
    notes: [],
  },
];

describe("KDV Declaration Report (v5.5.0 - Issue #260)", () => {
  it("accurately summarizes VAT breakdown across rates 1%, 10%, and 20%", () => {
    const report = generateKdvDeclarationReport(mockInvoices);
    expect(report.invoiceCount).toBe(2);
    expect(report.rates[20].taxableAmount).toBe(3000);
    expect(report.rates[20].vatAmount).toBe(600);
    expect(report.rates[10].taxableAmount).toBe(500);
    expect(report.rates[10].vatAmount).toBe(50);
    expect(report.rates[1].taxableAmount).toBe(100);
    expect(report.rates[1].vatAmount).toBe(1);
    expect(report.totalTaxableAmount).toBe(3600);
    expect(report.totalCalculatedVat).toBe(651);
    expect(report.totalWithholdingVat).toBe(100);
  });

  it("filters correctly by period when specified", () => {
    const reportOct = generateKdvDeclarationReport(mockInvoices, "2026-10");
    expect(reportOct.invoiceCount).toBe(2);

    const reportNov = generateKdvDeclarationReport(mockInvoices, "2026-11");
    expect(reportNov.invoiceCount).toBe(0);
    expect(reportNov.totalTaxableAmount).toBe(0);
  });
});
