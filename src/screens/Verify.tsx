import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  BadgeCheck,
  FileWarning,
  FolderOpen,
  Globe,
  RefreshCw,
  ShieldAlert,
  ShieldCheck,
  ShieldQuestion,
} from "lucide-react";
import { Badge, Button, Card, Spinner } from "../components/ui";
import { DropZone, FileList } from "../components/files";
import { Screen } from "../components/layout";
import { RevocationStatus } from "../components/revocation-status";
import {
  pdfVerifySignatures,
  pdfVerifySignaturesOnline,
  toAppError,
  type SignatureInfo,
  type SignatureReport,
} from "../lib/api";
import { formatBytes } from "../lib/format";
import { useT } from "../lib/i18n";
import { useScreenReady } from "../lib/screen-ready";
import { useSettings } from "../lib/store";
import { useTool } from "../lib/useTool";
import {
  signatureVerdict,
  summarizeSignatureReport,
  type SignatureVerdict,
  type VerificationVerdict,
} from "../lib/verify-signatures";

function Row({ label, value }: { label: string; value: React.ReactNode }) {
  return (
    <div
      className="flex items-baseline justify-between gap-3 py-1 border-b last:border-0"
      style={{ borderColor: "var(--border)" }}
    >
      <span className="text-xs muted">{label}</span>
      <span className="text-xs text-right break-words" style={{ color: "var(--text-1)" }}>
        {value}
      </span>
    </div>
  );
}

const VERDICT_TONE: Record<VerificationVerdict, "ok" | "warn" | "danger" | "accent"> = {
  valid: "ok",
  modified: "danger",
  invalid: "danger",
  unreadable: "warn",
  unsigned: "accent",
};

const SIGNATURE_TONE: Record<SignatureVerdict, "ok" | "warn" | "danger"> = {
  valid: "ok",
  modified: "warn",
  invalid: "danger",
};

function verdictIcon(verdict: VerificationVerdict) {
  if (verdict === "valid") return <BadgeCheck size={22} />;
  if (verdict === "modified") return <ShieldAlert size={22} />;
  if (verdict === "invalid") return <ShieldQuestion size={22} />;
  if (verdict === "unreadable") return <FileWarning size={22} />;
  return <ShieldCheck size={22} />;
}

/** One verdict for the document, then the counts that explain it. */
function VerdictCard({ report }: { report: SignatureReport }) {
  const t = useT();
  const summary = useMemo(() => summarizeSignatureReport(report), [report]);
  const verdict = summary.verdict;
  return (
    <Card>
      <div className="flex items-start gap-3">
        <span className="mt-0.5" aria-hidden>
          {verdictIcon(verdict)}
        </span>
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2 flex-wrap">
            <strong>{t(`verify.verdict.${verdict}.title`)}</strong>
            <Badge tone={VERDICT_TONE[verdict]}>{t(`verify.verdictLabel.${verdict}`)}</Badge>
          </div>
          <p className="muted small mt-1">{t(`verify.verdict.${verdict}.body`)}</p>
        </div>
      </div>
      <div className="row mt-3" style={{ flexWrap: "wrap" }}>
        <Badge>{t("verify.counts", { valid: summary.valid, count: summary.signatures })}</Badge>
        {summary.superseded > 0 ? (
          <Badge tone="warn">{t("verify.countSuperseded", { count: summary.superseded })}</Badge>
        ) : null}
        {summary.timestamped > 0 ? (
          <Badge tone="accent">{t("verify.countTimestamped", { count: summary.timestamped })}</Badge>
        ) : null}
        {summary.expired > 0 ? <Badge tone="warn">{t("verify.countExpired", { count: summary.expired })}</Badge> : null}
      </div>
    </Card>
  );
}

