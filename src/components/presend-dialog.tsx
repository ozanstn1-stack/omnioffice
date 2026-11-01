import { useEffect, useState } from "react";
import { AlertOctagon, AlertTriangle, CheckCircle2, Info, Loader2 } from "lucide-react";
import { Button, Modal } from "./ui";
import { useT } from "../lib/i18n";
import { fileBaseName } from "../lib/format";
import { canClean, type PresendFinding, type PresendSeverity } from "../lib/presend-check";
import { cleanCopyForSending, runPresendCheck } from "../lib/presend";
import { usePresend, type PresendRequest } from "../lib/presend-gate";
import { shareDocumentFile } from "../lib/share";
import { reportError } from "../lib/store";
import type { Navigate } from "../lib/nav";

const ICONS: Record<PresendSeverity, React.ReactNode> = {
  stop: <AlertOctagon size={16} style={{ color: "var(--danger)" }} aria-hidden />,
  warn: <AlertTriangle size={16} style={{ color: "var(--warn)" }} aria-hidden />,
  info: <Info size={16} style={{ color: "var(--muted)" }} aria-hidden />,
};

type Phase =
  { kind: "checking" } | { kind: "ready"; findings: PresendFinding[] } | { kind: "failed" } | { kind: "busy" };

/**
 * "Before you send": shown by `shareWithCheck` for a PDF. Lists what the file
 * carries, then shares it as is, shares a cleaned copy, or opens the
 * redaction tool. A file the check cannot read can still be sent.
 */
export function PresendDialog({ onNavigate }: { onNavigate: Navigate }) {
  const t = useT();
  const request = usePresend((state) => state.request);
  const close = usePresend((state) => state.close);
  // The phase belongs to one request; a new request reads as "checking"
  // until its own check answers.
  const [state, setState] = useState<{ request: PresendRequest | null; phase: Phase }>({
    request: null,
    phase: { kind: "checking" },
  });
  const phase: Phase = state.request === request ? state.phase : { kind: "checking" };
  const setPhase = (next: Phase) => setState({ request, phase: next });

  useEffect(() => {
    if (!request) return;
    let live = true;
    runPresendCheck(request.path)
      .then((findings) => live && setState({ request, phase: { kind: "ready", findings } }))
      .catch(() => live && setState({ request, phase: { kind: "failed" } }));
    return () => {
      live = false;
    };
  }, [request]);

  if (!request) return null;

  const findings = phase.kind === "ready" ? phase.findings : [];
  const send = async (cleaned: boolean) => {
    setPhase({ kind: "busy" });
    try {
      const path = cleaned ? await cleanCopyForSending(request.path, request.name) : request.path;
      await shareDocumentFile(path, request.title);
      close();
    } catch (error) {
      reportError(error, t);
      close();
    }
  };
  const personalData = findings.some((finding) => finding.code === "personalData");
  const worst = findings[0]?.severity;

  return (
    <Modal
      title={t("presend.title", { name: request.name ?? fileBaseName(request.path) })}
      onClose={close}
      footer={
        <>
          <Button variant="ghost" onClick={close}>
            {t("common.cancel")}
          </Button>
          {personalData ? (
            <Button
              onClick={() => {
                close();
                onNavigate("redact", { files: [request.path] });
              }}
            >
              {t("presend.redact")}
            </Button>
          ) : null}
          {canClean(findings) ? (
            <Button variant="primary" onClick={() => void send(true)}>
              {t("presend.cleanAndSend")}
            </Button>
          ) : null}
          <Button
            variant={worst === "stop" || canClean(findings) ? "ghost" : "primary"}
            disabled={phase.kind === "checking" || phase.kind === "busy"}
            onClick={() => void send(false)}
          >
            {findings.length || phase.kind === "failed" ? t("presend.sendAnyway") : t("presend.send")}
          </Button>
        </>
      }
    >
      {phase.kind === "checking" || phase.kind === "busy" ? (
        <p className="flex items-center gap-2 text-[13.5px]" role="status">
          <Loader2 size={16} className="spin" aria-hidden />
          {phase.kind === "checking" ? t("presend.checking") : t("presend.sending")}
        </p>
      ) : phase.kind === "failed" ? (
        <p className="text-[13.5px]" role="status">
          {t("presend.failed")}
        </p>
      ) : findings.length === 0 ? (
        <p className="flex items-center gap-2 text-[13.5px]" role="status">
          <CheckCircle2 size={16} style={{ color: "var(--ok)" }} aria-hidden />
          {t("presend.allClear")}
        </p>
      ) : (
        <ul className="flex flex-col gap-2" aria-label={t("presend.findings")}>
          {findings.map((finding) => (
            <li
              key={finding.code}
              className="flex items-start gap-2 rounded-lg p-2.5"
              style={{ background: "var(--surface-2)" }}
            >
              <span className="mt-0.5">{ICONS[finding.severity]}</span>
              <span className="flex-1 min-w-0">
                <span className="block font-semibold text-[13.5px]">
                  {t(`presend.${finding.code}Title`, { count: finding.count })}
                </span>
                <span className="block text-xs muted mt-0.5 leading-relaxed">
                  {t.plural(`presend.${finding.code}Body`, finding.count, {
                    count: finding.count,
                    items: describe(finding, t),
                  })}
                </span>
              </span>
            </li>
          ))}
        </ul>
      )}
    </Modal>
  );
}

/** Details as words: data kinds through the dictionary, names as they are. */
function describe(finding: PresendFinding, t: (key: string) => string): string {
  if (finding.code === "personalData") {
    return finding.items
      .map((item) => {
        const [kind, count] = item.split(":");
        return `${t(`presend.kind.${kind}`)} ${count}`;
      })
      .join(", ");
  }
  if (finding.code === "metadata") return finding.items.map((item) => t(`presend.meta.${item}`)).join(", ");
  return finding.items.join(", ");
}
