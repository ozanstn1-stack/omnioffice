/**
 * Accountant's period close audit (issue #341, C4): what is wrong with a
 * month's e-invoices before the VAT declaration goes in.
 *
 * - invoices dated outside the period;
 * - duplicates (same ETTN, same supplier + number, or same supplier, date and
 *   amount; `duplicate-invoices.ts`);
 * - gaps in the numbering of the user's own sales series (GİB numbers are a
 *   3-character series, the year and a 9-digit sequence), which point at a
 *   missing or cancelled invoice;
 * - a party's tax number that fails the VKN check digit or the TCKN check;
 * - invoices that are neither a sale nor a purchase of the user's own VKN;
 * - the VAT figures computed from the invoices against the ones typed from
 *   the declaration.
 *
 * Copy lives under `periodAudit.*`; this module returns codes and numbers.
 */
import { isValidTckn } from "./document-health";
import { detectDuplicateInvoices } from "./duplicate-invoices";
import { generateKdvDeclarationReport, type KdvDeclarationReport } from "./kdv-declaration";
import type { UblInvoice } from "./office-api";

export type AuditCode = "outOfPeriod" | "duplicate" | "numberGap" | "invalidTaxId" | "unclassified" | "mismatch";

export interface AuditFinding {
  code: AuditCode;
  severity: "error" | "warn";
  /** Invoice numbers (or missing numbers for a gap) the finding is about. */
  items: string[];
  /** Extra values for the message: a series, a party, a figure name. */
  detail?: string;
}

export type DeclaredField = "salesTaxable" | "salesVat" | "purchasesVat";

export interface DeclarationComparison {
  field: DeclaredField;
  computed: number;
  declared: number;
  difference: number;
}

export interface PeriodAuditInput {
  invoices: UblInvoice[];
  /** `YYYY-MM`. */
  period: string;
  ownTaxId: string;
  /** Figures typed from the declaration; a missing one is not compared. */
  declared?: Partial<Record<DeclaredField, number>>;
  /** Largest difference still treated as rounding (TL). */
  tolerance?: number;
}

export interface PeriodAuditReport {
  period: string;
  invoiceCount: number;
  inPeriod: number;
  kdv: KdvDeclarationReport;
  comparisons: DeclarationComparison[];
  findings: AuditFinding[];
}

/** The most-represented `YYYY-MM` among the invoices, a default period. */
export function likelyPeriod(invoices: readonly UblInvoice[]): string {
  const counts = new Map<string, number>();
  for (const invoice of invoices) {
    const month = (invoice.issueDate || "").slice(0, 7);
    if (/^\d{4}-\d{2}$/.test(month)) counts.set(month, (counts.get(month) ?? 0) + 1);
  }
  let best = "";
  for (const [month, count] of counts) if (!best || count > (counts.get(best) ?? 0)) best = month;
  return best;
}

/** VKN check digit (Gelir İdaresi algorithm). */
export function isValidVkn(value: string): boolean {
  if (!/^\d{10}$/.test(value)) return false;
  let sum = 0;
  for (let index = 0; index < 9; index++) {
    const shifted = (Number(value[index]) + 9 - index) % 10;
    let term = (shifted * 2 ** (9 - index)) % 9;
    if (shifted !== 0 && term === 0) term = 9;
    sum += term;
  }
  return (10 - (sum % 10)) % 10 === Number(value[9]);
}

/** A party's tax number: a 10-digit VKN or an 11-digit TCKN. Empty is not judged. */
export function isValidTaxNumber(value: string): boolean {
  const trimmed = value.trim();
  if (!trimmed) return true;
  return trimmed.length === 11 ? isValidTckn(trimmed) : isValidVkn(trimmed);
}

const GIB_NUMBER = /^([A-Z0-9]{3})(\d{4})(\d{9})$/;
/** A gap longer than this is reported by its ends, not number by number. */
const MAX_LISTED_GAP = 20;

