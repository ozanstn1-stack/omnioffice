/**
 * The derivation chain of a document (D7): its recorded sources, the
 * transformation step, the output digest and the tamper warning.
 *
 * Split out of `provenance-card.tsx` so the card only orchestrates reading,
 * writing and the empty states.
 */
import { Fingerprint, ShieldAlert, ShieldCheck } from "lucide-react";
import { Badge } from "./ui";
import { formatBytes } from "../lib/format";
import { useT } from "../lib/i18n";
import type { SourceChainReport } from "../lib/provenance-api";

function shortDigest(digest: string): string {
  return digest.length > 16 ? `${digest.slice(0, 16)}…` : digest;
}

export function ProvenanceChain({ chain, savedTo }: { chain: SourceChainReport; savedTo: string | null }) {
  const t = useT();
  const changed = chain.verdict === "changed";
  const stepLabel = (kind: string) =>
    kind === "source"
      ? t("provenance.chainStepSource")
      : kind === "transform"
        ? t("provenance.chainStepTransform")
        : t("provenance.chainStepOutput");
  return (
    <div className="stack mt-3">
      {changed ? (
        <div className="row" role="alert" style={{ alignItems: "flex-start", gap: 8 }}>
          <ShieldAlert size={16} style={{ color: "var(--danger)", flexShrink: 0, marginTop: 2 }} />
          <p className="small" style={{ color: "var(--danger)" }}>
            <strong>{t("provenance.changed")}</strong> — {t("provenance.chainTampered")}
          </p>
        </div>
      ) : null}
      {savedTo ? <p className="muted small">{t("provenance.savedTo", { path: savedTo })}</p> : null}
      <div className="flex flex-wrap gap-4">
        <p className="muted small">
          {t("provenance.module")}: <strong>{chain.module}</strong>
        </p>
        <p className="muted small">
          {t("provenance.createdAt")}: <strong>{chain.createdAt}</strong>
        </p>
        <p className="muted small">
          {t("provenance.appVersion")}: <strong>{chain.appVersion || "?"}</strong>
        </p>
      </div>
      {chain.note ? <p className="small">{chain.note}</p> : null}
      <div className="stack">
        <p className="text-xs muted">{t("provenance.chain")}</p>
        <p className="text-xs muted">{t("provenance.chainNote")}</p>
        <ol className="stack" style={{ listStyle: "none", padding: 0, margin: 0 }}>
          {chain.links.map((link, position) => (
            <li key={`${link.kind}-${position}`} className="row" style={{ alignItems: "baseline", gap: 8 }}>
              <span className="muted font-mono text-xs">{position + 1}.</span>
              <Badge tone={link.kind === "output" ? "ok" : link.kind === "transform" ? "accent" : "warn"}>
                {stepLabel(link.kind)}
              </Badge>
              <span className="truncate">{link.label}</span>
              {link.size !== null && link.size !== undefined && link.kind !== "transform" ? (
                <span className="muted small">{formatBytes(link.size)}</span>
              ) : null}
              {link.sha256 ? <span className="muted small font-mono">{shortDigest(link.sha256)}</span> : null}
            </li>
          ))}
        </ol>
      </div>
      {chain.ai ? (
        <p className="muted small">
          {t("provenance.ai")}: {chain.ai.provider} · {chain.ai.model}
          {chain.ai.citations ? ` · ${t("provenance.citations", { count: chain.ai.citations })}` : ""}
        </p>
      ) : null}
      <p className="muted small row">
        <Fingerprint size={13} /> {t("provenance.recorded")}: {shortDigest(chain.recordedSha256)}
      </p>
      <p className="muted small row">
        <ShieldCheck size={13} /> {t("provenance.outputDigest")}: {shortDigest(chain.outputSha256)}
      </p>
      {(chain.warnings ?? []).length ? (
        <ul className="stack" style={{ margin: 0, paddingLeft: 18 }}>
          {(chain.warnings ?? []).map((warning, position) => (
            <li key={position} className="muted small">
              {warning}
            </li>
          ))}
        </ul>
      ) : null}
    </div>
  );
}
