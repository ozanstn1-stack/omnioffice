/**
 * Impress values shared by the editor controller and its extracted views.
 *
 * Moved verbatim out of ImpressEditor.tsx (B4 editor seam 3): the dialog and
 * slide-view modules need these runtime values, and importing them back from
 * the editor would make the modules import each other. ImpressEditor.tsx
 * re-exports the public names from here, so its module API is unchanged.
 */

/** The slide theme the editor picks from `THEMES` and the views render with. */
export type Theme = {
  id: string;
  name: string;
  background: string;
  title: string;
  body: string;
  accent: string;
  titleColor: string;
  bodyColor: string;
};

export const MAX_GROUP_DEPTH = 8;
export const INHERITED_KEYS: Set<string> = new Set();

export const ANIMATION_EFFECTS: Record<string, string[]> = {
  entrance: ["appear", "fade", "flyIn", "zoom"],
  emphasis: ["pulse", "spin", "grow", "shrink"],
  exit: ["disappear", "fadeOut", "flyOut"],
};

export {
  CHART_PALETTE,
  chartDataRowCount,
  chartHasCachedData,
  chartWorkbookRange,
  formatChartCellValue,
  mergeChartPaste,
  normalizeChartDraft,
  parseChartCellNumber,
  parseChartClipboard,
  syncChartDataRanges,
  type ChartGridCell,
} from "../charts/chart-helpers";
