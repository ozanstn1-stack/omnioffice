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

  it("ignores non-KDV subtotals such as ÖTV and computes declaredVatAmount on withholdings", () => {
    const invoiceWithOtv: UblInvoice = {
      ...mockInvoices[0],
      taxSubtotals: [
        { rate: 20, taxable: 1000, tax: 200, name: "KDV", exemptionReason: "" },
        { rate: 50, taxable: 1000, tax: 500, name: "ÖTV", exemptionReason: "" },
      ],
    };
    const report = generateKdvDeclarationReport([invoiceWithOtv]);
    expect(report.totalTaxableAmount).toBe(1000);
    expect(report.totalCalculatedVat).toBe(200);
    expect(report.withholdings[0].declaredVatAmount).toBe(900);
  });
});

describe("KDV sales/purchase split and period strictness", () => {
  const purchase: UblInvoice = {
    ...mockInvoices[0],
    number: "GIB2026000000003",
    ettn: "uuid-3",
    // Own tax id (1) is the buyer here: a purchase with 300 VAT.
    supplier: { name: "Tedarik�i", taxId: "9", taxIdScheme: "VKN", taxOffice: "", address: "", city: "" },
    customer: { name: "Ben", taxId: "1", taxIdScheme: "VKN", taxOffice: "", address: "", city: "" },
    taxSubtotals: [{ rate: 20, taxable: 1500, tax: 300, name: "KDV", exemptionReason: "" }],
    withholdingSubtotals: [],
    taxExclusiveAmount: 1500,
    taxAmount: 300,
    withholdingAmount: 0,
    payableAmount: 1800,
  };

  it("splits sales and purchases by the configured own tax id", () => {
    const report = generateKdvDeclarationReport([...mockInvoices, purchase], "", "1");
    expect(report.sales.invoiceCount).toBe(2);
    expect(report.sales.vatAmount).toBe(651);
    expect(report.purchases.invoiceCount).toBe(1);
    expect(report.purchases.taxableAmount).toBe(1500);
    expect(report.purchases.vatAmount).toBe(300);
    expect(report.netVat).toBe(351);
    expect(report.unclassified).toBe(0);
  });

  it("counts everything as unclassified when no own tax id is set", () => {
    const report = generateKdvDeclarationReport([...mockInvoices, purchase]);
    expect(report.sales.invoiceCount).toBe(0);
    expect(report.purchases.invoiceCount).toBe(0);
    expect(report.unclassified).toBe(3);
  });

  it("treats an incomplete period as no filter instead of a text prefix", () => {
    // "2026-1" used to match 2026-10/11/12 through startsWith.
    expect(generateKdvDeclarationReport(mockInvoices, "2026-1").invoiceCount).toBe(2);
    expect(generateKdvDeclarationReport(mockInvoices, "2026-11").invoiceCount).toBe(0);
  });
});
