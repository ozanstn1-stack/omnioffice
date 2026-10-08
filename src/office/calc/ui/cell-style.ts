/** The text look of a grid cell: its own format, the table header, a link and conditional formats, in that order of precedence. */
import type { CSSProperties } from "react";
import type { CellStyle } from "../../../lib/office-types";

/** A hyperlink with no colour of its own uses the accent colour. */
const LINK_COLOR = "var(--accent)";

export function cellTextStyle(input: {
  style: CellStyle;
  /** Numbers align right under the "general" alignment. */
  numeric: boolean;
  /** The cell is in the bold header row of a structured table. */
  headerBold: boolean;
  /** The cell sits on a table header fill, which wants white text. */
  onHeaderFill: boolean;
  linked: boolean;
  /** What conditional formats add on top of the cell's own format. */
  rule?: { bold?: boolean; italic?: boolean; color?: string | null } | null;
}): CSSProperties {
  const { style, numeric, rule } = input;
  const decoration = [style.underline || input.linked ? "underline" : "", style.strike ? "line-through" : ""]
    .filter(Boolean)
    .join(" ");
  const alignRight = style.align === "right" || (style.align === "general" && numeric);
  return {
    fontWeight: style.bold || input.headerBold || rule?.bold ? 700 : undefined,
    fontStyle: style.italic || rule?.italic ? "italic" : undefined,
    textDecoration: decoration || undefined,
    color: rule?.color ?? style.color ?? (input.onHeaderFill ? "#ffffff" : input.linked ? LINK_COLOR : undefined),
    textAlign: (style.align === "general" ? (numeric ? "right" : "left") : style.align) as "left" | "right" | "center",
    justifyContent: style.align === "center" ? "center" : alignRight ? "flex-end" : "flex-start",
  };
}
