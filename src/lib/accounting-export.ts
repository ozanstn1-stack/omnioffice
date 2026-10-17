import type { UblInvoice } from "./office-api";

export type AccountingSoftware = "luca" | "logo" | "zirve";

export interface AccountCodeMapping {
  /** Cari hesap kodu (varsayılan: 120.01 / 320.01) */
  cariAccount?: string;
  /** Matrah hesap kodları: %1, %10, %20 (varsayılan: 600.01/600.10/600.20 veya 153.01/153.10/153.20) */
  matrahAccounts?: Record<number, string>;
  /** KDV hesap kodları: %1, %10, %20 (varsayılan: 391.01/391.10/391.20 veya 191.01/191.10/191.20) */
  kdvAccounts?: Record<number, string>;
}

export interface AccountingExportOptions {
  transactionType?: "sales" | "purchase";
  accountCodes?: AccountCodeMapping;
}

const DEFAULT_SALES_CODES: Required<AccountCodeMapping> = {
  cariAccount: "120.01.001",
  matrahAccounts: { 1: "600.01.001", 10: "600.10.001", 20: "600.20.001" },
  kdvAccounts: { 1: "391.01.001", 10: "391.10.001", 20: "391.20.001" },
};

const DEFAULT_PURCHASE_CODES: Required<AccountCodeMapping> = {
  cariAccount: "320.01.001",
  matrahAccounts: { 1: "153.01.001", 10: "153.10.001", 20: "153.20.001" },
  kdvAccounts: { 1: "191.01.001", 10: "191.10.001", 20: "191.20.001" },
};

function formatMoney(value: number): string {
  return value.toFixed(2);
}

