/**
 * The period audit as a workbook (C4): a summary sheet with the declaration
 * comparison, the findings, and every invoice of the period with its side
 * and flags. Written through the regular Calc writer (`saveDocument`).
 */
import { defaultCellStyle, newSheet, newWorkbook, type Cell, type Sheet, type Workbook } from "./office-types";
import type { UblInvoice } from "./office-api";
import type { PeriodAuditReport } from "./period-audit";

type Translate = (key: string, params?: Record<string, string | number>) => string;
type Value = string | number;

function columnName(index: number): string {
  let name = "";
  for (let rest = index + 1; rest > 0; rest = Math.floor((rest - 1) / 26)) {
    name = String.fromCharCode(65 + ((rest - 1) % 26)) + name;
  }
  return name;
}

function cell(value: Value, bold = false): Cell {
  return {
    value: typeof value === "number" ? { kind: "number", value } : { kind: "text", value },
    formula: null,
    style: { ...defaultCellStyle(), bold },
    comment: null,
  };
}

function sheetOf(name: string, rows: Value[][], headerRows: number[] = [0]): Sheet {
  const sheet = newSheet(name);
  rows.forEach((row, rowIndex) => {
    row.forEach((value, columnIndex) => {
      if (value === "") return;
      sheet.cells[`${columnName(columnIndex)}${rowIndex + 1}`] = cell(value, headerRows.includes(rowIndex));
    });
  });
  sheet.rowCount = Math.max(sheet.rowCount, rows.length + 10);
  sheet.freezeRows = headerRows.includes(0) ? 1 : 0;
  return sheet;
}

export function periodAuditWorkbook(
  report: PeriodAuditReport,
  invoices: readonly UblInvoice[],
  ownTaxId: string,
  t: Translate,
): Workbook {
  const own = ownTaxId.trim();
  const summary: Value[][] = [
    [t("periodAudit.sheet.summary"), ""],
    [t("periodAudit.period"), report.period],
    [t("periodAudit.invoiceCount"), report.invoiceCount],
    [t("periodAudit.inPeriod"), report.inPeriod],
    [t("periodAudit.findingCount"), report.findings.length],
    ["", ""],
    [t("periodAudit.figure"), t("periodAudit.computed"), t("periodAudit.declared"), t("periodAudit.difference")],
    ...report.comparisons.map((row) => [
      t(`periodAudit.field.${row.field}`),
      row.computed,
      row.declared,
      row.difference,
    ]),
  ];
  const findings: Value[][] = [
    [t("periodAudit.severity"), t("periodAudit.finding"), t("periodAudit.detail"), t("periodAudit.invoices")],
    ...report.findings.map((finding) => [
      t(`periodAudit.severity.${finding.severity}`),
      t(`periodAudit.code.${finding.code}Title`),
      finding.code === "mismatch" ? t(`periodAudit.field.${finding.detail}`) : (finding.detail ?? ""),
      finding.items.join(", "),
    ]),
  ];
  const side = (invoice: UblInvoice) =>
    !own
      ? ""
      : (invoice.supplier?.taxId || "").trim() === own
        ? t("periodAudit.side.sale")
        : (invoice.customer?.taxId || "").trim() === own
          ? t("periodAudit.side.purchase")
          : t("periodAudit.side.other");
  const rows: Value[][] = [
    [
      t("periodAudit.col.number"),
      t("periodAudit.col.date"),
      t("periodAudit.col.side"),
      t("periodAudit.col.supplier"),
      t("periodAudit.col.supplierTaxId"),
      t("periodAudit.col.customer"),
      t("periodAudit.col.customerTaxId"),
      t("periodAudit.col.taxable"),
      t("periodAudit.col.vat"),
      t("periodAudit.col.payable"),
      t("periodAudit.col.ettn"),
    ],
    ...invoices
      .filter((invoice) => (invoice.issueDate || "").slice(0, 7) === report.period)
      .map((invoice) => [
        invoice.number,
        invoice.issueDate,
        side(invoice),
        invoice.supplier?.name ?? "",
        invoice.supplier?.taxId ?? "",
        invoice.customer?.name ?? "",
        invoice.customer?.taxId ?? "",
        invoice.taxExclusiveAmount,
        invoice.taxAmount,
        invoice.payableAmount,
        invoice.ettn,
      ]),
  ];

  const workbook = newWorkbook(t("periodAudit.fileTitle", { period: report.period }));
  workbook.sheets = [
    sheetOf(t("periodAudit.sheet.summary"), summary, [0, 6]),
    sheetOf(t("periodAudit.sheet.findings"), findings),
    sheetOf(t("periodAudit.sheet.invoices"), rows),
  ];
  workbook.sheets[0].freezeRows = 0;
  return workbook;
}