/** Missing numbers inside each series (prefix + year) of the given invoice numbers. */
export function numberGaps(numbers: readonly string[]): { series: string; missing: string[] }[] {
  const series = new Map<string, number[]>();
  for (const number of numbers) {
    const match = GIB_NUMBER.exec(number.trim().toUpperCase());
    if (!match) continue;
    const key = `${match[1]}${match[2]}`;
    const list = series.get(key) ?? [];
    list.push(Number(match[3]));
    series.set(key, list);
  }
  const result: { series: string; missing: string[] }[] = [];
  for (const [key, list] of [...series.entries()].sort(([a], [b]) => a.localeCompare(b))) {
    const sorted = [...new Set(list)].sort((a, b) => a - b);
    const missing: string[] = [];
    for (let index = 1; index < sorted.length; index++) {
      const from = sorted[index - 1] + 1;
      const to = sorted[index] - 1;
      if (from > to) continue;
      const format = (sequence: number) => `${key}${String(sequence).padStart(9, "0")}`;
      if (to - from + 1 > MAX_LISTED_GAP) missing.push(`${format(from)}…${format(to)}`);
      else for (let sequence = from; sequence <= to; sequence++) missing.push(format(sequence));
    }
    if (missing.length) result.push({ series: key, missing });
  }
  return result;
}

function round2(value: number): number {
  return Math.round(value * 100) / 100;
}

export function auditPeriod(input: PeriodAuditInput): PeriodAuditReport {
  const { invoices, period } = input;
  const own = input.ownTaxId.trim();
  const tolerance = input.tolerance ?? 1;
  const findings: AuditFinding[] = [];
  const label = (invoice: UblInvoice) => invoice.number || invoice.ettn || "?";

  const inPeriod = invoices.filter((invoice) => (invoice.issueDate || "").slice(0, 7) === period);
  const outside = invoices.filter((invoice) => (invoice.issueDate || "").slice(0, 7) !== period);
  if (outside.length) findings.push({ code: "outOfPeriod", severity: "warn", items: outside.map(label) });

  for (const group of detectDuplicateInvoices(inPeriod).groups) {
    findings.push({
      code: "duplicate",
      severity: group.reason === "identical_amount_date" ? "warn" : "error",
      items: group.invoices.map(label),
      detail: group.reason,
    });
  }

  if (own) {
    const sales = inPeriod.filter((invoice) => (invoice.supplier?.taxId || "").trim() === own);
    for (const gap of numberGaps(sales.map((invoice) => invoice.number || ""))) {
      findings.push({ code: "numberGap", severity: "warn", items: gap.missing, detail: gap.series });
    }
    const unclassified = inPeriod.filter(
      (invoice) => (invoice.supplier?.taxId || "").trim() !== own && (invoice.customer?.taxId || "").trim() !== own,
    );
    if (unclassified.length) {
      findings.push({ code: "unclassified", severity: "warn", items: unclassified.map(label) });
    }
  }

  const badTaxIds = new Map<string, string[]>();
  for (const invoice of inPeriod) {
    for (const party of [invoice.supplier, invoice.customer]) {
      const taxId = (party?.taxId || "").trim();
      if (taxId && !isValidTaxNumber(taxId)) badTaxIds.set(taxId, [...(badTaxIds.get(taxId) ?? []), label(invoice)]);
    }
  }
  for (const [taxId, numbers] of badTaxIds) {
    findings.push({ code: "invalidTaxId", severity: "error", items: [...new Set(numbers)], detail: taxId });
  }

  const kdv = generateKdvDeclarationReport(inPeriod, period, own);
  const computed: Record<DeclaredField, number> = {
    salesTaxable: kdv.sales.taxableAmount,
    salesVat: kdv.sales.vatAmount,
    purchasesVat: kdv.purchases.vatAmount,
  };
  const comparisons: DeclarationComparison[] = [];
  for (const field of ["salesTaxable", "salesVat", "purchasesVat"] as const) {
    const declared = input.declared?.[field];
    if (declared === undefined || !Number.isFinite(declared)) continue;
    const difference = round2(computed[field] - declared);
    comparisons.push({ field, computed: round2(computed[field]), declared, difference });
    if (Math.abs(difference) > tolerance) {
      findings.push({ code: "mismatch", severity: "error", items: [], detail: field });
    }
  }

  const order = { error: 0, warn: 1 };
  findings.sort((a, b) => order[a.severity] - order[b.severity]);
  return { period, invoiceCount: invoices.length, inPeriod: inPeriod.length, kdv, comparisons, findings };
}
