import type { GibQr, UblInvoice } from "./office-api";

/** One field whose GİB karekod value disagrees with the invoice document. */
export interface QrMismatch {
  field: string;
  qr: string;
  invoice: string;
}

/** `2026-03-14`, `14.03.2026` and `14/03/2026` all become `2026-03-14`. */
export function normalizeDate(value: string): string {
  const trimmed = value.trim();
  if (/^\d{4}-\d{2}-\d{2}$/.test(trimmed)) return trimmed;
  const match = trimmed.match(/^(\d{1,2})[./-](\d{1,2})[./-](\d{4})$/);
  if (match) {
    const [, day, month, year] = match;
    return `${year}-${month.padStart(2, "0")}-${day.padStart(2, "0")}`;
  }
  return trimmed;
}

/** Parses `1.250,15`, `1,250.15` or `1250.15`; `null` when it is not a number. */
export function normalizeNumber(value: string): number | null {
  const trimmed = value.trim().replace(/\s/g, "");
  if (!trimmed) return null;
  const lastDot = trimmed.lastIndexOf(".");
  const lastComma = trimmed.lastIndexOf(",");
  let normalized = trimmed;
  if (lastDot >= 0 && lastComma >= 0) {
    // The rightmost separator is the decimal mark, the other one is grouping.
    normalized = lastDot > lastComma ? trimmed.replace(/,/g, "") : trimmed.replace(/\./g, "").replace(",", ".");
  } else if (lastComma >= 0) {
    // A single comma followed by one or two digits is a decimal mark.
    normalized = /,\d{1,2}$/.test(trimmed) ? trimmed.replace(",", ".") : trimmed.replace(/,/g, "");
  }
  const parsed = Number(normalized);
  return Number.isNaN(parsed) ? null : parsed;
}

function push(out: QrMismatch[], field: string, qr: string, invoice: string) {
  const left = qr.trim();
  const right = invoice.trim();
  if (left === right) return;
  out.push({ field, qr: left, invoice: right });
}

function pushNumber(out: QrMismatch[], field: string, qr: string, invoice: number) {
  const left = normalizeNumber(qr);
  if (left === null || Math.abs(left - invoice) <= 0.005) return;
  out.push({ field, qr: qr.trim(), invoice: invoice.toFixed(2) });
}

/**
 * Compares a GİB karekod payload with the UBL invoice it belongs to. Dates and
 * amounts are normalised first, so a formatting difference is not a mismatch.
 */
export function compareQrWithInvoice(qr: GibQr, invoice: UblInvoice): QrMismatch[] {
  const out: QrMismatch[] = [];
  push(out, "tarih", normalizeDate(qr.tarih), normalizeDate(invoice.issueDate));
  push(out, "no", qr.no, invoice.number);
  push(out, "ettn", qr.ettn.toLowerCase(), invoice.ettn.toLowerCase());
  push(out, "vkntckn", qr.vknTckn, invoice.supplier.taxId);
  if (qr.avknTckn && invoice.customer.taxId) push(out, "avkntckn", qr.avknTckn, invoice.customer.taxId);
  pushNumber(out, "malhizmettoplam", qr.malHizmetToplam, invoice.lineExtensionAmount);
  pushNumber(out, "vergidahil", qr.vergiDahil, invoice.taxInclusiveAmount);
  pushNumber(out, "odenecek", qr.odenecek, invoice.payableAmount);
  // VAT per rate: the payload keys the base and the tax by the rate.
  for (const [rate, base] of Object.entries(qr.kdvMatrah)) {
    const subtotal = invoice.taxSubtotals.find((entry) => Math.abs(entry.rate - Number(rate)) < 0.001);
    if (!subtotal) {
      out.push({ field: `kdvmatrah(${rate})`, qr: base, invoice: "" });
      continue;
    }
    pushNumber(out, `kdvmatrah(${rate})`, base, subtotal.taxable);
  }
  for (const [rate, tax] of Object.entries(qr.hesaplananKdv)) {
    const subtotal = invoice.taxSubtotals.find((entry) => Math.abs(entry.rate - Number(rate)) < 0.001);
    if (!subtotal) {
      out.push({ field: `hesaplanankdv(${rate})`, qr: tax, invoice: "" });
      continue;
    }
    pushNumber(out, `hesaplanankdv(${rate})`, tax, subtotal.tax);
  }
  return out;
}
