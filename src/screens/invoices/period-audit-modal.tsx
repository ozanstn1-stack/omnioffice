import { useMemo, useState } from "react";
import { AlertOctagon, AlertTriangle, CheckCircle2, FileSpreadsheet } from "lucide-react";
import { appCacheDir, join } from "@tauri-apps/api/path";
import { Button, Modal } from "../../components/ui";
import { pickSaveToken } from "../../lib/file-tokens";
import { useT } from "../../lib/i18n";
import { isAndroid, saveFileOnAndroid } from "../../lib/mobile";
import { saveDocument, type UblInvoice } from "../../lib/office-api";
import { auditPeriod, likelyPeriod, type AuditFinding, type DeclaredField } from "../../lib/period-audit";
import { periodAuditWorkbook } from "../../lib/period-audit-workbook";
import { reportError, useToasts } from "../../lib/store";

const FIELDS: DeclaredField[] = ["salesTaxable", "salesVat", "purchasesVat"];

function parseAmount(text: string): number | undefined {
  const trimmed = text.trim();
  if (!trimmed) return undefined;
  // Accept "1.234,56" (Turkish) as well as "1234.56".
  const normalized = /,\d{1,2}$/.test(trimmed)
    ? trimmed.replace(/\./g, "").replace(",", ".")
    : trimmed.replace(/,/g, "");
  const value = Number(normalized);
  return Number.isFinite(value) ? value : undefined;
}

function money(value: number): string {
  return value.toLocaleString("tr-TR", { minimumFractionDigits: 2, maximumFractionDigits: 2 });
}

/**
 * Period close audit for the loaded invoices (C4): pick the month, type the
 * declaration's figures, read the findings and export them as a workbook.
 */
