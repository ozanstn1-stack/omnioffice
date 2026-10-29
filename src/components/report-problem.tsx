/**
 * "Report a problem" (C15): a privacy-preserving feedback path.
 *
 * The user sees the redacted diagnostics report before anything leaves the
 * app, copies it, and the GitHub issue form is opened with a template. There
 * is no automatic upload and the report itself never carries document content
 * (the Rust side masks file and folder names as `<path>`).
 */
import { useState } from "react";
import { Bug } from "lucide-react";
import { Badge, Button, Modal, Spinner, TextArea } from "./ui";
import { appInfo, diagnosticsReport, toAppError } from "../lib/api";
import { buildIssueUrl, draftIssue } from "../lib/feedback";
import { useT } from "../lib/i18n";
import { openExternalLink } from "../office/calc/open-link";
import { useToasts } from "../lib/store";

/** Copies text, with a textarea fallback for restricted webviews. */
async function copyToClipboard(value: string): Promise<boolean> {
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(value);
      return true;
    }
  } catch {
    // Fall through to the selection-based copy below.
  }
  try {
    const area = document.createElement("textarea");
    area.value = value;
    area.style.position = "fixed";
    area.style.opacity = "0";
    document.body.appendChild(area);
    area.select();
    const copied = document.execCommand("copy");
    document.body.removeChild(area);
    return copied;
  } catch {
    return false;
  }
}

/**
 * The Settings entry point. It builds the report only after the click (never
 * on mount), shows it for review, and then copies it and/or opens the issue.
 */
export function ReportProblem() {
  const t = useT();
  const push = useToasts((state) => state.push);
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [report, setReport] = useState("");
  const [version, setVersion] = useState("");

  const collect = async () => {
    setOpen(true);
    setBusy(true);
    setError(null);
    setReport("");
    try {
      const [text, info] = await Promise.all([diagnosticsReport(), appInfo().catch(() => null)]);
      setReport(text);
      setVersion(info?.appVersion ?? "");
    } catch (caught) {
      setError(toAppError(caught).message);
    } finally {
      setBusy(false);
    }
  };

  const copy = async (): Promise<boolean> => {
    const copied = await copyToClipboard(report);
    push(
      copied ? { kind: "success", title: t("feedback.copied") } : { kind: "error", title: t("feedback.copyFailed") },
    );
    return copied;
  };

  const openIssue = async () => {
    // Copy first: if the browser opens and the clipboard is empty the user has
    // to run the flow again. A failed copy does not block the issue page.
    await copy();
    const url = buildIssueUrl(draftIssue(t("feedback.issueBody"), t("feedback.issueTitle"), version));
    if (!(await openExternalLink(url))) push({ kind: "error", title: t("feedback.githubFailed") });
  };

  return (
    <>
      <div className="flex flex-col gap-2">
        <div>
          <Button size="sm" variant="ghost" icon={<Bug size={14} />} onClick={() => void collect()}>
            {t("feedback.button")}
          </Button>
        </div>
        <p className="text-xs muted">{t("feedback.note")}</p>
      </div>
      {open ? (
        <Modal
          title={t("feedback.title")}
          onClose={() => setOpen(false)}
          width={720}
          footer={
            <div className="row" style={{ justifyContent: "flex-end", flexWrap: "wrap" }}>
              <Button size="sm" variant="ghost" onClick={() => setOpen(false)}>
                {t("feedback.close")}
              </Button>
              <Button size="sm" disabled={busy || !report} onClick={() => void copy()}>
                {t("feedback.copy")}
              </Button>
              <Button size="sm" variant="primary" disabled={busy || !report} onClick={() => void openIssue()}>
                {t("feedback.github")}
              </Button>
            </div>
          }
        >
          <div className="stack">
            <p className="muted small">{t("feedback.intro")}</p>
            <label className="text-xs muted" htmlFor="report-problem-preview">
              {t("feedback.previewLabel")}
            </label>
            {busy ? (
              <div className="row">
                <Spinner size={14} /> <span className="muted small">{t("feedback.loading")}</span>
              </div>
            ) : null}
            {error ? (
              <p className="small" role="status">
                <Badge tone="danger">{t("errors.title")}</Badge> {error}
              </p>
            ) : null}
            <TextArea
              id="report-problem-preview"
              readOnly
              rows={14}
              value={report}
              aria-label={t("feedback.previewLabel")}
              style={{ fontFamily: "var(--font-mono, monospace)", fontSize: 12 }}
            />
            <p className="muted small">{t("feedback.privacy")}</p>
          </div>
        </Modal>
      ) : null}
    </>
  );
}
