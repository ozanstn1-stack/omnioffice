import { Check } from "lucide-react";
import { useT } from "../lib/i18n";
import { useSettings } from "../lib/store";
import { ACCENTS, ACCENT_SWATCH } from "../lib/accent";

/**
 * Settings > Appearance: the four accent colours of the approved design as a
 * radio group of swatches. The midnight and paper themes keep their own
 * palette, which the hint under the swatches says.
 */
export function AccentPicker() {
  const t = useT();
  const accent = useSettings((state) => state.settings.accent ?? "indigo");
  const theme = useSettings((state) => state.settings.theme);
  const update = useSettings((state) => state.update);
  const ownPalette = theme === "midnight" || theme === "paper";

  return (
    <div>
      <p className="label" id="accent-label">
        {t("settings.accent")}
      </p>
      <div className="accent-swatches" role="radiogroup" aria-labelledby="accent-label">
        {ACCENTS.map((name) => (
          <button
            key={name}
            type="button"
            role="radio"
            aria-checked={accent === name}
            aria-label={t(`settings.accent.${name}`)}
            title={t(`settings.accent.${name}`)}
            className="accent-swatch"
            style={{ background: ACCENT_SWATCH[name] }}
            disabled={ownPalette}
            onClick={() => void update({ accent: name })}
          >
            {accent === name ? <Check size={16} aria-hidden /> : null}
          </button>
        ))}
      </div>
      {ownPalette ? <p className="text-xs muted mt-1">{t("settings.accentOwnPalette")}</p> : null}
    </div>
  );
}

/** Interface density selector (Comfortable vs Compact). */
export function DensityPicker() {
  const t = useT();
  const density = useSettings((state) => state.settings.uiDensity ?? "comfortable");
  const update = useSettings((state) => state.update);
  return (
    <div>
      <p className="label">{t("settings.density")}</p>
      <div className="seg">
        <button
          type="button"
          data-active={density === "comfortable"}
          onClick={() => void update({ uiDensity: "comfortable" })}
        >
          {t("settings.densityComfortable")}
        </button>
        <button type="button" data-active={density === "compact"} onClick={() => void update({ uiDensity: "compact" })}>
          {t("settings.densityCompact")}
        </button>
      </div>
    </div>
  );
}

/** Opt-in OCSP/CRL lookup for signer certificates. */
export function RevocationControl() {
  const t = useT();
  const enabled = useSettings((s) => s.settings.onlineRevocationCheck === true);
  const update = useSettings((s) => s.update);
  return (
    <div className="flex flex-col gap-2">
      <label className="checkbox">
        <input
          type="checkbox"
          checked={enabled}
          onChange={(event) => void update({ onlineRevocationCheck: event.target.checked })}
        />
        <span>{t("settings.onlineRevocation")}</span>
      </label>
      <p className="text-xs muted">{t("settings.onlineRevocationHint")}</p>
    </div>
  );
}
