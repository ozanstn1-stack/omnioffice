/**
 * PDF Studio: the V3.0 document-health tools - sanitizer, flattening and
 * PDF/A validation/conversion. Every result shown here comes from a real
 * check in pdfcore; nothing is reported as "compliant" without validation.
 */
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { FileCheck2, Layers, ShieldAlert, ShieldCheck } from "lucide-react";
import { useT } from "../lib/i18n";
import { errorMessage, useToasts } from "../lib/store";
import { DropZone, FileList } from "../components/files";
import { Badge, Card } from "../components/ui";

type StudioTab = "sanitize" | "flatten" | "pdfa";

interface ProgressEvent {
  jobId: string;
  stage: string;
  current: number;
  total: number;
  message?: string;
}

interface SanitizeReport {
  javascriptRemoved: number;
  embeddedFilesRemoved: number;
  actionsRemoved: number;
  metadataRemoved: boolean;
  annotationsRemoved: number;
  linksRemoved: number;
  warnings: string[];
}

interface FlattenReport {
  annotationsFlattened: number;
  fieldsFlattened: number;
  pagesTouched: number;
  warnings: string[];
}

interface PdfaCheck {
  id: string;
  level: string;
  status: string;
  message: string;
}

interface PdfaReport {
  valid: boolean;
  level: string;
  checks: PdfaCheck[];
  failures: number;
  warnings: number;
  converted: boolean;
}

