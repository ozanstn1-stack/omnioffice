import { BadgeCheck, PenLine, RefreshCw, ShieldCheck } from "lucide-react";
import { useT } from "../../lib/i18n";
import type { LtvReport, SignatureInfo, SignatureReport, SigningCertificateSummary, SignResult } from "../../lib/api";
import { Badge, Card, Field, Toggle } from "../../components/ui";
import { RevocationStatus } from "../../components/revocation-status";

function signatureTone(info: SignatureInfo): "ok" | "warn" | "danger" {
  if (!info.signatureValid || !info.digestMatches) return "danger";
  if (info.modifiedAfterSigning || !info.coversWholeDocument) return "warn";
  return "ok";
}

export interface SignaturesPanelProps {
  input: string | undefined;
  windows: boolean;
  verifying: boolean;
  archiving: boolean;
  signing: boolean;
  ltvReport: LtvReport | null;
  signReport: SignatureReport | null;
  certificates: SigningCertificateSummary[];
  certIndex: number | null;
  setCertIndex: (value: number | null) => void;
  pfxPath: string | null;
  setPfxPath: (value: string | null) => void;
  pfxName: string;
  pfxPassword: string;
  setPfxPassword: (value: string) => void;
  shownPage: number;
  setPage: (value: number) => void;
  reason: string;
  setReason: (value: string) => void;
  location: string;
  setLocation: (value: string) => void;
  signerName: string;
  setSignerName: (value: string) => void;
  appearance: boolean;
  setAppearance: (value: boolean) => void;
  tsaUrl: string;
  setTsaUrl: (value: string) => void;
  lastResult: SignResult | null;
  verifySignatures: () => Promise<void>;
  archiveValidationData: () => Promise<void>;
  loadCertificates: () => Promise<void>;
  choosePfx: () => Promise<void>;
  signDocument: () => Promise<void>;
}

