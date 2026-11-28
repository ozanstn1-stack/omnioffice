/**
 * Effective paragraph/run style resolution for Writer documents.
 */
import type { ParaProps, ParaStyle, TextDocument } from "./office-types";

export interface ParaStyleLike {
  font?: string | null;
  sizePt?: number | null;
  bold?: boolean | null;
  italic?: boolean | null;
  underline?: boolean | null;
  strike?: boolean | null;
  color?: string | null;
  highlight?: string | null;
  align?: string | null;
  lineSpacing?: number | null;
  spaceBeforePt?: number | null;
  spaceAfterPt?: number | null;
  indentLeftPt?: number | null;
  indentRightPt?: number | null;
  firstLinePt?: number | null;
}

/** Resolves a style chain plus paragraph overrides to concrete values. */
export function effectiveStyle(
  document: TextDocument,
  props: ParaProps,
): Required<
  Pick<
    ParaStyleLike,
    | "font"
    | "sizePt"
    | "bold"
    | "italic"
    | "underline"
    | "strike"
    | "color"
    | "align"
    | "lineSpacing"
    | "spaceBeforePt"
    | "spaceAfterPt"
    | "indentLeftPt"
    | "indentRightPt"
    | "firstLinePt"
  >
> & { highlight: string | null } {
  const chain: ParaStyle[] = [];
  let current: string | null = props.style;
  for (let depth = 0; depth < 8 && current; depth += 1) {
    const style = document.styles.find((candidate) => candidate.id === current);
    if (!style) break;
    chain.unshift(style);
    current = style.basedOn;
  }
  let result = {
    font: "Calibri",
    sizePt: 11,
    bold: false,
    italic: false,
    underline: false,
    strike: false,
    color: "#1f2328",
    highlight: null as string | null,
    align: "left",
    lineSpacing: 1.15,
    spaceBeforePt: 0,
    spaceAfterPt: 8,
    indentLeftPt: 0,
    indentRightPt: 0,
    firstLinePt: 0,
  };
  for (const style of chain) {
    result = {
      font: style.font ?? result.font,
      sizePt: style.sizePt ?? result.sizePt,
      bold: style.bold ?? result.bold,
      italic: style.italic ?? result.italic,
      underline: style.underline ?? result.underline,
      strike: style.strike ?? result.strike,
      color: style.color ?? result.color,
      highlight: style.highlight ?? result.highlight,
      align: style.align ?? result.align,
      lineSpacing: style.lineSpacing ?? result.lineSpacing,
      spaceBeforePt: style.spaceBeforePt ?? result.spaceBeforePt,
      spaceAfterPt: style.spaceAfterPt ?? result.spaceAfterPt,
      indentLeftPt: style.indentLeftPt ?? result.indentLeftPt,
      indentRightPt: style.indentRightPt ?? result.indentRightPt,
      firstLinePt: style.firstLinePt ?? result.firstLinePt,
    };
  }
  if (props.align) result.align = props.align;
  if (props.lineSpacing) result.lineSpacing = props.lineSpacing;
  if (props.spaceBeforePt) result.spaceBeforePt = props.spaceBeforePt;
  if (props.spaceAfterPt) result.spaceAfterPt = props.spaceAfterPt;
  result.indentLeftPt = Math.max(result.indentLeftPt, props.indentLeftPt);
  result.indentRightPt = Math.max(result.indentRightPt, props.indentRightPt);
  if (props.firstLinePt) result.firstLinePt = props.firstLinePt;
  if (props.list) result.indentLeftPt = Math.max(result.indentLeftPt, 18 * (props.list.level + 1));
  return result;
}
