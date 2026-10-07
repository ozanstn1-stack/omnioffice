import type { RevocationInfo } from "../lib/api";
import { useT } from "../lib/i18n";
import { Badge } from "./ui";

/** "2026-10-07T12:34:56Z" -> "2026-10-07 12:34 UTC"; anything else is shown as given. */
export function formatRevocationTime(value: string): string {
  const match = /^(\d{4}-\d{2}-\d{2})T(\d{2}:\d{2})/.exec(value);
  return match ? `${match[1]} ${match[2]} UTC` : value;
}

/**
 * Result of the optional online revocation check, kept apart from the trust
 * badge: "not revoked" says nothing about whether the signer is trusted.
 */
export function RevocationStatus({ revocation }: { revocation?: RevocationInfo | null }) {
  const t = useT();
  const status = revocation?.status ?? "not_checked";
  const checked = revocation?.checkedAt ? formatRevocationTime(revocation.checkedAt) : "?";
  const source = revocation?.source === "crl" ? "CRL" : "OCSP";

  let tone: "default" | "ok" | "warn" | "danger" = "default";
  let text = t("studio.revocationNotChecked");
  if (status === "good") {
    tone = "ok";
    text = t("studio.revocationGood", { source, time: checked });
  } else if (status === "revoked") {
    tone = "danger";
    text = t("studio.revocationRevoked", {
      time: revocation?.revokedAt ? formatRevocationTime(revocation.revokedAt) : "?",
    });
  } else if (status === "unknown") {
    tone = "warn";
    text = t("studio.revocationUnknown");
  } else if (status === "error") {
    tone = "warn";
    text = t("studio.revocationError");
  }

  const details = [
    status === "revoked" && revocation?.reason ? t("studio.revocationReason", { reason: revocation.reason }) : null,
    status !== "good" && status !== "not_checked" && revocation?.detail ? revocation.detail : null,
  ].filter((line): line is string => Boolean(line));

  return (
    <>
      <Badge tone={tone}>{text}</Badge>
      {details.map((line, index) => (
        <p key={index} className="muted small">
          {line}
        </p>
      ))}
    </>
  );
}