/** Everything the backend learned about one embedded signature. */
function SignatureCard({ signature, index }: { signature: SignatureInfo; index: number }) {
  const t = useT();
  const verdict = signatureVerdict(signature);
  return (
    <Card>
      <div className="flex items-center justify-between gap-2 flex-wrap">
        <div className="row">
          <Badge tone={SIGNATURE_TONE[verdict]}>{t(`verify.signature.${verdict}`)}</Badge>
          <strong>{signature.fieldName || t("verify.signatureFallback", { number: index + 1 })}</strong>
        </div>
        <span className="muted small">
          {signature.subFilter || "?"} · {signature.algorithm}
        </span>
      </div>
      <div className="row mt-2.5" style={{ flexWrap: "wrap" }}>
        <Badge tone={signature.digestMatches ? "ok" : "danger"}>
          {signature.digestMatches ? t("studio.digestOk") : t("studio.digestBad")}
        </Badge>
        {signature.coversWholeDocument ? <Badge tone="ok">{t("studio.coversWhole")}</Badge> : null}
        {signature.supersededByLaterRevision ? <Badge tone="warn">{t("studio.superseded")}</Badge> : null}
        {signature.timestamp ? (
          <Badge tone="accent">{t("studio.timestampPresent", { time: signature.timestamp })}</Badge>
        ) : (
          <Badge>{t("studio.timestampNone")}</Badge>
        )}
        <Badge tone="warn">{t("studio.trustUnknown")}</Badge>
      </div>
      <div className="mt-3">
        <RevocationStatus revocation={signature.revocation} />
      </div>
      <div className="stack mt-3">
        <Row label={t("studio.signer")} value={signature.signer.subject || "?"} />
        <Row label={t("studio.issuer")} value={signature.signer.issuer || "?"} />
        <Row label={t("studio.serial")} value={signature.signer.serialHex || "?"} />
        <Row
          label={t("studio.validity")}
          value={`${signature.signer.notBefore || "?"} → ${signature.signer.notAfter || "?"}${
            signature.signer.expired ? ` (${t("studio.expired")})` : ""
          }`}
        />
        {signature.signingTime ? <Row label={t("studio.signingTime")} value={signature.signingTime} /> : null}
        <Row label={t("studio.fingerprint")} value={signature.signer.sha256Fingerprint || "?"} />
      </div>
      {signature.chain.length > 1 ? (
        <div className="stack mt-3">
          <Row
            label={t("studio.chain")}
            value={signature.chainLinked ? t("studio.chainLinked") : t("studio.chainBroken")}
          />
          {signature.chain.map((certificate, position) => (
            <p key={position} className="muted small">
              {position + 1}. {certificate.subject}
              {certificate.isCa ? " (CA)" : ""}
              {certificate.expired ? ` (${t("studio.expired")})` : ""}
            </p>
          ))}
          {signature.selfSignedChain ? <p className="muted small">{t("studio.selfSigned")}</p> : null}
        </div>
      ) : null}
      {signature.notes.length ? (
        <div className="stack mt-3">
          {signature.notes.map((note, position) => (
            <p key={position} className="muted small">
              {t("studio.notes")}: {note}
            </p>
          ))}
        </div>
      ) : null}
    </Card>
  );
}

/**
 * Document verification (C9): one screen that answers "was this signed, did it
 * change after signing, and what does the certificate say?" without signing
 * anything. The verdict and the per-signature details come from the local
 * `pdf_verify_signatures` command; the online revocation check is opt-in and
 * reported separately from trust.
 */