export function PdfStudio({ initialFiles, dragging }: { initialFiles?: string[]; dragging?: boolean }) {
  const t = useT();
  const [tab, setTab] = useState<StudioTab>("sanitize");
  const [files, setFiles] = useState<string[]>(initialFiles ?? []);
  const [level, setLevel] = useState("A-2b");
  const [running, setRunning] = useState<string | null>(null);
  const [progress, setProgress] = useState<ProgressEvent | null>(null);
  const [sanitizeReport, setSanitizeReport] = useState<SanitizeReport | null>(null);
  const [flattenReport, setFlattenReport] = useState<FlattenReport | null>(null);
  const [pdfaReport, setPdfaReport] = useState<PdfaReport | null>(null);

  useEffect(() => {
    if (initialFiles?.length) setFiles(initialFiles);
  }, [initialFiles]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void listen<ProgressEvent>("job:progress", (event) => {
      if (running && event.payload.jobId === running) setProgress(event.payload);
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, [running]);

  const input = files[0];
  const toast = (kind: "success" | "error", title: string, detail?: string) => useToasts.getState().push({ kind, title, detail });

  const run = async (action: () => Promise<void>, jobId: string) => {
    if (!input) {
      toast("error", t("studio.needFile"));
      return;
    }
    setRunning(jobId);
    setProgress(null);
    try {
      await action();
    } catch (reason) {
      toast("error", t("errors.title"), errorMessage(reason, t));
    } finally {
      setRunning(null);
      setProgress(null);
    }
  };

  const runSanitize = () =>
    run(async () => {
      const report = await invoke<SanitizeReport>("sanitize_pdf", {
        request: { input, jobId: "studio-sanitize", options: null },
      });
      setSanitizeReport(report);
      toast("success", t("studio.sanitizeDone"));
    }, "studio-sanitize");

  const runFlatten = () =>
    run(async () => {
      const report = await invoke<FlattenReport>("flatten_pdf", {
        request: { input, jobId: "studio-flatten", options: null },
      });
      setFlattenReport(report);
      toast("success", t("studio.flattenDone"));
    }, "studio-flatten");

  const runValidate = () =>
    run(async () => {
      const report = await invoke<PdfaReport>("pdfa_validate", { request: { input, level } });
      setPdfaReport(report);
    }, "studio-pdfa-validate");

  const runConvert = () =>
    run(async () => {
      const report = await invoke<PdfaReport>("pdfa_convert", { request: { input, level, jobId: "studio-pdfa" } });
      setPdfaReport(report);
      toast(report.valid ? "success" : "error", report.valid ? t("studio.pdfaValid") : t("studio.pdfaStillFailing"));
    }, "studio-pdfa");

  const tabs: { id: StudioTab; label: string; icon: React.ReactElement }[] = [
    { id: "sanitize", label: t("studio.sanitize"), icon: <ShieldAlert size={14} /> },
    { id: "flatten", label: t("studio.flatten"), icon: <Layers size={14} /> },
    { id: "pdfa", label: t("studio.pdfa"), icon: <FileCheck2 size={14} /> },
  ];

  return (
    <div className="screen">
      <div className="screen-head">
        <div>
          <h1>
            <ShieldCheck size={18} /> {t("studio.title")}
          </h1>
          <p className="muted">{t("studio.subtitle")}</p>
        </div>
      </div>

      <div className="row" style={{ marginBottom: 12 }}>
        {tabs.map((entry) => (
          <button key={entry.id} type="button" className="btn btn-soft" data-active={tab === entry.id} onClick={() => setTab(entry.id)}>
            {entry.icon} {entry.label}
          </button>
        ))}
      </div>

      <Card>
        <DropZone onPaths={(paths) => setFiles(paths)} dragging={dragging ?? false} accept="pdf" />
        <FileList
          files={files.map((path) => ({ path, name: path.split(/[\\/]/).pop() ?? path, sizeBytes: 0 }))}
          onRemove={(index) => setFiles((current) => current.filter((_, position) => position !== index))}
          onAdd={async () => {
            const picked = await open({ multiple: false, filters: [{ name: "PDF", extensions: ["pdf"] }] });
            if (!picked) return;
            setFiles([String(Array.isArray(picked) ? picked[0] : picked)]);
          }}
          addLabel={t("common.addPdf")}
        />
      </Card>

      {progress ? (
        <Card soft>
          <p className="muted small">
            {progress.stage} {progress.total > 0 ? `${progress.current}/${progress.total}` : ""} {progress.message ?? ""}
          </p>
        </Card>
      ) : null}

      {tab === "sanitize" ? (
        <Card>
          <strong>{t("studio.sanitize")}</strong>
          <p className="muted small">{t("studio.sanitizeHint")}</p>
          <button type="button" className="btn btn-primary" disabled={!input || running !== null} onClick={() => void runSanitize()}>
            {t("studio.run")}
          </button>
          {sanitizeReport ? (
            <div className="stack" style={{ marginTop: 10 }}>
              <div className="row">
                <Badge tone="ok">{t("studio.javascript")}: {sanitizeReport.javascriptRemoved}</Badge>
                <Badge tone="ok">{t("studio.attachments")}: {sanitizeReport.embeddedFilesRemoved}</Badge>
                <Badge tone="ok">{t("studio.actions")}: {sanitizeReport.actionsRemoved}</Badge>
                <Badge tone={sanitizeReport.metadataRemoved ? "ok" : "warn"}>{t("studio.metadata")}</Badge>
                <Badge tone="accent">{t("studio.annotations")}: {sanitizeReport.annotationsRemoved}</Badge>
              </div>
              {sanitizeReport.warnings.map((warning, index) => (
                <p key={index} className="muted small">{warning}</p>
              ))}
              <p className="muted small">{t("studio.sanitizeVerify")}</p>
            </div>
          ) : null}
        </Card>
      ) : null}

      {tab === "flatten" ? (
        <Card>
          <strong>{t("studio.flatten")}</strong>
          <p className="muted small">{t("studio.flattenHint")}</p>
          <button type="button" className="btn btn-primary" disabled={!input || running !== null} onClick={() => void runFlatten()}>
            {t("studio.run")}
          </button>
          {flattenReport ? (
            <div className="stack" style={{ marginTop: 10 }}>
              <div className="row">
                <Badge tone="ok">{t("studio.annotations")}: {flattenReport.annotationsFlattened}</Badge>
                <Badge tone="ok">{t("studio.fields")}: {flattenReport.fieldsFlattened}</Badge>
                <Badge tone="accent">{t("studio.pages")}: {flattenReport.pagesTouched}</Badge>
              </div>
              {flattenReport.warnings.map((warning, index) => (
                <p key={index} className="muted small">{warning}</p>
              ))}
            </div>
          ) : null}
        </Card>
      ) : null}

      {tab === "pdfa" ? (
        <Card>
          <strong>{t("studio.pdfa")}</strong>
          <p className="muted small">{t("studio.pdfaHint")}</p>
          <div className="row">
            <select value={level} onChange={(event) => setLevel(event.target.value)}>
              <option value="A-1b">PDF/A-1b</option>
              <option value="A-2b">PDF/A-2b</option>
              <option value="A-3b">PDF/A-3b</option>
            </select>
            <button type="button" className="btn btn-soft" disabled={!input || running !== null} onClick={() => void runValidate()}>
              {t("studio.validate")}
            </button>
            <button type="button" className="btn btn-primary" disabled={!input || running !== null} onClick={() => void runConvert()}>
              {t("studio.convert")}
            </button>
          </div>
          {pdfaReport ? (
            <div className="stack" style={{ marginTop: 10 }}>
              <div className="row">
                <Badge tone={pdfaReport.valid ? "ok" : "danger"}>
                  {pdfaReport.valid ? t("studio.pdfaValid") : t("studio.pdfaInvalid")}
                </Badge>
                <span className="muted small">
                  {pdfaReport.level} · {pdfaReport.failures} {t("studio.failures")} · {pdfaReport.warnings} {t("studio.warnings")}
                </span>
              </div>
              {pdfaReport.checks.map((check) => (
                <div key={check.id} className="row">
                  <Badge tone={check.status === "pass" ? "ok" : check.status === "warning" ? "warn" : "danger"}>{check.status}</Badge>
                  <strong className="small">{check.id}</strong>
                  <span className="muted small">{check.message}</span>
                </div>
              ))}
            </div>
          ) : null}
        </Card>
      ) : null}
    </div>
  );
}
