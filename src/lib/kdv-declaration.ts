import type { UblInvoice } from "./office-api";
import { isKdvSubtotal } from "./accounting-export";

export interface VatRateBreakdown {
  rate: number;
  taxableAmount: number;
  vatAmount: number;
}

export interface WithholdingBreakdown {
  rate: number;
  taxableAmount: number;
  withholdingAmount: number;
  declaredVatAmount: number;
}

export interface KdvDeclarationReport {
  period: string;
  invoiceCount: number;
  /** Tevkifatsız standart satış/alım matrah ve KDV dağılımı */
  rates: Record<number, VatRateBreakdown>;
  /** Tevkifatlı işlemler dökümü */
  withholdings: WithholdingBreakdown[];
  /** Toplam matrah */
  totalTaxableAmount: number;
  /** Toplam hesaplanan KDV */
  totalCalculatedVat: number;
  /** Toplam tevkif edilen KDV */
  totalWithholdingVat: number;
  /** Net ödenecek/tahsil edilecek genel toplam */
  totalPayableAmount: number;
  /** %0 / İstisna kapsamındaki matrah */
  exemptionAmount: number;
}

export function generateKdvDeclarationReport(invoices: UblInvoice[], period: string = ""): KdvDeclarationReport {
  const filtered = period ? invoices.filter((inv) => (inv.issueDate || "").startsWith(period)) : invoices;

  const rates: Record<number, VatRateBreakdown> = {
    1: { rate: 1, taxableAmount: 0, vatAmount: 0 },
    10: { rate: 10, taxableAmount: 0, vatAmount: 0 },
    20: { rate: 20, taxableAmount: 0, vatAmount: 0 },
  };

  const withholdingsMap = new Map<number, WithholdingBreakdown>();
  let totalTaxableAmount = 0;
  let totalCalculatedVat = 0;
  let totalWithholdingVat = 0;
  let totalPayableAmount = 0;
  let exemptionAmount = 0;

  for (const inv of filtered) {
    totalPayableAmount += inv.payableAmount || 0;
    totalWithholdingVat += inv.withholdingAmount || 0;

    const kdvSubs = (inv.taxSubtotals || []).filter((sub) => isKdvSubtotal(sub.name));
    const subtotals =
      kdvSubs.length > 0 ? kdvSubs : [{ rate: 20, taxable: inv.taxExclusiveAmount || 0, tax: inv.taxAmount || 0 }];

    for (const sub of subtotals) {
      const rate = Math.round(sub.rate);
      const taxable = sub.taxable || 0;
      const tax = sub.tax || 0;

      totalTaxableAmount += taxable;
      totalCalculatedVat += tax;

      if (rate === 0) {
        exemptionAmount += taxable;
      } else if (rate === 1) {
        rates[1].taxableAmount += taxable;
        rates[1].vatAmount += tax;
      } else if (rate === 10 || rate === 8) {
        rates[10].taxableAmount += taxable;
        rates[10].vatAmount += tax;
      } else if (rate === 20 || rate === 18) {
        rates[20].taxableAmount += taxable;
        rates[20].vatAmount += tax;
      } else {
        if (!rates[rate]) {
          rates[rate] = { rate, taxableAmount: 0, vatAmount: 0 };
        }
        rates[rate].taxableAmount += taxable;
        rates[rate].vatAmount += tax;
      }
    }

    if (inv.withholdingSubtotals && inv.withholdingSubtotals.length > 0) {
      for (const w of inv.withholdingSubtotals) {
        const rate = Math.round(w.rate);
        const taxable = w.taxable || 0;
        const wAmount = w.tax || 0;
        const existing = withholdingsMap.get(rate) || {
          rate,
          taxableAmount: 0,
          withholdingAmount: 0,
          declaredVatAmount: 0,
        };
        existing.taxableAmount += taxable;
        existing.withholdingAmount += wAmount;
        existing.declaredVatAmount += Math.max(0, taxable - wAmount);
        withholdingsMap.set(rate, existing);
      }
    }
  }

  return {
    period: period || "Tüm Dönemler",
    invoiceCount: filtered.length,
    rates,
    withholdings: Array.from(withholdingsMap.values()),
    totalTaxableAmount: Number(totalTaxableAmount.toFixed(2)),
    totalCalculatedVat: Number(totalCalculatedVat.toFixed(2)),
    totalWithholdingVat: Number(totalWithholdingVat.toFixed(2)),
    totalPayableAmount: Number(totalPayableAmount.toFixed(2)),
    exemptionAmount: Number(exemptionAmount.toFixed(2)),
  };
}