export function VerifyScreen({ initialFiles = [], dragging = false }: { initialFiles?: string[]; dragging?: boolean }) {
  const t = useT();
  const session = useTool({ suffix: "_verified", accept: "pdf", loadInfo: false, initialPaths: initialFiles });
  const onlineEnabled = useSettings((state) => state.settings.onlineRevocationCheck);
  const [report, setReport] = useState<SignatureReport | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [onlineError, setOnlineError] = useState<string | null>(null);
  const [online, setOnline] = useState(false);
  const request = useRef(0);

  const primaryPath = session.primary?.path ?? null;

  const run = useCallback(
    async (mode: "offline" | "online") => {
      if (!primaryPath) return;
      const ticket = ++request.current;
      setBusy(true);
      setError(null);
      setOnlineError(null);
      try {
        const result =
          mode === "online" ? await pdfVerifySignaturesOnline(primaryPath) : await pdfVerifySignatures(primaryPath);
        if (request.current !== ticket) return;
        setReport(result);
        setOnline(mode === "online");
      } catch (caught) {
        if (request.current !== ticket) return;
        // A failed online check must not throw away the offline result: it is an
        // add-on, and the signature details are still the ones the user came for.
        if (mode === "online") {
          setOnlineError(toAppError(caught).message);
        } else {
          setReport(null);
          setError(toAppError(caught).message);
        }
      } finally {
        if (request.current === ticket) setBusy(false);
      }
    },
    [primaryPath],
  );

  useEffect(() => {
    // The loader owns the report/busy/error state; a new document starts a new run.
    // eslint-disable-next-line react-hooks/set-state-in-effect -- the loader owns the report state
    if (primaryPath) void run("offline");
  }, [primaryPath, run]);

  useScreenReady("verify", Boolean(primaryPath));

  return (
    <Screen
      title={t("verify.title")}
      subtitle={t("verify.subtitle")}
      actions={
        primaryPath ? (
          <Button variant="ghost" icon={<FolderOpen size={16} />} onClick={() => void session.pickFiles()}>
            {t("verify.changeFile")}
          </Button>
        ) : null
      }
    >
      {!primaryPath ? (
        <Card>
          <DropZone
            onPaths={(paths) => void session.addPaths(paths)}
            dragging={dragging}
            accept="pdf"
            title={t("verify.dropTitle")}
            hint={t("verify.dropHint")}
          />
          <div className="row mt-3">
            <Button variant="primary" icon={<FolderOpen size={16} />} onClick={() => void session.pickFiles()}>
              {t("verify.pick")}
            </Button>
          </div>
        </Card>
      ) : (
        <>
          <Card>
            <div className="flex items-center justify-between gap-2 flex-wrap">
              <div className="min-w-0">
                <strong className="break-words">{session.primary?.name}</strong>
                <p className="muted small mt-0.5">
                  {formatBytes(session.primary?.sizeBytes)} · {t("verify.localOnly")}
                </p>
              </div>
              <div className="row">
                <Button size="sm" icon={<RefreshCw size={14} />} disabled={busy} onClick={() => void run("offline")}>
                  {t("verify.refresh")}
                </Button>
                <Button
                  size="sm"
                  variant="ghost"
                  icon={<Globe size={14} />}
                  disabled={busy || !onlineEnabled}
                  onClick={() => void run("online")}
                >
                  {t("verify.online")}
                </Button>
              </div>
            </div>
            {online ? <p className="muted small mt-2">{t("verify.onlineDone")}</p> : null}
            {!onlineEnabled ? <p className="muted small mt-2">{t("verify.onlineHint")}</p> : null}
            {onlineError ? (
              <p className="small mt-2" role="status">
                <Badge tone="warn">{t("verify.onlineFailed")}</Badge> {onlineError} {t("verify.onlineHint")}
              </p>
            ) : null}
          </Card>

          {busy ? (
            <Card>
              <div className="row">
                <Spinner size={16} /> <span className="muted small">{t("verify.checking")}</span>
              </div>
            </Card>
          ) : null}

          {!busy && error ? (
            <Card>
              <div className="row">
                <Badge tone="danger">{t("errors.title")}</Badge>
                <span className="small">{error}</span>
              </div>
            </Card>
          ) : null}

          {!busy && report ? (
            <>
              <VerdictCard report={report} />
              {report.warnings.length ? (
                <Card>
                  <strong>{t("verify.warnings")}</strong>
                  <div className="stack mt-2">
                    {report.warnings.map((warning, position) => (
                      <p key={position} className="muted small">
                        {warning}
                      </p>
                    ))}
                  </div>
                </Card>
              ) : null}
              {report.signatures.map((signature, position) => (
                <SignatureCard key={position} signature={signature} index={position} />
              ))}
            </>
          ) : null}

          <Card>
            <FileList
              files={session.files}
              onRemove={session.removeFile}
              onAdd={() => void session.pickFiles()}
              addLabel={t("verify.changeFile")}
            />
          </Card>
        </>
      )}
    </Screen>
  );
}
