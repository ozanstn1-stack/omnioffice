import { lazy, Suspense, useCallback, useEffect, useState } from "react";
import { appCacheDir, join } from "@tauri-apps/api/path";
import { pickSaveToken } from "../lib/file-tokens";
import { fileBaseName } from "../lib/format";
import { pickDocumentTokens } from "../lib/file-tokens";
import {
  AlertTriangle,
  Calculator,
  CheckCircle2,
  ClipboardCheck,
  Copy,
  FileText,
  QrCode,
  Receipt,
  Save,
  Table2,
} from "lucide-react";
import { Badge, Button, Card, Modal } from "../components/ui";
import { Screen } from "../components/layout";
import { useT } from "../lib/i18n";
import { flagEnabled } from "../lib/flags";
import { reportError, useSettings, useToasts } from "../lib/store";
import { isAndroid, pickOfficeFiles, saveFileOnAndroid, saveTextOnAndroid } from "../lib/mobile";
import { compareQrWithInvoice } from "../lib/invoice-qr";
import { exportAccounting, type AccountingSoftware } from "../lib/accounting-export";
import { aiSaveOutput } from "../lib/api";
import { generateKdvDeclarationReport } from "../lib/kdv-declaration";
import { detectDuplicateInvoices } from "../lib/duplicate-invoices";
import {
  exportPdf,
  openInvoice,
  openInvoicePackage,
  invoiceSummary,
  readInvoiceQr,
  saveDocument,
  vatRates,
} from "../lib/office-api";
import type { InvoiceDocument, InvoiceQr, UblInvoice, VatRate } from "../lib/office-api";

// The audit (rules, workbook writer, dialog) loads on first use.
const PeriodAuditModal = lazy(() =>
  import("./invoices/period-audit-modal").then((module) => ({ default: module.PeriodAuditModal })),
);

type Entry =
  | { id: string; path: string; kind: "ubl"; document: InvoiceDocument }
  | { id: string; path: string; kind: "pdf"; qr: InvoiceQr | null; error?: string };

function money(value: number, currency: string) {
  const formatted = value.toLocaleString("tr-TR", { minimumFractionDigits: 2, maximumFractionDigits: 2 });
  return `${formatted} ${currency || "TRY"}`;
}

function extensionOf(path: string) {
  return (fileBaseName(path).split(".").pop() ?? "").toLowerCase();
}

/** Reads a UBL file: a `.zip` can hold several invoices, so all of them load. */
async function loadUblEntries(path: string): Promise<Entry[]> {
  if (extensionOf(path) === "zip") {
    const documents = await openInvoicePackage(path);
    return documents.map((document, idx) => ({
      id: `${path}#${document.invoice.ettn || document.invoice.number || idx}`,
      path,
      kind: "ubl" as const,
      document,
    }));
  }
  const document = await openInvoice(path);
  return [
    {
      id: `${path}#${document.invoice.ettn || document.invoice.number || 0}`,
      path,
      kind: "ubl",
      document,
    },
  ];
}

/**
 * e-Fatura / e-Arşiv (UBL-TR) reader.
 *
 * UBL `.xml`/`.zip` files are rendered with the app's own deterministic
 * template; a scanned/printed PDF's GİB karekod is read locally and compared
 * with the UBL document, with any disagreement shown in red. An XSLT embedded
 * in an invoice is never executed.
 */