export function PeriodAuditModal({
  invoices,
  ownTaxId,
  onClose,
}: {
  invoices: UblInvoice[];
  ownTaxId: string;
  onClose: () => void;
}) {
  const t = useT();
  const periods = useMemo(
    () =>
      Array.from(
        new Set(
          invoices
            .map((invoice) => (invoice.issueDate || "").slice(0, 7))
            .filter((value) => /^\d{4}-\d{2}$/.test(value)),
        ),
      ).sort((a, b) => b.localeCompare(a)),
    [invoices],
  );
  const [period, setPeriod] = useState(() => likelyPeriod(invoices));
  const [declared, setDeclared] = useState<Record<DeclaredField, string>>({
    salesTaxable: "",
    salesVat: "",
    purchasesVat: "",
  });
  const [saving, setSaving] = useState(false);

  const report = useMemo(
    () =>
      auditPeriod({
        invoices,
        period,
        ownTaxId,
        declared: Object.fromEntries(FIELDS.map((field) => [field, parseAmount(declared[field])])),
      }),
    [invoices, period, ownTaxId, declared],
  );

  const exportWorkbook = async () => {
    setSaving(true);
    try {
      const workbook = periodAuditWorkbook(report, invoices, ownTaxId, t);
      const name = `Donem-Denetimi-${period}.xlsx`;
      if (isAndroid()) {
        const temp = await join(await appCacheDir(), name);
        await saveDocument("calc", workbook, temp);
        const saved = await saveFileOnAndroid(temp, name);
        if (saved) useToasts.getState().push({ kind: "success", title: t("periodAudit.saved"), detail: saved });
      } else {
        const target = await pickSaveToken(name, ["xlsx"]).catch(() => null);
        if (!target) return;
        await saveDocument("calc", workbook, target.token);
        useToasts
          .getState()
          .push({ kind: "success", title: t("periodAudit.saved"), detail: target.path ?? target.name });
      }
    } catch (error) {
      reportError(error, t);
    } finally {
      setSaving(false);
    }
  };

  return (
    <Modal
      title={t("periodAudit.title")}
      onClose={onClose}
      width={720}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t("common.close")}
          </Button>
          <Button
            variant="primary"
            icon={<FileSpreadsheet size={14} />}
            disabled={saving}
            onClick={() => void exportWorkbook()}
          >
            {t("periodAudit.export")}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-3 text-sm">
        <div className="grid gap-3" style={{ gridTemplateColumns: "repeat(auto-fit, minmax(160px, 1fr))" }}>
          <div>
            <label className="label" htmlFor="audit-period">
              {t("periodAudit.period")}
            </label>
            <select
              id="audit-period"
              className="select"
              value={period}
              onChange={(event) => setPeriod(event.target.value)}
            >
              {periods.map((value) => (
                <option key={value} value={value}>
                  {value}
                </option>
              ))}
            </select>
          </div>
          {FIELDS.map((field) => (
            <div key={field}>
              <label className="label" htmlFor={`audit-${field}`}>
                {t("periodAudit.declaredField", { field: t(`periodAudit.field.${field}`) })}
              </label>
              <input
                id={`audit-${field}`}
                className="input"
                inputMode="decimal"
                value={declared[field]}
                placeholder="0,00"
                onChange={(event) => setDeclared((current) => ({ ...current, [field]: event.target.value }))}
              />
            </div>
          ))}
        </div>
        {!ownTaxId.trim() ? <p className="text-xs muted">{t("periodAudit.noOwnTaxId")}</p> : null}

        {report.comparisons.length ? (
          <table className="table">
            <thead>
              <tr>
                <th>{t("periodAudit.figure")}</th>
                <th>{t("periodAudit.computed")}</th>
                <th>{t("periodAudit.declared")}</th>
                <th>{t("periodAudit.difference")}</th>
              </tr>
            </thead>
            <tbody>
              {report.comparisons.map((row) => (
                <tr key={row.field}>
                  <td>{t(`periodAudit.field.${row.field}`)}</td>
                  <td>{money(row.computed)}</td>
                  <td>{money(row.declared)}</td>
                  <td style={{ color: Math.abs(row.difference) > 1 ? "var(--danger)" : undefined }}>
                    {money(row.difference)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        ) : null}

        {report.findings.length === 0 ? (
          <p className="flex items-center gap-2" role="status">
            <CheckCircle2 size={16} style={{ color: "var(--ok)" }} aria-hidden />
            {t("periodAudit.clean", { count: report.inPeriod })}
          </p>
        ) : (
          <ul className="flex flex-col gap-2" aria-label={t("periodAudit.findings")}>
            {report.findings.map((finding, index) => (
              <FindingRow key={`${finding.code}-${index}`} finding={finding} />
            ))}
          </ul>
        )}
      </div>
    </Modal>
  );
}

function FindingRow({ finding }: { finding: AuditFinding }) {
  const t = useT();
  const shown = finding.items.slice(0, 12).join(", ");
  const more = finding.items.length > 12 ? ` ${t("periodAudit.more", { count: finding.items.length - 12 })}` : "";
  const detail =
    finding.code === "mismatch"
      ? t(`periodAudit.field.${finding.detail}`)
      : finding.code === "duplicate"
        ? t(`periodAudit.duplicate.${finding.detail}`)
        : (finding.detail ?? "");
  return (
    <li className="flex items-start gap-2 rounded-lg p-2.5" style={{ background: "var(--surface-2)" }}>
      {finding.severity === "error" ? (
        <AlertOctagon size={16} style={{ color: "var(--danger)", marginTop: 2 }} aria-hidden />
      ) : (
        <AlertTriangle size={16} style={{ color: "var(--warn)", marginTop: 2 }} aria-hidden />
      )}
      <span className="flex-1 min-w-0">
        <span className="block font-semibold">{t(`periodAudit.code.${finding.code}Title`)}</span>
        <span className="block text-xs muted mt-0.5 leading-relaxed">
          {t(`periodAudit.code.${finding.code}Body`, { detail, count: finding.items.length })}
          {shown ? ` ${shown}${more}` : ""}
        </span>
      </span>
    </li>
  );
}
