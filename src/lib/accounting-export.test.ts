import { describe, expect, it } from "vitest";
import { exportAccounting, exportLuca, exportLogo, exportZirve } from "./accounting-export";
import type { UblInvoice } from "./office-api";

const mockInvoices: UblInvoice[] = [
  {
    number: "GIB2026000000001",
    ettn: "a1b2c3d4-e5f6-7a8b-9c0d-1e2f3a4b5c6d",
    issueDate: "2026-10-15",
    issueTime: "14:30:00",
    typeCode: "SATIS",
    profile: "TICARIFATURA",
    currency: "TRY",
    supplier: {
      name: "Tedarikçi A.Ş.",
      taxId: "1234567890",
      taxIdScheme: "VKN",
      taxOffice: "Kadıköy",
      address: "İstanbul",
      city: "İstanbul",
    },
    customer: {
      name: "Müşteri Ltd. Şti.",
      taxId: "9876543210",
      taxIdScheme: "VKN",
      taxOffice: "Beşiktaş",
      address: "İstanbul",
      city: "İstanbul",
    },
    lines: [],
    taxSubtotals: [
      {
        rate: 20,
        taxable: 1000,
        tax: 200,
        name: "KDV",
        exemptionReason: "",
      },
      {
        rate: 10,
        taxable: 500,
        tax: 50,
        name: "KDV",
        exemptionReason: "",
      },
    ],
    withholdingSubtotals: [],
    lineExtensionAmount: 1500,
    taxExclusiveAmount: 1500,
    taxInclusiveAmount: 1750,
    allowanceTotal: 0,
    taxAmount: 250,
    withholdingAmount: 0,
    payableAmount: 1750,
    notes: [],
  },
];

describe("Accounting Export (v5.5.0 - Issue #259)", () => {
  it("exports Luca CSV format correctly with semicolon separation and account codes", () => {
    const csv = exportLuca(mockInvoices, { transactionType: "sales" });
    expect(csv).toContain("Tarih;Belge No;ETTN;Cari Unvan;Vergi No;Hesap Kodu;Matrah;KDV Oranı;KDV Tutarı");
    expect(csv).toContain("GIB2026000000001");
    expect(csv).toContain("Müşteri Ltd. Şti.");
    expect(csv).toContain("600.20.001;1000.00;%20;200.00");
    expect(csv).toContain("600.10.001;500.00;%10;50.00");
  });

  it("exports Logo CSV format with quotes and comma separation", () => {
    const csv = exportLogo(mockInvoices, { transactionType: "sales" });
    expect(csv).toContain('"FATURA_NO","TARIH","ETTN","CARI_UNVAN"');
    expect(csv).toContain('"GIB2026000000001","2026-10-15"');
    expect(csv).toContain("1000.00,20,200.00");
  });

  it("exports Zirve format with horizontal VAT columns", () => {
    const csv = exportZirve(mockInvoices, { transactionType: "sales" });
    expect(csv).toContain("Tarih;Fatura No;ETTN;Cari Unvan;Vergi No;Matrah 1;KDV 1;Matrah 10;KDV 10;Matrah 20;KDV 20");
    expect(csv).toContain("GIB2026000000001");
    expect(csv).toContain("500.00;50.00;1000.00;200.00");
  });

  it("delegates properly through exportAccounting wrapper", () => {
    const luca = exportAccounting(mockInvoices, "luca");
    const logo = exportAccounting(mockInvoices, "logo");
    const zirve = exportAccounting(mockInvoices, "zirve");
    expect(luca.startsWith("\uFEFF")).toBe(true);
    expect(luca).toContain("600.20.001");
    expect(logo).toContain('"FATURA_NO"');
    expect(zirve).toContain("Matrah 20");
  });

  it("excludes non-KDV subtotals (e.g. ÖTV) and does not duplicate payableAmount across multi-rate rows", () => {
    const invoiceWithOtv: UblInvoice = {
      ...mockInvoices[0],
      taxSubtotals: [
        { rate: 20, taxable: 1000, tax: 200, name: "KDV", exemptionReason: "" },
        { rate: 10, taxable: 500, tax: 50, name: "KDV", exemptionReason: "" },
        { rate: 20, taxable: 1000, tax: 300, name: "ÖTV", exemptionReason: "" },
      ],
      withholdingAmount: 40,
      payableAmount: 1710,
    };
    const lucaLines = exportLuca([invoiceWithOtv], { includeBom: false }).split("\r\n");
    // 1 header + 2 KDV rows (ÖTV excluded)
    expect(lucaLines.length).toBe(3);
    // First row has 1000 + 200 - 40 = 1160.00, second row has 500 + 50 - 0 = 550.00 -> sum = 1710.00
    expect(lucaLines[1]).toContain(";40.00;1160.00;");
    expect(lucaLines[2]).toContain(";0.00;550.00;");
  });
});
