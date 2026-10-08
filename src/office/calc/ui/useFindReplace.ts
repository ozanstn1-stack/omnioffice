/**
 * State and actions of the Calc find & replace panel.
 *
 * The matches are derived from the workbook, the query and the options, so
 * they follow every edit. A regular expression is first run over the cell
 * texts in a worker (writer/regex-probe.ts): a catastrophic pattern is stopped
 * there instead of freezing the grid with unsaved work in it.
 */
import { useCallback, useDeferredValue, useEffect, useMemo, useState } from "react";
import { useT } from "../../../lib/i18n";
import type { Workbook } from "../../../lib/office-types";
import { useToasts } from "../../../lib/store";
import { canProbeRegex, probeRegex, type ProbeStatus, type RegexProbe } from "../../writer/regex-probe";
import { workbookValues } from "../cells";
import {
  compileFind,
  DEFAULT_FIND_OPTIONS,
  matchCells,
  replaceCells,
  searchCells,
  stepMatch,
  type CalcFindOptions,
  type SearchCell,
} from "../find-replace";
import type { Scalar } from "../formula";
import type { CellPosition } from "../grid-types";

const NO_VALUES: ReadonlyMap<string, Scalar> = new Map();

export type FindMode = "find" | "replace";

export interface FindReplaceHost {
  workbook: Workbook;
  sheetIndex: number;
  /** The active cell. */
  focus: CellPosition;
  /** Selects a cell of a sheet (switching sheets when needed) and scrolls it into view. */
  jumpTo: (sheet: number, row: number, col: number) => void;
  /** Makes a replacement result the new workbook, as one undo step. */
  commit: (next: Workbook) => void;
  /** Called when the panel closes, to hand the keyboard back to the grid. */
  onClosed: () => void;
}

export interface FindReplacePanelModel {
  mode: FindMode;
  query: string;
  replacement: string;
  options: CalcFindOptions;
  /** A problem with the query (invalid or too slow regular expression), or null. */
  error: string | null;
  /** The match count / position line. */
  status: string;
  /** Why some found cells will not be replaced, or null. */
  notice: string | null;
  /** Bumped when the panel is asked for again, so the search field takes focus. */
  focusRequest: number;
  canStep: boolean;
  canReplace: boolean;
  onQuery: (query: string) => void;
  onReplacement: (replacement: string) => void;
  onOptions: (patch: Partial<CalcFindOptions>) => void;
  onMode: (mode: FindMode) => void;
  onStep: (direction: 1 | -1) => void;
  onReplace: () => void;
  onReplaceAll: () => void;
  onClose: () => void;
}

