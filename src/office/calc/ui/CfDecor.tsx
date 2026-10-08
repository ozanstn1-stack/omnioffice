/** What conditional formats draw inside a cell besides its fill: the data bar and the icon. */
import { ArrowRight, Circle, Flag } from "lucide-react";
import type { Translate } from "../../../lib/i18n";
import { scaleColor, type CellFormat } from "../conditional";

/** Red, amber, green: the colour of an icon tier from the lowest (0) to the highest (1). */
const RAMP = [
  { pos: 0, rgb: [0xdc, 0x26, 0x26] as [number, number, number] },
  { pos: 0.5, rgb: [0xd9, 0x77, 0x06] as [number, number, number] },
  { pos: 1, rgb: [0x05, 0x96, 0x69] as [number, number, number] },
];

/** One icon of a set. Arrows turn from pointing down to pointing up; flags and dots are coloured by tier. */
export function CfIcon({ set, tier, count, size = 12 }: { set: string; tier: number; count: number; size?: number }) {
  const color = scaleColor(RAMP, count > 1 ? tier / (count - 1) : 1);
  if (/arrow/i.test(set)) {
    const angle = count > 1 ? 90 - (180 * tier) / (count - 1) : 0;
    return <ArrowRight size={size} color={color} strokeWidth={3} style={{ transform: `rotate(${angle}deg)` }} />;
  }
  if (/flag/i.test(set)) return <Flag size={size} color={color} fill={color} />;
  return <Circle size={size} color={color} fill={color} />;
}

/** Takes the translator instead of subscribing to it: one of these is drawn for every formatted cell on screen. */
export function CfDecor({ format, t }: { format: CellFormat | null; t: Translate }) {
  if (!format) return null;
  const { bar, icon } = format;
  return (
    <>
      {bar ? <span className="data-bar" style={{ width: `${bar.fraction * 100}%`, background: bar.color }} /> : null}
      {icon ? (
        <span
          className="cf-icon"
          role="img"
          data-tier={icon.tier}
          aria-label={t("calc.iconTier", { tier: icon.rank + 1, count: icon.count })}
        >
          <CfIcon set={icon.set} tier={icon.tier} count={icon.count} />
        </span>
      ) : null}
    </>
  );
}
