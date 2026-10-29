import { useState } from "react";
import { Stethoscope } from "lucide-react";
import { callPlain } from "../lib/typed-call";
import { Button, Card, Modal, TextArea } from "./ui";
import { useT } from "../lib/i18n";
import { reportError, useToasts } from "../lib/store";
import { diagnosticsReport } from "../lib/api";
import { isAndroid, saveFileBytes } from "../lib/mobile";
import { openWithDiagnosticsText } from "../lib/open-with-diagnostics";

/**
 * Diagnostics UI.
 *
 * `DiagnosticsExport` is the existing desktop-report exporter (moved here from
 * Settings.tsx so the two diagnostics entry points live together).
 *
 * `OpenWithDiagnostics` is the on-device answer to the reported "sharing opens
 * the app but the document never arrives" bug: the user has no adb, so the
 * app itself has to show what happened. It combines the three traces - the
 * Rust cache paths/pending queue, the Kotlin shell log and the frontend ring -
 * into one scrollable, copyable text. It renders nothing outside Android.
 */

/** Writes a redacted plain-text report the user can attach to an issue. */
export function DiagnosticsExport() {
  const t = useT();
  const [busy, setBusy] = useState(false);
  const exportReport = async () => {
    setBusy(true);
    try {
      const report = await diagnosticsReport();
      const stamp = new Date().toISOString().slice(0, 10);
      const saved = await saveFileBytes(new TextEncoder().encode(report), `omnioffice-diagnostics-${stamp}.txt`, {
        name: "Text",
        extensions: ["txt"],
      });
      if (saved) useToasts.getState().push({ kind: "success", title: t("settings.diagnosticsSaved"), detail: saved });
    } catch (error) {
      reportError(error, t);
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="flex flex-col gap-2">
      <div>
        <Button size="sm" variant="ghost" disabled={busy} onClick={() => void exportReport()}>
          {t("settings.diagnosticsExport")}
        </Button>
      </div>
      <p className="text-xs muted">{t("settings.diagnosticsHint")}</p>
    </div>
  );
}

/** Android-only open-with report; a no-op on every other platform. */
export function OpenWithDiagnostics() {
  const t = useT();
  const [open, setOpen] = useState(false);
  const [report, setReport] = useState("");
  const [busy, setBusy] = useState(false);
  const [copied, setCopied] = useState(false);

  const load = async () => {
    setBusy(true);
    setCopied(false);
    try {
      // The command answers on every build (a desktop stub returns ""), and a
      // failed IPC call must still show the frontend ring instead of nothing.
      const native = await callPlain((c) => c.androidOpenWithDiagnostics()).catch(() => "");
      // The frontend trace goes first: it is short and names the route that
      // was taken, while the native log can fill the visible text on a phone.
      const sections = [openWithDiagnosticsText(), native.trim()].filter((section) => section.length > 0);
      setReport(sections.join("\n\n") || t("settings.openWithDiagnosticsEmpty"));
      setOpen(true);
    } catch (error) {
      reportError(error, t);
    } finally {
      setBusy(false);
    }
  };

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(report);
      setCopied(true);
    } catch {
      // The clipboard needs a secure context and permission; fall back to
      // selecting the text so the user can copy it from the system menu.
      const pane = document.getElementById("open-with-diagnostics-report");
      if (pane instanceof HTMLTextAreaElement) {
        pane.focus();
        pane.select();
      }
    }
  };

  if (!isAndroid()) return null;
  return (
    <Card className="p-5 flex flex-col gap-4">
      <h3 className="font-semibold flex items-center gap-2">
        <Stethoscope size={16} style={{ color: "var(--accent)" }} /> {t("settings.openWithDiagnostics")}
      </h3>
      <div>
        <Button size="sm" variant="ghost" disabled={busy} onClick={() => void load()}>
          {t("settings.openWithDiagnosticsShow")}
        </Button>
        <p className="text-xs muted">{t("settings.openWithDiagnosticsHint")}</p>
      </div>
      {open ? (
        <Modal
          title={t("settings.openWithDiagnosticsTitle")}
          onClose={() => setOpen(false)}
          width={720}
          footer={
            <>
              <Button variant="ghost" onClick={() => setOpen(false)}>
                {t("common.close")}
              </Button>
              <Button variant="primary" onClick={() => void copy()}>
                {t("settings.openWithDiagnosticsCopy")}
              </Button>
            </>
          }
        >
          <p className="text-xs muted mb-2" role="status">
            {copied ? t("settings.openWithDiagnosticsCopied") : t("settings.openWithDiagnosticsHint")}
          </p>
          <TextArea
            id="open-with-diagnostics-report"
            aria-label={t("settings.openWithDiagnosticsTitle")}
            className="font-mono text-xs w-full"
            style={{ height: "46vh" }}
            readOnly
            spellCheck={false}
            value={report}
            onFocus={(event) => event.currentTarget.select()}
          />
        </Modal>
      ) : null}
    </Card>
  );
}
