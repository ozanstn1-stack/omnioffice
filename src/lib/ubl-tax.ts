/**
 * UBL-TR tax helpers shared by the invoice screens and the accounting
 * exports. Kept neutral on purpose: the KDV declaration helper and the
 * duplicate check must not depend on `accounting-export`, whose vendor
 * exports are closed behind the `accountingVendorExport` flag until a real
 * import template verifies them (RELEASE_READINESS.md).
 */
import type { UblInvoice, UblTaxSubtotal } from "./office-api";

/**
 * Returns true when a UBL tax subtotal entry represents VAT (KDV) rather than
 * excise tax (ÖTV), special communication tax (ÖİV), or withholding
 * (Tevkifat).
 */
export function isKdvSubtotal(name?: string): boolean {
  const trimmed = (name || "").trim();
  if (!trimmed) return true;
  const upper = trimmed.toLocaleUpperCase("tr-TR");
  return (
    (upper.includes("KDV") || upper.includes("KATMA DEĞER") || upper.includes("KATMA DEGER")) &&
    !upper.includes("TEVKİFAT") &&
    !upper.includes("TEVKIFAT")
  );
}

/**
 * The document's KDV subtotals, falling back to per-line aggregation and then
 * to the document totals when the invoice carries no subtotal breakdown.
 */
export function getKdvSubtotals(inv: UblInvoice): Array<Pick<UblTaxSubtotal, "rate" | "taxable" | "tax">> {
  const kdvSubs = (inv.taxSubtotals || []).filter((sub) => isKdvSubtotal(sub.name));
  if (kdvSubs.length > 0) {
    return kdvSubs;
  }
  if (inv.lines && inv.lines.length > 0) {
    const byRate = new Map<number, { rate: number; taxable: number; tax: number }>();
    for (const line of inv.lines) {
      const rate = Math.round(line.vatRate || 0);
      const current = byRate.get(rate) || { rate, taxable: 0, tax: 0 };
      current.taxable += line.lineTotal || 0;
      current.tax += line.vatAmount || 0;
      byRate.set(rate, current);
    }
    return Array.from(byRate.values());
  }
  return [{ rate: 20, taxable: inv.taxExclusiveAmount || 0, tax: inv.taxAmount || 0 }];
}