export function useFindReplace(host: FindReplaceHost): {
  show: (mode: FindMode) => void;
  panel: FindReplacePanelModel | null;
} {
  const t = useT();
  const { workbook, sheetIndex, focus, jumpTo, commit, onClosed } = host;
  const [mode, setMode] = useState<FindMode | null>(null);
  const [query, setQuery] = useState("");
  const [replacement, setReplacement] = useState("");
  const [options, setOptions] = useState<CalcFindOptions>(DEFAULT_FIND_OPTIONS);
  const [focusRequest, setFocusRequest] = useState(0);
  const deferredQuery = useDeferredValue(query);
  const open = mode !== null;

  const search = useMemo(() => (open ? compileFind(deferredQuery, options) : null), [open, deferredQuery, options]);
  const cells = useMemo<SearchCell[]>(
    () =>
      search?.ok
        ? searchCells(workbook, sheetIndex, options, options.lookIn === "values" ? workbookValues(workbook) : NO_VALUES)
        : [],
    [search, workbook, sheetIndex, options],
  );

  const probing = options.regex && search?.ok === true && canProbeRegex();
  const probeKey = useMemo(() => ({ search, cells }), [search, cells]);
  const [probe, setProbe] = useState<{ key: object; status: ProbeStatus } | null>(null);
  useEffect(() => {
    if (!probing || !search?.ok) return;
    let run: RegexProbe | null = null;
    const timer = setTimeout(() => {
      run = probeRegex(
        search.pattern,
        cells.map((cell) => cell.text),
      );
      void run.promise.then((status) => setProbe({ key: probeKey, status }));
    }, 250);
    return () => {
      clearTimeout(timer);
      run?.cancel();
    };
  }, [probing, probeKey, search, cells]);
  const probeStatus: ProbeStatus | "pending" = !probing ? "ok" : probe?.key === probeKey ? probe.status : "pending";

  const matches = useMemo(
    () => (search?.ok && probeStatus === "ok" ? matchCells(cells, search.pattern) : []),
    [search, cells, probeStatus],
  );
  const currentIndex = matches.findIndex(
    (match) => match.sheet === sheetIndex && match.row === focus.row && match.col === focus.col,
  );

  const show = useCallback((next: FindMode) => {
    setMode((current) => (next === "replace" || current === null ? next : current));
    setFocusRequest((value) => value + 1);
  }, []);

  const close = () => {
    setMode(null);
    onClosed();
  };

  const step = (direction: 1 | -1) => {
    const next = stepMatch(matches, { sheet: sheetIndex, row: focus.row, col: focus.col }, direction);
    if (next) jumpTo(next.sheet, next.row, next.col);
  };

  const replaceCurrent = () => {
    if (!search?.ok) return;
    const current = currentIndex >= 0 ? matches[currentIndex] : null;
    if (!current) {
      step(1);
      return;
    }
    const result = replaceCells(workbook, [current], search.pattern, replacement, options);
    if (result.replaced > 0) commit(result.workbook);
    // Continue from the old list, past the cell just handled, so a replacement
    // that still matches does not keep the search on the same cell.
    const next = stepMatch(matches, current, 1);
    if (next && next !== current) jumpTo(next.sheet, next.row, next.col);
  };

  const replaceAll = () => {
    if (!search?.ok) return;
    const result = replaceCells(workbook, matches, search.pattern, replacement, options);
    const push = useToasts.getState().push;
    if (result.replaced === 0) {
      push({ kind: "info", title: t("calc.replaceNothing") });
      return;
    }
    commit(result.workbook);
    push({
      kind: "success",
      title:
        result.skipped > 0
          ? t("calc.replaceDoneSkipped", { count: result.replaced, skipped: result.skipped })
          : t("calc.replaceDone", { count: result.replaced }),
    });
  };

  if (mode === null) return { show, panel: null };

  const error =
    search && !search.ok
      ? `${t("writer.findInvalidRegex")}: ${search.error}`
      : probeStatus === "slow"
        ? t("writer.regexTooSlow")
        : null;
  const status = (() => {
    if (!search?.ok || error) return "";
    if (probeStatus === "pending") return t("writer.findSearching");
    if (matches.length === 0) return t("calc.findNoCells");
    if (currentIndex >= 0) return t("calc.findCellOf", { current: currentIndex + 1, count: matches.length });
    return matches.length === 1 ? t("calc.findOneCell") : t("calc.findCells", { count: matches.length });
  })();
  const blocked = matches.filter((match) => !match.replaceable).length;

  return {
    show,
    panel: {
      mode,
      query,
      replacement,
      options,
      error,
      status,
      notice: mode === "replace" && blocked > 0 ? t("calc.findNotReplaceable", { count: blocked }) : null,
      focusRequest,
      canStep: matches.length > 0,
      canReplace: matches.length > 0,
      onQuery: setQuery,
      onReplacement: setReplacement,
      onOptions: (patch) => setOptions((current) => ({ ...current, ...patch })),
      onMode: setMode,
      onStep: step,
      onReplace: replaceCurrent,
      onReplaceAll: replaceAll,
      onClose: close,
    },
  };
}