export function Invoices({ initialFiles = [] }: { initialFiles?: string[] }) {
  const t = useT();
  const [entries, setEntries] = useState<Entry[]>([]);
  const [rates, setRates] = useState<VatRate[]>([]);
  const [busy, setBusy] = useState(false);
  const [showKdvModal, setShowKdvModal] = useState(false);
  const [kdvPeriod, setKdvPeriod] = useState("");
  const [showAudit, setShowAudit] = useState(false);
  const ownTaxId = useSettings((state) => state.settings.ownTaxId);

  const initialKey = initialFiles.join("|");

  const load = useCallback(
    async (paths: string[]) => {
      if (!paths.length) return;
      setBusy(true);
      try {
        const loaded: Entry[] = [];
        for (const path of paths) {
          const extension = extensionOf(path);
          if (extension === "xml" || extension === "zip") {
            try {
              loaded.push(...(await loadUblEntries(path)));
            } catch (error) {
              reportError(error, t);
            }
          } else {
            loaded.push({ id: path, path, kind: "pdf", qr: null });
          }
        }
        setEntries((current) => [...loaded, ...current.filter((entry) => !paths.includes(entry.path))]);
      } finally {
        setBusy(false);
      }
    },
    [t],
  );

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
        const extension = extensionOf(path);
        if (extension === "xml" || extension === "zip") {
          try {
            loaded.push(...(await loadUblEntries(path)));
          } catch (error) {
            reportError(error, t);
          }
        } else {
          loaded.push({ id: path, path, kind: "pdf", qr: null });
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
      paths = (await pickDocumentTokens(["xml", "zip", "pdf"], true).catch(() => [])).map((file) => file.token);
    }
    await load(paths);
  };

  const readQr = async (entry: Entry & { kind: "pdf" }) => {
    try {
      const qr = await readInvoiceQr(entry.path);
      setEntries((current) =>
        current.map((candidate) =>
          candidate.id === entry.id ? { ...candidate, kind: "pdf", qr, error: undefined } : candidate,
        ),
      );
    } catch (error) {
      reportError(error, t);
      setEntries((current) =>
        current.map((candidate) => (candidate.id === entry.id ? { ...candidate, kind: "pdf", qr: null } : candidate)),
      );
    }
  };

  const savePdf = async (document: InvoiceDocument) => {
    const defaultName = `${document.invoice.number || "e-fatura"}.pdf`;
    try {
      if (isAndroid()) {
        const tempPath = await join(await appCacheDir(), defaultName);
        await exportPdf("writer", document.model, tempPath);
        const saved = await saveFileOnAndroid(tempPath, defaultName);
        if (saved) {
          useToasts.getState().push({ kind: "success", title: t("invoices.pdfSaved"), detail: saved });
        }
        return;
      }
      const target = await pickSaveToken(fileBaseName(defaultName), ["pdf"]).catch(() => null);
      if (!target) return;
      await exportPdf("writer", document.model, target.token);
      useToasts.getState().push({ kind: "success", title: t("invoices.pdfSaved"), detail: target.path ?? target.name });
    } catch (error) {
      reportError(error, t);
    }
  };

  const saveSummary = async () => {
    const ubl = entries.filter((entry): entry is Entry & { kind: "ubl" } => entry.kind === "ubl");
    if (!ubl.length) return;
    const uniquePaths = Array.from(new Set(ubl.map((entry) => entry.path)));
    setBusy(true);
    try {
      const summary = await invoiceSummary(uniquePaths);
      if (isAndroid()) {
        const tempPath = await join(await appCacheDir(), "KDV-Ozeti.xlsx");
        await saveDocument("calc", summary.workbook, tempPath);
        const saved = await saveFileOnAndroid(tempPath, "KDV-Ozeti.xlsx");
        if (saved) {
          useToasts.getState().push({
            kind: "success",
            title: t("invoices.summarySaved", { count: summary.count }),
            detail: saved,
          });
        }
      } else {
        const target = await pickSaveToken("KDV-Ozeti.xlsx", ["xlsx"]).catch(() => null);
        if (!target) return;
        await saveDocument("calc", summary.workbook, target.token);
        useToasts.getState().push({
          kind: "success",
          title: t("invoices.summarySaved", { count: summary.count }),
          detail: target.path ?? target.name,
        });
      }
      for (const warning of summary.warnings) {
        useToasts.getState().push({ kind: "info", title: t("invoices.warnings"), detail: warning });
      }
    } catch (error) {
      reportError(error, t);
    } finally {
      setBusy(false);
    }
  };

  const exportForAccounting = async (software: AccountingSoftware) => {
    const ublInvoices = entries
      .filter((e): e is Entry & { kind: "ubl" } => e.kind === "ubl")
      .map((e) => e.document.invoice);
    if (!ublInvoices.length) return;
    const dateSlug = new Date().toISOString().slice(0, 10);
    const defaultName = `${software.toUpperCase()}-Aktarim-${dateSlug}.csv`;
    setBusy(true);
    try {
      if (isAndroid()) {
        const content = exportAccounting(ublInvoices, software);
        const ok = await saveTextOnAndroid(content, defaultName);
        if (ok) {
          useToasts.getState().push({
            kind: "success",
            title: `${software.toUpperCase()} Aktarım Dosyası Kaydedildi`,
            detail: defaultName,
          });
        }
        return;
      }
      // CSV only: the vendor "Excel" and Logo-XML shapes were never verified
      // against a real import template, so they are not offered
      // (RELEASE_READINESS.md; the whole vendor export area is flag-gated).
      const target = await pickSaveToken(fileBaseName(defaultName), ["csv"]).catch(() => null);
      if (!target) return;
      const content = exportAccounting(ublInvoices, software);
      await aiSaveOutput(target.token, content, "overwrite");
      useToasts.getState().push({
        kind: "success",
        title: `${software.toUpperCase()} Aktarım Dosyası Kaydedildi`,
        detail: target.path ?? target.name,
      });
    } catch (error) {
      reportError(error, t);
    } finally {
      setBusy(false);
    }
  };

  const invoices: UblInvoice[] = entries.filter((entry) => entry.kind === "ubl").map((entry) => entry.document.invoice);
  const duplicateResult = detectDuplicateInvoices(invoices);

  const deduplicateInvoices = () => {
    const seenEttns = new Set<string>();
    const seenNumbers = new Set<string>();

    setEntries((current) => {
      const kept: Entry[] = [];
      for (const entry of current) {
        if (entry.kind !== "ubl") {
          kept.push(entry);
          continue;
        }
        const inv = entry.document.invoice;
        const ettn = (inv.ettn || "").toLowerCase().trim();
        const supplierTaxId = (inv.supplier?.taxId || "").trim();
        const docNo = (inv.number || "").trim();
        const numKey = supplierTaxId && docNo ? `${supplierTaxId}:${docNo}` : "";

        if (ettn && seenEttns.has(ettn)) continue;
        if (numKey && seenNumbers.has(numKey)) continue;

        if (ettn) seenEttns.add(ettn);
        if (numKey) seenNumbers.add(numKey);
        kept.push(entry);
      }
      return kept;
    });

    useToasts.getState().push({
      kind: "success",
      title: "Mükerrer Faturalar Temizlendi",
      detail: `${duplicateResult.totalDuplicates} adet mükerrer fatura listeden çıkarıldı.`,
    });
  };

  const matchingInvoice = (qr: InvoiceQr): UblInvoice | undefined =>
    invoices.find(
      (invoice) =>
        (qr.payload.no && invoice.number === qr.payload.no) ||
        (qr.payload.ettn && invoice.ettn.toLowerCase() === qr.payload.ettn.toLowerCase()),
    );

  return (
    <Screen title={t("invoices.title")} subtitle={t("invoices.subtitle")}>
      <div className="flex flex-col gap-4">
        <Card className="p-5 flex flex-col gap-3">
          <div className="flex items-center gap-2 flex-wrap">
            <Button variant="primary" disabled={busy} onClick={() => void pick()}>
              <FileText size={16} /> {t("invoices.pick")}
            </Button>
            <Button variant="ghost" disabled={busy || !invoices.length} onClick={() => void saveSummary()}>
              <Table2 size={16} /> {t("invoices.summary")}
            </Button>
            {/* Luca/Logo/Zirve exports are experimental: no real import
                template has verified them (RELEASE_READINESS.md), so they stay
                behind the `accountingVendorExport` flag. */}
            {flagEnabled("accountingVendorExport") ? (
              <>
                <Button
                  variant="ghost"
                  disabled={busy || !invoices.length}
                  onClick={() => void exportForAccounting("luca")}
                >
                  <Receipt size={16} /> Luca
                </Button>
                <Button
                  variant="ghost"
                  disabled={busy || !invoices.length}
                  onClick={() => void exportForAccounting("logo")}
                >
                  <Receipt size={16} /> Logo
                </Button>
                <Button
                  variant="ghost"
                  disabled={busy || !invoices.length}
                  onClick={() => void exportForAccounting("zirve")}
                >
                  <Receipt size={16} /> Zirve
                </Button>
              </>
            ) : null}
            <Button
              variant="ghost"
              disabled={busy || !invoices.length}
              data-testid="kdv-declaration-open"
              onClick={() => setShowKdvModal(true)}
            >
              <Calculator size={16} /> {t("invoices.kdv.open")}
            </Button>
            <Button variant="ghost" disabled={busy || !invoices.length} onClick={() => setShowAudit(true)}>
              <ClipboardCheck size={16} /> {t("periodAudit.open")}
            </Button>
            <span className="text-xs muted">{t("invoices.pickHint")}</span>
          </div>
          <p className="text-xs muted">{t("invoices.xsltNote")}</p>
        </Card>

        {duplicateResult.hasDuplicates ? (
          <Card className="p-4 bg-amber-500/10 border-amber-500/30 flex items-center justify-between gap-4 flex-wrap">
            <div className="flex items-center gap-3">
              <AlertTriangle className="text-amber-500 shrink-0" size={20} />
              <div>
                <strong className="text-amber-500">
                  {t.plural("invoices.duplicatesFound", duplicateResult.totalDuplicates, {
                    count: duplicateResult.totalDuplicates,
                  })}
                </strong>
                <p className="text-xs muted">{t("invoices.duplicatesDetail")}</p>
              </div>
            </div>
            <Button size="sm" variant="default" onClick={deduplicateInvoices}>
              <Copy size={14} /> {t("invoices.deduplicate")}
            </Button>
          </Card>
        ) : null}

        {entries.length === 0 ? (
          <Card className="p-5">
            <p className="muted">{t("invoices.empty")}</p>
          </Card>
        ) : null}

        {entries.map((entry) => {
          if (entry.kind === "pdf") {
            const qr = entry.qr;
            const invoice = qr ? matchingInvoice(qr) : undefined;
            const mismatches = qr && invoice ? compareQrWithInvoice(qr.payload, invoice) : [];
            return (
              <Card key={entry.id} className="p-5 flex flex-col gap-3">
                <div className="flex items-center gap-2 flex-wrap">
                  <QrCode size={16} style={{ color: "var(--accent)" }} />
                  <strong>{fileBaseName(entry.path)}</strong>
                  <Button size="sm" variant="ghost" onClick={() => void readQr(entry)}>
                    {t("invoices.qr.read")}
                  </Button>
                </div>
                {!qr ? (
                  <p className="muted text-sm">{t("invoices.qr.none")}</p>
                ) : (
                  <>
                    <div className="grid gap-2" style={{ gridTemplateColumns: "repeat(auto-fit, minmax(200px, 1fr))" }}>
                      <div>
                        <span className="label">{t("invoices.number")}</span>
                        <div>{qr.payload.no || "—"}</div>
                      </div>
                      <div>
                        <span className="label">{t("invoices.date")}</span>
                        <div>{qr.payload.tarih || "—"}</div>
                      </div>
                      <div>
                        <span className="label">{t("invoices.qr.sender")}</span>
                        <div>
                          {qr.payload.vknTckn}{" "}
                          {qr.taxIdValid ? (
                            <Badge tone="ok">{t("invoices.qr.validTaxId")}</Badge>
                          ) : (
                            <Badge tone="danger">{t("invoices.qr.invalidTaxId")}</Badge>
                          )}
                        </div>
                      </div>
                      <div>
                        <span className="label">{t("invoices.payable")}</span>
                        <div>{qr.payload.odenecek || "—"}</div>
                      </div>
                    </div>
                    <div className="text-xs muted">
                      {t("invoices.ettn")}: {qr.payload.ettn || "—"} · {t("invoices.qr.page", { page: qr.page })}
                    </div>
                    {invoice ? (
                      mismatches.length === 0 ? (
                        <p className="text-sm flex items-center gap-2">
                          <CheckCircle2 size={16} style={{ color: "var(--ok, #16a34a)" }} /> {t("invoices.qr.match")}
                        </p>
                      ) : (
                        <div className="flex flex-col gap-1">
                          <strong className="flex items-center gap-2">
                            <AlertTriangle size={16} style={{ color: "var(--danger, #dc2626)" }} />{" "}
                            {t("invoices.qr.mismatch")}
                          </strong>
                          <ul className="text-sm" style={{ color: "var(--danger, #dc2626)" }}>
                            {mismatches.map((mismatch) => (
                              <li key={mismatch.field}>
                                {mismatch.field}: {t("invoices.qr.qrValue")} “{mismatch.qr}” ≠{" "}
                                {t("invoices.qr.invoiceValue")} “{mismatch.invoice}”
                              </li>
                            ))}
                          </ul>
                        </div>
                      )
                    ) : (
                      <p className="text-xs muted">{t("invoices.qr.noInvoiceToCompare")}</p>
                    )}
                    {qr.warnings.map((warning) => (
                      <p key={warning} className="text-xs muted">
                        {warning}
                      </p>
                    ))}
                  </>
                )}
              </Card>
            );
          }
          const invoice = entry.document.invoice;
          return (
            <Card key={entry.id} className="p-5 flex flex-col gap-3">
              <div className="flex items-center gap-2 flex-wrap">
                <Receipt size={16} style={{ color: "var(--accent)" }} />
                <strong>{invoice.number || entry.path}</strong>
                {invoice.profile ? <Badge>{invoice.profile}</Badge> : null}
                {invoice.typeCode ? <Badge>{invoice.typeCode}</Badge> : null}
                <Button size="sm" variant="ghost" onClick={() => void savePdf(entry.document)}>
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

        {showAudit ? (
          <Suspense fallback={null}>
            <PeriodAuditModal invoices={invoices} ownTaxId={ownTaxId} onClose={() => setShowAudit(false)} />
          </Suspense>
        ) : null}

        {showKdvModal ? (
          <Modal
            title={t("invoices.kdv.title")}
            onClose={() => setShowKdvModal(false)}
            footer={
              <Button variant="primary" onClick={() => setShowKdvModal(false)}>
                {t("common.close")}
              </Button>
            }
          >
            {(() => {
              // The period list comes from the loaded invoices' dates, so the
              // selector can only produce real YYYY-MM values.
              const periods = Array.from(
                new Set(
                  invoices
                    .map((invoice) => (invoice.issueDate || "").slice(0, 7))
                    .filter((value) => value.length === 7),
                ),
              ).sort((left, right) => right.localeCompare(left));
              const report = generateKdvDeclarationReport(invoices, kdvPeriod, ownTaxId);
              return (
                <div className="flex flex-col gap-3 text-sm" data-testid="kdv-declaration">
                  <div className="flex items-center gap-2 flex-wrap">
                    <label className="text-xs muted" htmlFor="kdv-period">
                      {t("invoices.kdv.period")}
                    </label>
                    <select
                      id="kdv-period"
                      className="input input-sm"
                      value={kdvPeriod}
                      onChange={(event) => setKdvPeriod(event.target.value)}
                    >
                      <option value="">{t("invoices.kdv.allPeriods")}</option>
                      {periods.map((period) => (
                        <option key={period} value={period}>
                          {period}
                        </option>
                      ))}
                    </select>
                    {!ownTaxId.trim() ? <span className="text-xs muted">{t("invoices.kdv.ownTaxIdHint")}</span> : null}
                  </div>
                  <p className="muted text-xs">{t("invoices.kdv.intro", { count: report.invoiceCount })}</p>
                  <table className="w-full text-left border-collapse">
                    <thead>
                      <tr className="border-b">
                        <th className="py-1">{t("invoices.kdv.rate")}</th>
                        <th className="py-1">{t("invoices.kdv.base")}</th>
                        <th className="py-1">{t("invoices.kdv.calculated")}</th>
                      </tr>
                    </thead>
                    <tbody>
                      <tr>
                        <td className="py-1">%1</td>
                        <td className="py-1">
                          {report.rates[1].taxableAmount.toLocaleString("tr-TR", { minimumFractionDigits: 2 })} TRY
                        </td>
                        <td className="py-1">
                          {report.rates[1].vatAmount.toLocaleString("tr-TR", { minimumFractionDigits: 2 })} TRY
                        </td>
                      </tr>
                      <tr>
                        <td className="py-1">%10</td>
                        <td className="py-1">
                          {report.rates[10].taxableAmount.toLocaleString("tr-TR", { minimumFractionDigits: 2 })} TRY
                        </td>
                        <td className="py-1">
                          {report.rates[10].vatAmount.toLocaleString("tr-TR", { minimumFractionDigits: 2 })} TRY
                        </td>
                      </tr>
                      <tr>
                        <td className="py-1">%20</td>
                        <td className="py-1">
                          {report.rates[20].taxableAmount.toLocaleString("tr-TR", { minimumFractionDigits: 2 })} TRY
                        </td>
                        <td className="py-1">
                          {report.rates[20].vatAmount.toLocaleString("tr-TR", { minimumFractionDigits: 2 })} TRY
                        </td>
                      </tr>
                      {report.exemptionAmount > 0 ? (
                        <tr>
                          <td className="py-1">{t("invoices.kdv.exempt")}</td>
                          <td className="py-1">
                            {report.exemptionAmount.toLocaleString("tr-TR", { minimumFractionDigits: 2 })} TRY
                          </td>
                          <td className="py-1">0,00 TRY</td>
                        </tr>
                      ) : null}
                      <tr className="border-t font-bold">
                        <td className="py-1">{t("invoices.kdv.total")}</td>
                        <td className="py-1">
                          {report.totalTaxableAmount.toLocaleString("tr-TR", { minimumFractionDigits: 2 })} TRY
                        </td>
                        <td className="py-1">
                          {report.totalCalculatedVat.toLocaleString("tr-TR", { minimumFractionDigits: 2 })} TRY
                        </td>
                      </tr>
                    </tbody>
                  </table>
                  {report.totalWithholdingVat > 0 ? (
                    <div className="p-2 rounded bg-amber-500/10 border border-amber-500/20 text-xs">
                      <strong>{t("invoices.kdv.withholding")}</strong>{" "}
                      {report.totalWithholdingVat.toLocaleString("tr-TR", { minimumFractionDigits: 2 })} TRY
                    </div>
                  ) : null}
                  {ownTaxId.trim() ? (
                    <table className="w-full text-left border-collapse text-xs">
                      <thead>
                        <tr className="border-b">
                          <th className="py-1">{t("invoices.kdv.summary")}</th>
                          <th className="py-1">{t("invoices.kdv.invoice")}</th>
                          <th className="py-1">{t("invoices.kdv.base")}</th>
                          <th className="py-1">{t("invoices.kdv.vat")}</th>
                        </tr>
                      </thead>
                      <tbody>
                        <tr>
                          <td className="py-1">{t("invoices.kdv.sales")}</td>
                          <td className="py-1">{report.sales.invoiceCount}</td>
                          <td className="py-1">
                            {report.sales.taxableAmount.toLocaleString("tr-TR", { minimumFractionDigits: 2 })} TRY
                          </td>
                          <td className="py-1">
                            {report.sales.vatAmount.toLocaleString("tr-TR", { minimumFractionDigits: 2 })} TRY
                          </td>
                        </tr>
                        <tr>
                          <td className="py-1">{t("invoices.kdv.purchases")}</td>
                          <td className="py-1">{report.purchases.invoiceCount}</td>
                          <td className="py-1">
                            {report.purchases.taxableAmount.toLocaleString("tr-TR", { minimumFractionDigits: 2 })} TRY
                          </td>
                          <td className="py-1">
                            {report.purchases.vatAmount.toLocaleString("tr-TR", { minimumFractionDigits: 2 })} TRY
                          </td>
                        </tr>
                        <tr className="border-t font-bold">
                          <td className="py-1" colSpan={3}>
                            {report.netVat >= 0 ? t("invoices.kdv.payable") : t("invoices.kdv.carry")}
                          </td>
                          <td className="py-1">
                            {Math.abs(report.netVat).toLocaleString("tr-TR", { minimumFractionDigits: 2 })} TRY
                          </td>
                        </tr>
                        {report.unclassified > 0 ? (
                          <tr>
                            <td className="py-1 muted" colSpan={4}>
                              {t.plural("invoices.kdv.unclassified", report.unclassified, {
                                count: report.unclassified,
                              })}
                            </td>
                          </tr>
                        ) : null}
                      </tbody>
                    </table>
                  ) : null}
                </div>
              );
            })()}
          </Modal>
        ) : null}
      </div>
    </Screen>
  );
}
