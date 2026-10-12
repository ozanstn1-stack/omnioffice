import { describe, expect, it } from "vitest";
import { compareQrWithInvoice, normalizeDate, normalizeNumber } from "./invoice-qr";
import type { GibQr, UblInvoice } from "./office-api";

function qr(overrides: Partial<GibQr> = {}): GibQr {
  return {
    vknTckn: "1111111111",
    avknTckn: "2222222222",
    senaryo: "TEMELFATURA",
    tip: "SATIS",
    tarih: "2026-03-14",
    no: "GIB2026000000001",
    ettn: "04e26a62-7c00-46d0-878c-6f7c60834525",
    paraBirimi: "TRY",
    malHizmetToplam: "1000.00",
    vergiDahil: "1200.00",
    odenecek: "1200.00",
    kdvMatrah: { "20": "1000.00" },
    hesaplananKdv: { "20": "200.00" },
    ...overrides,
  };
}

function invoice(overrides: Partial<UblInvoice> = {}): UblInvoice {
  return {
    number: "GIB2026000000001",
    ettn: "04e26a62-7c00-46d0-878c-6f7c60834525",
    issueDate: "2026-03-14",
    issueTime: "11:42:00",
    typeCode: "SATIS",
    profile: "TEMELFATURA",
    currency: "TRY",
    supplier: { name: "Satıcı", taxId: "1111111111", taxIdScheme: "VKN", taxOffice: "", address: "", city: "" },
    customer: { name: "Alıcı", taxId: "2222222222", taxIdScheme: "VKN", taxOffice: "", address: "", city: "" },
    lines: [],
    taxSubtotals: [{ rate: 20, taxable: 1000, tax: 200, name: "KDV", exemptionReason: "" }],
    withholdingSubtotals: [],
    lineExtensionAmount: 1000,
    taxExclusiveAmount: 1000,
    taxInclusiveAmount: 1200,
    allowanceTotal: 0,
    taxAmount: 200,
    withholdingAmount: 0,
    payableAmount: 1200,
    notes: [],
    ...overrides,
  };
}

describe("normalization", () => {
  it("accepts the date shapes GİB and UBL use", () => {
    expect(normalizeDate("2026-03-14")).toBe("2026-03-14");
    expect(normalizeDate("14.03.2026")).toBe("2026-03-14");
    expect(normalizeDate("14/03/2026")).toBe("2026-03-14");
  });

  it("parses Turkish and English amounts", () => {
    expect(normalizeNumber("1.250,15")).toBe(1250.15);
    expect(normalizeNumber("1,250.15")).toBe(1250.15);
    expect(normalizeNumber("1200.00")).toBe(1200);
    expect(normalizeNumber("abc")).toBeNull();
  });
});

describe("compareQrWithInvoice", () => {
  it("reports no mismatch when the karekod matches the document", () => {
    expect(compareQrWithInvoice(qr(), invoice())).toEqual([]);
  });

  it("tolerates a differently formatted date and amount", () => {
    const mismatches = compareQrWithInvoice(qr({ tarih: "14.03.2026", odenecek: "1.200,00" }), invoice());
    expect(mismatches).toEqual([]);
  });

  it("flags a disagreeing payable amount, date and invoice number", () => {
    const mismatches = compareQrWithInvoice(
      qr({ odenecek: "1500.00", tarih: "2026-03-15", no: "GIB2026000000009" }),
      invoice(),
    );
    expect(mismatches.map((mismatch) => mismatch.field).sort()).toEqual(["no", "odenecek", "tarih"]);
  });

  it("flags a VAT rate the document does not carry", () => {
    const mismatches = compareQrWithInvoice(qr({ kdvMatrah: { "10": "1000.00" } }), invoice());
    expect(mismatches.map((mismatch) => mismatch.field)).toContain("kdvmatrah(10)");
  });

  it("flags a disagreeing sender tax id", () => {
    const mismatches = compareQrWithInvoice(qr({ vknTckn: "9999999999" }), invoice());
    expect(mismatches.map((mismatch) => mismatch.field)).toContain("vkntckn");
  });
});
