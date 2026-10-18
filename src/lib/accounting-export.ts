import type { UblInvoice, UblTaxSubtotal } from "./office-api";
import { defaultCellStyle, newSheet, newWorkbook, type Workbook } from "./office-types";

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
  /** Prepends UTF-8 BOM (\uFEFF) so Microsoft Excel on Windows opens Turkish characters cleanly (default: true). */
  includeBom?: boolean;
}

const UTF8_BOM = "\uFEFF";

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
 * Returns true when a UBL tax subtotal entry represents VAT (KDV) rather than
 * excise tax (ÖTV), special communication tax (ÖİV), or withholding (Tevkifat).
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

function getKdvSubtotals(inv: UblInvoice): Array<Pick<UblTaxSubtotal, "rate" | "taxable" | "tax">> {
  const kdvSubs = (inv.taxSubtotals || []).filter((sub) => isKdvSubtotal(sub.name));
  if (kdvSubs.length > 0) {
    return kdvSubs;
  }
  if (inv.lines && inv.lines.length > 0) {
    const byRate = new Map<number, { rate: number; taxable: number; tax: number }>();
    for (const line of inv.lines) {
      const rate = Math.round(line.vatRate || 0);
      const cur = byRate.get(rate) || { rate, taxable: 0, tax: 0 };
      cur.taxable += line.lineTotal || 0;
      cur.tax += line.vatAmount || 0;
      byRate.set(rate, cur);
    }
    return Array.from(byRate.values());
  }
  return [{ rate: 20, taxable: inv.taxExclusiveAmount || 0, tax: inv.taxAmount || 0 }];
}

function withBom(content: string, options: AccountingExportOptions): string {
  const includeBom = options.includeBom ?? true;
  return includeBom ? `${UTF8_BOM}${content}` : content;
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
    const subtotals = getKdvSubtotals(inv);

    subtotals.forEach((sub, idx) => {
      const rateKey = Math.round(sub.rate);
      const matrahCode = codes.matrahAccounts[rateKey] || (isSales ? `600.${rateKey}.001` : `153.${rateKey}.001`);
      const matrah = formatMoney(sub.taxable || 0);
      const kdv = formatMoney(sub.tax || 0);
      const rowWithholding = idx === 0 ? inv.withholdingAmount || 0 : 0;
      const rowTotal =
        subtotals.length === 1 ? inv.payableAmount || 0 : (sub.taxable || 0) + (sub.tax || 0) - rowWithholding;
      const desc = sanitizeText(`${docNo} nolu fatura - ${partyName}`);

      rows.push(
        `${date};${docNo};${ettn};${partyName};${taxId};${matrahCode};${matrah};%${rateKey};${kdv};${formatMoney(rowWithholding)};${formatMoney(rowTotal)};${desc}`,
      );
    });
  }

  return withBom(rows.join("\r\n"), options);
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
    const subtotals = getKdvSubtotals(inv);

    subtotals.forEach((sub, idx) => {
      const rate = Math.round(sub.rate);
      const matrah = formatMoney(sub.taxable || 0);
      const kdv = formatMoney(sub.tax || 0);
      const rowWithholding = idx === 0 ? inv.withholdingAmount || 0 : 0;
      const rowTotal =
        subtotals.length === 1 ? inv.payableAmount || 0 : (sub.taxable || 0) + (sub.tax || 0) - rowWithholding;

      rows.push(
        `"${docNo}","${date}","${ettn}","${partyName}","${taxId}",${matrah},${rate},${kdv},${formatMoney(rowWithholding)},${formatMoney(rowTotal)}`,
      );
    });
  }

  return withBom(rows.join("\r\n"), options);
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

    const subtotals = getKdvSubtotals(inv);

    for (const sub of subtotals) {
      const rate = Math.round(sub.rate);
      if (rate === 1) {
        m1 += sub.taxable || 0;
        k1 += sub.tax || 0;
      } else if (rate === 10 || rate === 8) {
        m10 += sub.taxable || 0;
        k10 += sub.tax || 0;
      } else if (rate === 20 || rate === 18) {
        m20 += sub.taxable || 0;
        k20 += sub.tax || 0;
      }
    }

    rows.push(
      `${date};${docNo};${ettn};${partyName};${taxId};${formatMoney(m1)};${formatMoney(k1)};${formatMoney(m10)};${formatMoney(k10)};${formatMoney(m20)};${formatMoney(k20)};${withholding};${total}`,
    );
  }

  return withBom(rows.join("\r\n"), options);
}

