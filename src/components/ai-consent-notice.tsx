import { Check } from "lucide-react";
import { Button, Checkbox } from "./ui";
import { useT } from "../lib/i18n";

interface AiConsentNoticeProps {
  /** Consent given for this document, or remembered for this provider. */
  consent: boolean;
  /** The consent comes from the remembered setting. */
  remembered: boolean;
  /** i18n keys for the notice text and the checkbox (local vs. remote provider). */
  notice: { consent: string; privacyBody: string };
  params: Record<string, string>;
  onConsent: (value: boolean) => void;
  onRemember: () => void;
  onForget: () => void;
}

/** The data notice above the AI actions: ask, confirm, or show the remembered consent. */
export function AiConsentNotice({
  consent,
  remembered,
  notice,
  params,
  onConsent,
  onRemember,
  onForget,
}: AiConsentNoticeProps) {
  const t = useT();
  if (!consent) {
    return (
      <>
        <p className="text-[13px] muted">{t(notice.privacyBody, params)}</p>
        <Checkbox checked={false} onChange={onConsent} label={t(notice.consent, params)} />
      </>
    );
  }
  return (
    <div className="flex items-center gap-2 text-xs flex-wrap">
      <span className="flex items-center gap-1.5" style={{ color: "var(--ok)" }}>
        <Check size={13} /> {remembered ? t("ai.consentRemembered", params) : t("ai.consentShort")}
      </span>
      {remembered ? (
        <Button size="sm" variant="ghost" onClick={onForget}>
          {t("ai.consentForget")}
        </Button>
      ) : (
        <>
          <Button size="sm" variant="ghost" onClick={() => onConsent(false)}>
            {t("ai.consentChange")}
          </Button>
          <Button size="sm" variant="ghost" onClick={onRemember}>
            {t("ai.consentRemember", params)}
          </Button>
        </>
      )}
    </div>
  );
}
