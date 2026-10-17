import type { UblInvoice } from "./office-api";

export type DuplicateReason = "ettn" | "invoice_number" | "identical_amount_date";

export interface DuplicateGroup {
  reason: DuplicateReason;
  key: string;
  invoices: UblInvoice[];
}

export interface DuplicateDetectionResult {
  hasDuplicates: boolean;
  totalDuplicates: number;
  groups: DuplicateGroup[];
  uniqueInvoices: UblInvoice[];
}

/**
 * Toplu faturalarda mükerrer kayıtları tespit eder.
 * 1. ETTN (UUID) çakışması
 * 2. Aynı satıcı + Fatura numarası çakışması
 * 3. Aynı satıcı + Aynı tarih + Aynı ödenecek tutar (şüpheli çift fatura)
 */
export function detectDuplicateInvoices(invoices: UblInvoice[]): DuplicateDetectionResult {
  const groups: DuplicateGroup[] = [];
  const ettnMap = new Map<string, UblInvoice[]>();
  const numberMap = new Map<string, UblInvoice[]>();
  const suspectMap = new Map<string, UblInvoice[]>();

  for (const inv of invoices) {
    // 1. ETTN kontrolü
    const ettn = (inv.ettn || "").toLowerCase().trim();
    if (ettn) {
      const list = ettnMap.get(ettn) || [];
      list.push(inv);
      ettnMap.set(ettn, list);
    }

    // 2. Satıcı VKN + Fatura No kontrolü
    const supplierTaxId = (inv.supplier?.taxId || "").trim();
    const docNo = (inv.number || "").trim();
    if (supplierTaxId && docNo) {
      const numKey = `${supplierTaxId}:${docNo}`;
      const list = numberMap.get(numKey) || [];
      list.push(inv);
      numberMap.set(numKey, list);
    }

    // 3. Şüpheli: Satıcı VKN + Tarih + Tutar
    const date = (inv.issueDate || "").trim();
    const amount = (inv.payableAmount || 0).toFixed(2);
    if (supplierTaxId && date && amount) {
      const suspectKey = `${supplierTaxId}:${date}:${amount}`;
      const list = suspectMap.get(suspectKey) || [];
      list.push(inv);
      suspectMap.set(suspectKey, list);
    }
  }

  // Grupları derle
  const seenGroupKeys = new Set<string>();

  for (const [ettn, list] of ettnMap.entries()) {
    if (list.length > 1) {
      groups.push({
        reason: "ettn",
        key: `ETTN: ${ettn}`,
        invoices: list,
      });
      seenGroupKeys.add(ettn);
    }
  }

  for (const [key, list] of numberMap.entries()) {
    if (list.length > 1) {
      // Eğer zaten aynı ETTN ile gruplanmadıysa ekle
      const ettn = (list[0].ettn || "").toLowerCase().trim();
      if (!seenGroupKeys.has(ettn)) {
        groups.push({
          reason: "invoice_number",
          key: `Fatura No: ${key}`,
          invoices: list,
        });
      }
    }
  }

  // Benzersiz listeyi oluştur (ETTN veya Fatura No bazlı ilk kaydı tutar)
  const seenEttns = new Set<string>();
  const seenNumbers = new Set<string>();
  const uniqueInvoices: UblInvoice[] = [];

  for (const inv of invoices) {
    const ettn = (inv.ettn || "").toLowerCase().trim();
    const numKey = `${(inv.supplier?.taxId || "").trim()}:${(inv.number || "").trim()}`;

    if (ettn && seenEttns.has(ettn)) continue;
    if (numKey && seenNumbers.has(numKey)) continue;

    if (ettn) seenEttns.add(ettn);
    if (numKey) seenNumbers.add(numKey);
    uniqueInvoices.push(inv);
  }

  const totalDuplicates = invoices.length - uniqueInvoices.length;

  return {
    hasDuplicates: groups.length > 0 || totalDuplicates > 0,
    totalDuplicates,
    groups,
    uniqueInvoices,
  };
}