function escapeXml(value: string): string {
  return (value || "")
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&apos;");
}

/**
 * Logo GO / Tiger XML Fatura Aktarım Formatı
 */
export function exportLogoXml(invoices: UblInvoice[], options: AccountingExportOptions = {}): string {
  const isSales = options.transactionType !== "purchase";
  const rootTag = isSales ? "SALES_INVOICES" : "PURCHASE_INVOICES";
  const trCode = isSales ? 8 : 1;
  const items = invoices.map((inv) => {
    const party = isSales ? inv.customer : inv.supplier;
    const subtotals = getKdvSubtotals(inv);
    const transactions = subtotals
      .map(
        (sub) => `      <TRANSACTION>
        <TYPE>0</TYPE>
        <TOTAL>${formatMoney(sub.taxable || 0)}</TOTAL>
        <VAT_RATE>${Math.round(sub.rate)}</VAT_RATE>
        <VAT_AMOUNT>${formatMoney(sub.tax || 0)}</VAT_AMOUNT>
      </TRANSACTION>`,
      )
      .join("\n");
    return `  <INVOICE DBOP="INS">
    <TYPE>${trCode}</TYPE>
    <NUMBER>${escapeXml(inv.number || "")}</NUMBER>
    <DATE>${escapeXml(inv.issueDate || "")}</DATE>
    <GUID>${escapeXml(inv.ettn || "")}</GUID>
    <ARP_CODE>${escapeXml(party?.taxId || "")}</ARP_CODE>
    <CLIENT_TITLE>${escapeXml(party?.name || "")}</CLIENT_TITLE>
    <TOTAL_NET>${formatMoney(inv.payableAmount || 0)}</TOTAL_NET>
    <TOTAL_VAT>${formatMoney(inv.taxAmount || 0)}</TOTAL_VAT>
    <TRANSACTIONS>
${transactions}
    </TRANSACTIONS>
  </INVOICE>`;
  });

  return `<?xml version="1.0" encoding="UTF-8"?>\n<${rootTag}>\n${items.join("\n")}\n</${rootTag}>`;
}

/**
 * Muhasebe aktarım verisini doğrudan Excel (.xlsx) olarak kaydetmek için Calc Workbook modeli üretir.
 */
export function buildAccountingWorkbook(
  invoices: UblInvoice[],
  target: AccountingSoftware,
  options: AccountingExportOptions = {},
): Workbook {
  const csv = exportAccounting(invoices, target, { ...options, includeBom: false });
  const delimiter = target === "logo" ? "," : ";";
  const lines = csv.split(/\r?\n/).filter((l) => l.length > 0);
  const wb = newWorkbook(`${target.toUpperCase()} Muhasebe Aktarımı`);
  const sheet = newSheet(target.toUpperCase());
  sheet.freezeRows = 1;
  const colLetter = (idx: number) => String.fromCharCode(65 + idx);

  lines.forEach((line, rowIdx) => {
    const cols =
      delimiter === ","
        ? (line.match(/(".*?"|[^",\s]+)(?=\s*,|\s*$)/g) || []).map((c) => c.replace(/^"|"$/g, "").replace(/""/g, '"'))
        : line.split(";");
    cols.forEach((raw, colIdx) => {
      const addr = `${colLetter(colIdx)}${rowIdx + 1}`;
      const isHeader = rowIdx === 0;
      const num = !isHeader && /^-?\d+\.\d{2}$/.test(raw) ? Number(raw) : NaN;
      sheet.cells[addr] = {
        value: !Number.isNaN(num) ? { kind: "number", value: num } : { kind: "text", value: raw },
        formula: null,
        comment: null,
        link: null,
        style: {
          ...defaultCellStyle(),
          bold: isHeader,
          color: "#0F172A",
          fill: isHeader ? "#EEF2FF" : null,
          align: !Number.isNaN(num) ? "right" : "left",
          numberFormat: !Number.isNaN(num) ? "#,##0.00" : "General",
        },
      };
    });
  });

  wb.sheets = [sheet];
  return wb;
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
