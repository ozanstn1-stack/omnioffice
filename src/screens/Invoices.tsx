import { useCallback, useEffect, useState } from "react";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { AlertTriangle, CheckCircle2, FileText, Receipt, Save, Table2 } from "lucide-react";
import { Badge, Button, Card } from "../components/ui";
import { Screen } from "../components/layout";
import { useT } from "../lib/i18n";
import { reportError, useToasts } from "../lib/store";
import { isAndroid, pickOfficeFiles } from "../lib/mobile";
import { exportPdf, openInvoice, invoiceSummary, saveDocument, vatRates } from "../lib/office-api";
import type { InvoiceDocument, VatRate } from "../lib/office-api";

interface Entry {
  path: string;
  document: InvoiceDocument;
}

function money(value: number, currency: string) {
  const formatted = value.toLocaleString("tr-TR", { minimumFractionDigits: 2, maximumFractionDigits: 2 });
  return `${formatted} ${currency || "TRY"}`;
}

/**
 * e-Fatura / e-Arşiv (UBL-TR) reader.
 *
 * Invoices are rendered with the app's own deterministic template; an XSLT
 * embedded in the invoice is never executed (it is only listed as a warning).
 * The VAT summary is aggregated locally into a Calc workbook.
 */
export function Invoices({ initialFiles = [] }: { initialFiles?: string[] }) {
  const t = useT();
  const [entries, setEntries] = useState<Entry[]>([]);
  const [rates, setRates] = useState<VatRate[]>([]);
  const [busy, setBusy] = useState(false);

  const load = useCallback(
    async (paths: string[]) => {
      if (!paths.length) return;
      setBusy(true);
      try {
        const loaded: Entry[] = [];
        for (const path of paths) {
          try {
            loaded.push({ path, document: await openInvoice(path) });
          } catch (error) {
            reportError(error, t);
          }
        }
        setEntries((current) => [...loaded, ...current.filter((entry) => !paths.includes(entry.path))]);
      } finally {
        setBusy(false);
      }
    },
    [t],
  );

  const initialKey = initialFiles.join("|");

  useEffect(() => {
    void vatRates()
      .then((list) => setRates(list))
      .catch(() => undefined);
  }, []);

  // Auto-load the selection handed over by another screen. State is only set
  // after the awaits, so this never cascades a render synchronously.
  useEffect(() => {
    if (!initialKey) return;
    let cancelled = false;
    const paths = initialKey.split("|");
    void (async () => {
      const loaded: Entry[] = [];
      for (const path of paths) {
        try {
          loaded.push({ path, document: await openInvoice(path) });
        } catch (error) {
          reportError(error, t);
        }
      }
      if (!cancelled && loaded.length) {
        setEntries((current) => [...loaded, ...current.filter((entry) => !paths.includes(entry.path))]);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [initialKey, t]);

  const pick = async () => {
    let paths: string[] = [];
    if (isAndroid()) {
      paths = await pickOfficeFiles(true).catch(() => []);
    } else {
      const picked = await openDialog({
        multiple: true,
        filters: [{ name: t("invoices.filterName"), extensions: ["xml", "zip"] }],
      });
      paths = Array.isArray(picked) ? picked : typeof picked === "string" ? [picked] : [];
    }
    await load(paths);
  };

  const savePdf = async (entry: Entry) => {
    const path = await saveDialog({
      defaultPath: `${entry.document.invoice.number || "e-fatura"}.pdf`,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (typeof path !== "string") return;
    try {
      await exportPdf("writer", entry.document.model, path);
      useToasts.getState().push({ kind: "success", title: t("invoices.pdfSaved"), detail: path });
    } catch (error) {
      reportError(error, t);
    }
  };

  const saveSummary = async () => {
    if (!entries.length) return;
    const path = await saveDialog({
      defaultPath: "KDV-Ozeti.xlsx",
      filters: [{ name: "Excel", extensions: ["xlsx"] }],
    });
    if (typeof path !== "string") return;
    setBusy(true);
    try {
      const summary = await invoiceSummary(entries.map((entry) => entry.path));
      await saveDocument("calc", summary.workbook, path);
      useToasts.getState().push({
        kind: "success",
        title: t("invoices.summarySaved", { count: summary.count }),
        detail: path,
      });
      for (const warning of summary.warnings) {
        useToasts.getState().push({ kind: "info", title: t("invoices.warnings"), detail: warning });
      }
    } catch (error) {
      reportError(error, t);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Screen title={t("invoices.title")} subtitle={t("invoices.subtitle")}>
      <div className="flex flex-col gap-4">
        <Card className="p-5 flex flex-col gap-3">
          <div className="flex items-center gap-2 flex-wrap">
            <Button variant="primary" disabled={busy} onClick={() => void pick()}>
              <FileText size={16} /> {t("invoices.pick")}
            </Button>
            <Button variant="ghost" disabled={busy || !entries.length} onClick={() => void saveSummary()}>
              <Table2 size={16} /> {t("invoices.summary")}
            </Button>
            <span className="text-xs muted">{t("invoices.pickHint")}</span>
          </div>
          <p className="text-xs muted">{t("invoices.xsltNote")}</p>
        </Card>

        {entries.length === 0 ? (
          <Card className="p-5">
            <p className="muted">{t("invoices.empty")}</p>
          </Card>
        ) : null}

        {entries.map((entry) => {
          const invoice = entry.document.invoice;
          return (
            <Card key={entry.path} className="p-5 flex flex-col gap-3">
              <div className="flex items-center gap-2 flex-wrap">
                <Receipt size={16} style={{ color: "var(--accent)" }} />
                <strong>{invoice.number || entry.path}</strong>
                {invoice.profile ? <Badge>{invoice.profile}</Badge> : null}
                {invoice.typeCode ? <Badge>{invoice.typeCode}</Badge> : null}
                <Button size="sm" variant="ghost" onClick={() => void savePdf(entry)}>
                  <Save size={14} /> {t("invoices.savePdf")}
                </Button>
              </div>
              <div className="grid gap-2" style={{ gridTemplateColumns: "repeat(auto-fit, minmax(220px, 1fr))" }}>
                <div>
                  <span className="label">{t("invoices.date")}</span>
                  <div>
                    {invoice.issueDate} {invoice.issueTime}
                  </div>
                </div>
                <div>
                  <span className="label">{t("invoices.supplier")}</span>
                  <div>{invoice.supplier.name || "—"}</div>
                  <div className="text-xs muted">
                    {invoice.supplier.taxIdScheme} {invoice.supplier.taxId}
                  </div>
                </div>
                <div>
                  <span className="label">{t("invoices.customer")}</span>
                  <div>{invoice.customer.name || "—"}</div>
                  <div className="text-xs muted">
                    {invoice.customer.taxIdScheme} {invoice.customer.taxId}
                  </div>
                </div>
                <div>
                  <span className="label">{t("invoices.payable")}</span>
                  <div>
                    <strong>{money(invoice.payableAmount, invoice.currency)}</strong>
                  </div>
                  <div className="text-xs muted">
                    {t("invoices.vat")}: {money(invoice.taxAmount, invoice.currency)}
                  </div>
                </div>
              </div>
              {invoice.ettn ? (
                <div className="text-xs muted">
                  {t("invoices.ettn")}: {invoice.ettn}
                </div>
              ) : null}

              <div className="overflow-x-auto">
                <table className="text-sm" style={{ width: "100%" }}>
                  <thead>
                    <tr>
                      <th>{t("invoices.line.name")}</th>
                      <th>{t("invoices.line.qty")}</th>
                      <th>{t("invoices.line.unitPrice")}</th>
                      <th>{t("invoices.line.total")}</th>
                      <th>{t("invoices.line.vatRate")}</th>
                      <th>{t("invoices.line.vat")}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {invoice.lines.map((line) => (
                      <tr key={line.id || line.name}>
                        <td>{line.name}</td>
                        <td>
                          {line.quantity} {line.unit}
                        </td>
                        <td>{money(line.unitPrice, invoice.currency)}</td>
                        <td>{money(line.lineTotal, invoice.currency)}</td>
                        <td>%{line.vatRate}</td>
                        <td>{money(line.vatAmount, invoice.currency)}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>

              {entry.document.issues.length === 0 ? (
                <p className="text-sm flex items-center gap-2">
                  <CheckCircle2 size={16} style={{ color: "var(--ok, #16a34a)" }} /> {t("invoices.issuesNone")}
                </p>
              ) : (
                <div className="flex flex-col gap-1">
                  <strong className="flex items-center gap-2">
                    <AlertTriangle size={16} style={{ color: "var(--warn, #d97706)" }} /> {t("invoices.issues")}
                  </strong>
                  <ul className="text-sm muted">
                    {entry.document.issues.map((issue) => (
                      <li key={issue.field}>
                        {issue.message} ({issue.field}: {issue.expected} ≠ {issue.found})
                      </li>
                    ))}
                  </ul>
                </div>
              )}
              {entry.document.warnings.map((warning) => (
                <p key={warning} className="text-xs muted">
                  {warning}
                </p>
              ))}
            </Card>
          );
        })}

        {rates.length ? (
          <Card className="p-5 flex flex-col gap-2">
            <strong>{t("invoices.rates.title")}</strong>
            <table className="text-sm">
              <thead>
                <tr>
                  <th>{t("invoices.rates.rate")}</th>
                  <th>{t("invoices.rates.label")}</th>
                  <th>{t("invoices.rates.since")}</th>
                  <th>{t("invoices.rates.until")}</th>
                </tr>
              </thead>
              <tbody>
                {rates.map((rate) => (
                  <tr key={`${rate.rate}-${rate.since}`}>
                    <td>%{rate.rate}</td>
                    <td>{rate.label}</td>
                    <td>{rate.since}</td>
                    <td>{rate.until || "—"}</td>
                  </tr>
                ))}
              </tbody>
            </table>
            <p className="text-xs muted">{t("invoices.rates.note")}</p>
          </Card>
        ) : null}
      </div>
    </Screen>
  );
}
