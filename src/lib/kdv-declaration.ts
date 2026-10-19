import type { UblInvoice } from "./office-api";
import { isKdvSubtotal } from "./ubl-tax";

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
  /** Tevkifatsız standart satış/alış matrah ve KDV dağılımı */
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
  /** %0 / istisna kapsamındaki matrah */
  exemptionAmount: number;
  /** Satışlar (tedarikçi VKN'si "kendi VKN" olan faturalar). */
  sales: KdvSide;
  /** Alışlar (alıcı VKN'si "kendi VKN" olan faturalar): indirilecek KDV. */
  purchases: KdvSide;
  /** "Kendi VKN" ayarlanmadığında veya eşleşmeyen faturaların sayısı. */
  unclassified: number;
  /** Hesaplanan - indirilecek KDV (negatifse devreden KDV). */
  netVat: number;
}

export interface KdvSide {
  invoiceCount: number;
  taxableAmount: number;
  /** Satışta hesaplanan KDV, alışta indirilecek KDV. */
  vatAmount: number;
}

/** A period is `YYYY-MM`; anything else disables the filter instead of
 * silently matching by text prefix (`"2026-1"` used to catch 10/11/12). */
function matchesPeriod(issueDate: string, period: string): boolean {
  if (!period) return true;
  if (!/^\d{4}-\d{2}$/.test(period)) return true;
  return issueDate.slice(0, 7) === period;
}

export function generateKdvDeclarationReport(
  invoices: UblInvoice[],
  period: string = "",
  ownTaxId: string = "",
): KdvDeclarationReport {
  const filtered = invoices.filter((inv) => matchesPeriod(inv.issueDate || "", period));
  const own = ownTaxId.trim();
  const sales: KdvSide = { invoiceCount: 0, taxableAmount: 0, vatAmount: 0 };
  const purchases: KdvSide = { invoiceCount: 0, taxableAmount: 0, vatAmount: 0 };
  let unclassified = 0;

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

    // Sales and purchases: only decidable when the user's own tax id is set.
    const seller = (inv.supplier?.taxId || "").trim();
    const buyer = (inv.customer?.taxId || "").trim();
    const side = own && seller === own ? sales : own && buyer === own ? purchases : null;
    if (side) {
      side.invoiceCount += 1;
      side.taxableAmount += inv.taxExclusiveAmount || 0;
      side.vatAmount += inv.taxAmount || 0;
    } else {
      unclassified += 1;
    }

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
    sales: {
      invoiceCount: sales.invoiceCount,
      taxableAmount: Number(sales.taxableAmount.toFixed(2)),
      vatAmount: Number(sales.vatAmount.toFixed(2)),
    },
    purchases: {
      invoiceCount: purchases.invoiceCount,
      taxableAmount: Number(purchases.taxableAmount.toFixed(2)),
      vatAmount: Number(purchases.vatAmount.toFixed(2)),
    },
    unclassified,
    netVat: Number((sales.vatAmount - purchases.vatAmount).toFixed(2)),
  };
}