export function SignaturesPanel(props: SignaturesPanelProps) {
  const t = useT();
  const {
    input,
    windows,
    verifying,
    archiving,
    signing,
    ltvReport,
    signReport,
    certificates,
    certIndex,
    setCertIndex,
    pfxPath,
    setPfxPath,
    pfxName,
    pfxPassword,
    setPfxPassword,
    shownPage,
    setPage,
    reason,
    setReason,
    location,
    setLocation,
    signerName,
    setSignerName,
    appearance,
    setAppearance,
    tsaUrl,
    setTsaUrl,
    lastResult,
    verifySignatures,
    archiveValidationData,
    loadCertificates,
    choosePfx,
    signDocument,
  } = props;

  return (
    <>
      <Card>
        <strong>{t("studio.signatures")}</strong>
        <p className="muted small">{t("studio.signaturesHint")}</p>
        <div className="row">
          <button
            type="button"
            className="btn btn-soft"
            disabled={!input || verifying || signing}
            onClick={() => void verifySignatures()}
          >
            <ShieldCheck size={14} /> {t("studio.verify")}
          </button>
          <button
            type="button"
            className="btn btn-soft"
            disabled={!input || archiving || signing}
            onClick={() => void archiveValidationData()}
          >
            <ShieldCheck size={14} /> {t("studio.ltv")}
          </button>
        </div>
        <p className="muted small">{t("studio.ltvHint")}</p>
        {ltvReport ? (
          <p className="muted small" style={{ marginTop: 10 }}>
            <Badge tone="ok">{t("studio.ltvDone")}</Badge> {t("studio.ltvResult")}: {ltvReport.certificates}
            {ltvReport.warnings.map((warning, index) => (
              <span key={index} className="muted small">
                {" "}
                {warning}
              </span>
            ))}
          </p>
        ) : null}
        {signReport ? (
          <>
            {signReport.warnings.map((warning, index) => (
              <p key={index} className="muted small" style={{ marginTop: 10 }}>
                <Badge tone="warn">{t("studio.notes")}</Badge> {warning}
              </p>
            ))}
            {signReport.signatures.length ? (
              signReport.signatures.map((info, index) => (
                <div key={index} className="stack" style={{ marginTop: 12 }}>
                  <div className="row">
                    <Badge tone={signatureTone(info)}>
                      {info.signatureValid ? t("studio.signatureValid") : t("studio.signatureInvalid")}
                    </Badge>
                    <Badge tone={info.digestMatches ? "ok" : "danger"}>
                      {info.digestMatches ? t("studio.digestOk") : t("studio.digestBad")}
                    </Badge>
                    <Badge tone={info.coversWholeDocument ? "ok" : "warn"}>{t("studio.coversWhole")}</Badge>
                    {info.modifiedAfterSigning ? <Badge tone="danger">{t("studio.modifiedAfter")}</Badge> : null}
                    {info.supersededByLaterRevision ? <Badge tone="warn">{t("studio.superseded")}</Badge> : null}
                    <Badge tone="warn">{t("studio.trustUnknown")}</Badge>
                  </div>
                  <div className="row">
                    <RevocationStatus revocation={info.revocation} />
                  </div>
                  <p className="muted small">
                    <strong>{info.fieldName}</strong> · {info.subFilter || "?"} · {info.algorithm}
                  </p>
                  <p className="muted small">
                    {t("studio.signer")}: {info.signer.subject || "?"}
                  </p>
                  <p className="muted small">
                    {t("studio.issuer")}: {info.signer.issuer || "?"}
                  </p>
                  <p className="muted small">
                    {t("studio.serial")}: {info.signer.serialHex || "?"} · {t("studio.validity")}:{" "}
                    {info.signer.notBefore || "?"} → {info.signer.notAfter || "?"}
                    {info.signer.expired ? ` (${t("studio.expired")})` : ""}
                  </p>
                  {info.signingTime ? (
                    <p className="muted small">
                      {t("studio.signingTime")}: {info.signingTime}
                    </p>
                  ) : null}
                  <p className="muted small">
                    {t("studio.fingerprint")}: {info.signer.sha256Fingerprint || "?"}
                  </p>
                  {info.chain.length > 1 ? (
                    <div className="stack">
                      <p className="muted small">
                        {t("studio.chain")}: {info.chainLinked ? t("studio.chainLinked") : t("studio.chainBroken")}
                        {info.selfSignedChain ? ` · ${t("studio.selfSigned")}` : ""}
                      </p>
                      {info.chain.map((cert, position) => (
                        <p key={position} className="muted small">
                          {position + 1}. {cert.subject}
                          {cert.isCa ? ` (${t("studio.caBadge")})` : ""}
                          {cert.expired ? ` (${t("studio.expired")})` : ""}
                        </p>
                      ))}
                    </div>
                  ) : null}
                  {info.notes.map((note, position) => (
                    <p key={position} className="muted small">
                      {t("studio.notes")}: {note}
                    </p>
                  ))}
                </div>
              ))
            ) : signReport.warnings.length ? null : (
              <p className="muted small" style={{ marginTop: 10 }}>
                {t("studio.noSignatures")}
              </p>
            )}
          </>
        ) : null}
      </Card>

      <Card>
        <strong>{t("studio.signSection")}</strong>
        <div className="stack">
          {windows ? (
            <div className="stack">
              <div className="row">
                <span className="muted small">{t("studio.storeCertificates")}</span>
                <button type="button" className="btn btn-soft" onClick={() => void loadCertificates()}>
                  <RefreshCw size={13} /> {t("studio.refresh")}
                </button>
              </div>
              {certificates.length ? (
                <select
                  value={certIndex ?? ""}
                  onChange={(event) => {
                    setCertIndex(Number(event.target.value));
                    setPfxPath(null);
                  }}
                >
                  {certificates.map((cert) => (
                    <option key={cert.index} value={cert.index}>
                      {cert.subject}
                      {cert.expired ? ` (${t("studio.expired")})` : ""}
                      {cert.hasPrivateKey ? "" : ` - ${t("studio.noPrivateKey")}`}
                    </option>
                  ))}
                </select>
              ) : (
                <p className="muted small">{t("studio.noStoreCertificates")}</p>
              )}
              <p className="muted small">{t("studio.storeSigningNote")}</p>
            </div>
          ) : (
            <div className="stack">
              <p className="muted small">{t("studio.androidPfxNote")}</p>
              {pfxPath ? null : <p className="muted small">{t("studio.noPfxSelected")}</p>}
            </div>
          )}

          <div className="row">
            <button type="button" className="btn btn-soft" onClick={() => void choosePfx()}>
              {t("studio.choosePfx")}
            </button>
            {pfxName ? <span className="muted small">{pfxName}</span> : null}
          </div>
          {pfxPath ? (
            <Field label={t("studio.pfxPassword")} hint={t("studio.pfxHint")}>
              <input
                className="input"
                type="password"
                value={pfxPassword}
                onChange={(event) => setPfxPassword(event.target.value)}
                autoComplete="off"
                spellCheck={false}
              />
            </Field>
          ) : null}

          <div className="row">
            <Field label={t("studio.page")}>
              <input
                className="input"
                type="number"
                min={1}
                value={shownPage}
                onChange={(event) => setPage(Math.max(1, Number(event.target.value) || 1))}
                style={{ width: 90 }}
              />
            </Field>
            <Field label={t("studio.reason")}>
              <input className="input" value={reason} onChange={(event) => setReason(event.target.value)} />
            </Field>
            <Field label={t("studio.location")}>
              <input className="input" value={location} onChange={(event) => setLocation(event.target.value)} />
            </Field>
            <Field label={t("studio.signerName")}>
              <input className="input" value={signerName} onChange={(event) => setSignerName(event.target.value)} />
            </Field>
          </div>
          <Toggle checked={appearance} onChange={setAppearance} label={t("studio.visibleAppearance")} />
          <Field label={t("studio.tsaUrl")} hint={t("studio.tsaUrlHint")}>
            <input
              className="input"
              value={tsaUrl}
              spellCheck={false}
              placeholder={t("studio.tsaPlaceholder")}
              onChange={(event) => setTsaUrl(event.target.value)}
            />
          </Field>

          <button
            type="button"
            className="btn btn-primary"
            disabled={!input || signing || verifying || (!pfxPath && certIndex === null)}
            onClick={() => void signDocument()}
          >
            <PenLine size={14} /> {t("studio.signSave")}
          </button>

          {lastResult ? (
            <div className="stack">
              <Badge tone={lastResult.signature.signatureValid ? "ok" : "danger"}>
                <BadgeCheck size={12} />{" "}
                {lastResult.signature.signatureValid ? t("studio.signatureValid") : t("studio.signatureInvalid")}
              </Badge>
              <p className="muted small">{lastResult.output}</p>
              <p className="muted small">
                {lastResult.signature.timestamp
                  ? t("studio.timestampPresent", { time: lastResult.signature.timestamp })
                  : t("studio.timestampNone")}
              </p>
            </div>
          ) : null}
        </div>
      </Card>
    </>
  );
}