function sanitizeText(text: string): string {
  return (text || "").replace(/[\r\n;,"\t]/g, " ").trim();
}

/**
 * Luca Muhasebe CSV Dışa Aktarımı
 * Noktalı virgül (;) ile ayrılmış kolonlar.
 */
export function exportLuca(invoices: UblInvoice[], options: AccountingExportOptions = {}): string {
  const isSales = options.transactionType !== "purchase";
  const defaults = isSales ? DEFAULT_SALES_CODES : DEFAULT_PURCHASE_CODES;
  const codes = {
    cariAccount: options.accountCodes?.cariAccount || defaults.cariAccount,
    matrahAccounts: { ...defaults.matrahAccounts, ...options.accountCodes?.matrahAccounts },
    kdvAccounts: { ...defaults.kdvAccounts, ...options.accountCodes?.kdvAccounts },
  };

  const header =
    "Tarih;Belge No;ETTN;Cari Unvan;Vergi No;Hesap Kodu;Matrah;KDV Oranı;KDV Tutarı;Tevkifat Tutarı;Genel Toplam;Açıklama";
  const rows: string[] = [header];

  for (const inv of invoices) {
    const party = isSales ? inv.customer : inv.supplier;
    const date = inv.issueDate || "";
    const docNo = inv.number || "";
    const ettn = inv.ettn || "";
    const partyName = sanitizeText(party?.name || "");
    const taxId = sanitizeText(party?.taxId || "");
    const total = formatMoney(inv.payableAmount || 0);
    const withholding = formatMoney(inv.withholdingAmount || 0);

    // KDV oranlarına göre grupla
    const subtotals =
      inv.taxSubtotals && inv.taxSubtotals.length > 0
        ? inv.taxSubtotals
        : [{ rate: 20, taxable: inv.taxExclusiveAmount || 0, tax: inv.taxAmount || 0 }];

    for (const sub of subtotals) {
      const rateKey = Math.round(sub.rate);
      const matrahCode = codes.matrahAccounts[rateKey] || (isSales ? `600.${rateKey}.001` : `153.${rateKey}.001`);
      const matrah = formatMoney(sub.taxable || 0);
      const kdv = formatMoney(sub.tax || 0);
      const desc = sanitizeText(`${docNo} nolu fatura - ${partyName}`);

      rows.push(
        `${date};${docNo};${ettn};${partyName};${taxId};${matrahCode};${matrah};%${rateKey};${kdv};${withholding};${total};${desc}`,
      );
    }
  }

  return rows.join("\r\n");
}

/**
 * Logo Muhasebe CSV Dışa Aktarımı
 * Standart virgül ayrılmış, çift tırnaklı format.
 */
export function exportLogo(invoices: UblInvoice[], options: AccountingExportOptions = {}): string {
  const isSales = options.transactionType !== "purchase";
  const header =
    '"FATURA_NO","TARIH","ETTN","CARI_UNVAN","VERGI_NO","MATRAH","KDV_ORANI","KDV_TUTARI","TEVKIFAT","GENEL_TOPLAM"';
  const rows: string[] = [header];

  for (const inv of invoices) {
    const party = isSales ? inv.customer : inv.supplier;
    const date = inv.issueDate || "";
    const docNo = inv.number || "";
    const ettn = inv.ettn || "";
    const partyName = (party?.name || "").replace(/"/g, '""');
    const taxId = party?.taxId || "";
    const total = formatMoney(inv.payableAmount || 0);
    const withholding = formatMoney(inv.withholdingAmount || 0);

    const subtotals =
      inv.taxSubtotals && inv.taxSubtotals.length > 0
        ? inv.taxSubtotals
        : [{ rate: 20, taxable: inv.taxExclusiveAmount || 0, tax: inv.taxAmount || 0 }];

    for (const sub of subtotals) {
      const rate = Math.round(sub.rate);
      const matrah = formatMoney(sub.taxable || 0);
      const kdv = formatMoney(sub.tax || 0);

      rows.push(
        `"${docNo}","${date}","${ettn}","${partyName}","${taxId}",${matrah},${rate},${kdv},${withholding},${total}`,
      );
    }
  }

  return rows.join("\r\n");
}

/**
 * Zirve Müşavir CSV / Excel Şablonu
 * KDV oranları (%1, %10, %20) yatay kolonlar halinde açılır.
 */
export function exportZirve(invoices: UblInvoice[], options: AccountingExportOptions = {}): string {
  const isSales = options.transactionType !== "purchase";
  const header =
    "Tarih;Fatura No;ETTN;Cari Unvan;Vergi No;Matrah 1;KDV 1;Matrah 10;KDV 10;Matrah 20;KDV 20;Tevkifat;Genel Toplam";
  const rows: string[] = [header];

  for (const inv of invoices) {
    const party = isSales ? inv.customer : inv.supplier;
    const date = inv.issueDate || "";
    const docNo = inv.number || "";
    const ettn = inv.ettn || "";
    const partyName = sanitizeText(party?.name || "");
    const taxId = sanitizeText(party?.taxId || "");
    const total = formatMoney(inv.payableAmount || 0);
    const withholding = formatMoney(inv.withholdingAmount || 0);

    let m1 = 0,
      k1 = 0,
      m10 = 0,
      k10 = 0,
      m20 = 0,
      k20 = 0;

    const subtotals =
      inv.taxSubtotals && inv.taxSubtotals.length > 0
        ? inv.taxSubtotals
        : [{ rate: 20, taxable: inv.taxExclusiveAmount || 0, tax: inv.taxAmount || 0 }];

    for (const sub of subtotals) {
      const rate = Math.round(sub.rate);
      if (rate === 1) {
        m1 += sub.taxable || 0;
        k1 += sub.tax || 0;
      } else if (rate === 10 || rate === 8) {
        m10 += sub.taxable || 0;
        k10 += sub.tax || 0;
      } else {
        m20 += sub.taxable || 0;
        k20 += sub.tax || 0;
      }
    }

    rows.push(
      `${date};${docNo};${ettn};${partyName};${taxId};${formatMoney(m1)};${formatMoney(k1)};${formatMoney(m10)};${formatMoney(k10)};${formatMoney(m20)};${formatMoney(k20)};${withholding};${total}`,
    );
  }

  return rows.join("\r\n");
}

/**
 * Ana muhasebe formatlayıcı.
 */
export function exportAccounting(
  invoices: UblInvoice[],
  target: AccountingSoftware,
  options: AccountingExportOptions = {},
): string {
  switch (target) {
    case "luca":
      return exportLuca(invoices, options);
    case "logo":
      return exportLogo(invoices, options);
    case "zirve":
      return exportZirve(invoices, options);
    default:
      throw new Error(`Bilinmeyen muhasebe hedefi: ${target}`);
  }
}
