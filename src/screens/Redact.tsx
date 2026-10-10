import { useCallback, useMemo, useState } from "react";
import { ChevronLeft, ChevronRight, Eye, Eraser, Search, ShieldCheck, Trash2 } from "lucide-react";
import {
  Badge,
  Card,
  Checkbox,
  Field,
  Segmented,
  Slider,
  Spinner,
  TextArea,
  TextInput,
  Toggle,
} from "../components/ui";
import { DropZone, FileList, InfoStrip, OutputBar, ResultCard } from "../components/files";
import { PageCanvas, type Rect } from "../components/pages";
import { OptionCard, Screen, TwoColumn } from "../components/layout";
import { useT } from "../lib/i18n";
import { uid } from "../lib/format";
import { useJobProgress } from "../lib/store";
import { useTool } from "../lib/useTool";
import { cancelJob, detectRedactionMatches, detectSensitiveText, redactPdf, toAppError } from "../lib/api";
import { normalizePageSize, rectToUserSpace, userSpaceToRect } from "../lib/redact-geometry";
import type { ImageRedactionMode, RedactionArea, RedactionMatch, RedactionOptions } from "../lib/types";

/** A box the user drew, in page space, with a stable id for removal. */
interface DrawnBox extends RedactionArea {
  id: string;
  /** Set for boxes the detector proposed, so they can be labelled. */
  kind?: string;
}

/** A detected match plus the id that links its checkbox to its box. */
interface MatchRow {
  id: string;
  match: RedactionMatch;
}

/** Comma/semicolon/newline separated keywords: trimmed, order kept, deduped. */
export function splitKeywords(raw: string): string[] {
  const seen = new Set<string>();
  const keywords: string[] = [];
  for (const part of raw.split(/[,;\n]/)) {
    const keyword = part.trim();
    if (!keyword || seen.has(keyword)) continue;
    seen.add(keyword);
    keywords.push(keyword);
  }
  return keywords;
}

/** The row id is page-scoped so a match keeps its identity off the shown page. */
function matchRow(source: "auto" | "pattern", match: RedactionMatch, index: number): MatchRow {
  return { id: `${source}-${match.page}-${index}`, match };
}

function matchBox(row: MatchRow): DrawnBox {
  return {
    id: row.id,
    page: row.match.page,
    left: row.match.left,
    bottom: row.match.bottom,
    right: row.match.right,
    top: row.match.top,
    kind: row.match.kind,
  };
}

function kindLabel(t: (key: string) => string, kind?: string): string {
  if (!kind) return t("redact.manual");
  const key = `redact.kind.${kind}`;
  const label = t(key);
  return label === key ? kind : label;
}

export function Redact({ initialFiles, dragging }: { initialFiles?: string[]; dragging: boolean }) {
  const t = useT();
  const session = useTool({ suffix: "_redacted", accept: "pdf", initialPaths: initialFiles });
  const [options, setOptions] = useState<RedactionOptions>({
    fill: "#000000",
    images: "obscure",
    paddingPt: 1,
    removeMetadata: true,
  });
  const [page, setPage] = useState(1);
  const [boxes, setBoxes] = useState<DrawnBox[]>([]);
  const [matches, setMatches] = useState<MatchRow[]>([]);
  const [detecting, setDetecting] = useState(false);
  const [detected, setDetected] = useState(false);
  const [autoAdd, setAutoAdd] = useState(true);
  const [scanError, setScanError] = useState<string | null>(null);
  // Keyword/regex scan state: its own controls, its own job (progress +
  // cancel), and the outcome so an empty result can be reported.
  const [keywordsText, setKeywordsText] = useState("");
  const [pattern, setPattern] = useState("");
  const [caseSensitive, setCaseSensitive] = useState(false);
  const [scanningPatterns, setScanningPatterns] = useState(false);
  const [patternCount, setPatternCount] = useState<number | null>(null);
  const [scanJobId, setScanJobId] = useState("");
  const scanProgress = useJobProgress(scanJobId);
  const keywords = useMemo(() => splitKeywords(keywordsText), [keywordsText]);
  const canFindPatterns = keywords.length > 0 || pattern.trim().length > 0;

  const patch = (values: Partial<RedactionOptions>) => setOptions((previous) => ({ ...previous, ...values }));
  const pageCount = session.info?.pageCount ?? 0;
  // render::PageGeometry is serialized without rename_all, so the widths are
  // width_pt/height_pt. normalizePageSize is the single place that knows it.
  const geometry = useMemo(() => {
    const found = session.info?.pageGeometries.find((entry) => entry.page === page);
    return found ? normalizePageSize(found) : null;
  }, [session.info, page]);

  // Boxes belong to the page they were drawn on; a new document starts clean.
  // The reset is derived from the document identity during render instead of an
  // effect so no stale boxes paint for the new file.
  const docIdentity = session.primary?.path ?? "";
  const [lastDoc, setLastDoc] = useState(docIdentity);
  if (lastDoc !== docIdentity) {
    setLastDoc(docIdentity);
    setBoxes([]);
    setMatches([]);
    setDetected(false);
    setScanError(null);
    setPatternCount(null);
  }

  // Keep the current page inside the (possibly shrunken) document instead of
  // clamping in an effect.
  const shownPage = pageCount && page > pageCount ? pageCount : page;

  /**
   * The canvas reports a rectangle normalized to the rendered image with the
   * origin top-left, but PDF user space has its origin bottom-left, so the
   * vertical axis is flipped here. The conversion lives in redact-geometry so
   * it can be tested; getting it wrong silently redacts the wrong text.
   */
  const addBox = useCallback(
    (rect: Rect) => {
      if (!geometry) return;
      const area = rectToUserSpace(rect, geometry);
      if (!area) return;
      setBoxes((previous) => [...previous, { ...area, page: shownPage, id: `box-${shownPage}-${previous.length}` }]);
    },
    [geometry, shownPage],
  );

  const pageBoxes = useMemo(() => boxes.filter((box) => box.page === shownPage), [boxes, shownPage]);

  const overlayBoxes = geometry ? (
    <div className="absolute inset-0">
      {pageBoxes.map((box) => {
        const rect = userSpaceToRect(box, geometry);
        return (
          <button
            key={box.id}
            type="button"
            title={box.kind ? kindLabel(t, box.kind) : t("redact.manual")}
            aria-label={box.kind ? kindLabel(t, box.kind) : t("redact.manual")}
            onClick={() => setBoxes((previous) => previous.filter((item) => item.id !== box.id))}
            style={{
              position: "absolute",
              left: `${rect.x * 100}%`,
              top: `${rect.y * 100}%`,
              width: `${rect.w * 100}%`,
              height: `${rect.h * 100}%`,
              background: "color-mix(in srgb, var(--danger) 26%, transparent)",
              border: "2px solid var(--danger)",
              cursor: "pointer",
              padding: 0,
            }}
          />
        );
      })}
    </div>
  ) : null;

  const scan = async () => {
    if (!session.primary) return;
    setDetecting(true);
    setScanError(null);
    try {
      const found = await detectSensitiveText(session.primary.path, page, session.password || undefined);
      const rows = found.map((match, index) => matchRow("auto", match, index));
      // The detector replaces only its own rows/boxes; keyword/pattern results
      // and the boxes the user drew stay.
      setMatches((previous) => [...previous.filter((row) => !row.id.startsWith("auto-")), ...rows]);
      if (autoAdd) {
        setBoxes((previous) => [...previous.filter((box) => !box.id.startsWith("auto-")), ...rows.map(matchBox)]);
      }
      setDetected(true);
    } catch (error) {
      const appError = toAppError(error);
      setScanError(appError.message);
      setMatches((previous) => previous.filter((row) => !row.id.startsWith("auto-")));
    } finally {
      setDetecting(false);
    }
  };

  const findPatterns = async () => {
    if (!session.primary || !canFindPatterns) return;
    const jobId = uid("redact-scan");
    setScanJobId(jobId);
    setScanningPatterns(true);
    setScanError(null);
    setPatternCount(null);
    try {
      const found = await detectRedactionMatches(
        session.primary.path,
        keywords,
        pattern.trim() ? pattern.trim() : null,
        caseSensitive,
        null,
        jobId,
        session.password || undefined,
      );
      const rows = found.map((match, index) => matchRow("pattern", match, index));
      setMatches((previous) => [...previous.filter((row) => !row.id.startsWith("pattern-")), ...rows]);
      if (autoAdd) {
        setBoxes((previous) => [...previous.filter((box) => !box.id.startsWith("pattern-")), ...rows.map(matchBox)]);
      }
      setPatternCount(found.length);
    } catch (error) {
      const appError = toAppError(error);
      if (appError.code !== "cancelled") setScanError(appError.message);
    } finally {
      setScanningPatterns(false);
    }
  };

  const toggleMatch = (row: MatchRow) => {
    setBoxes((previous) => {
      const existing = previous.find((box) => box.id === row.id);
      if (existing) return previous.filter((box) => box.id !== row.id);
      return [...previous, matchBox(row)];
    });
  };

  const counts = useMemo(() => {
    const perPage = new Map<number, number>();
    for (const box of boxes) perPage.set(box.page, (perPage.get(box.page) ?? 0) + 1);
    return { total: boxes.length, perPage };
  }, [boxes]);

  session.registerAutoRun(() => void run());
  const run = () => {
    if (!boxes.length) return;
    const areas: RedactionArea[] = boxes.map(({ page: boxPage, left, bottom, right, top }) => ({
      page: boxPage,
      left,
      bottom,
      right,
      top,
    }));
    return session.run((jobId, overwrite) =>
      redactPdf(
        session.primary?.path ?? "",
        session.outputSpec(overwrite),
        areas,
        options,
        jobId,
        session.password || undefined,
      ),
    );
  };

  return (
    <Screen title={t("nav.redact")} subtitle={t("redact.subtitle")} actions={<Eraser size={18} className="muted" />}>
      <TwoColumn
        main={
          !session.primary ? (
            <DropZone onPaths={(paths) => void session.addPaths(paths)} dragging={dragging} accept="pdf" />
          ) : (
            <>
              <OptionCard>
                <FileList
                  files={session.files}
                  onRemove={session.removeFile}
                  onAdd={session.pickFiles}
                  addLabel={t("common.addPdf")}
                />
              </OptionCard>
              {session.info ? (
                <Card className="p-4">
                  <InfoStrip info={session.info} error={session.infoError} />
                </Card>
              ) : null}
              {pageCount > 1 ? (
                <div className="flex items-center justify-center gap-2">
                  <button
                    className="btn btn-sm"
                    type="button"
                    disabled={shownPage <= 1}
                    onClick={() => setPage((value) => Math.max(1, value - 1))}
                  >
                    <ChevronLeft size={14} />
                  </button>
                  <span className="text-xs muted">
                    {t("common.page")} {shownPage} / {pageCount}
                    {counts.perPage.get(shownPage) ? ` · ${counts.perPage.get(shownPage)} ${t("redact.box")}` : ""}
                  </span>
                  <button
                    className="btn btn-sm"
                    type="button"
                    disabled={shownPage >= pageCount}
                    onClick={() => setPage((value) => Math.min(pageCount, value + 1))}
                  >
                    <ChevronRight size={14} />
                  </button>
                </div>
              ) : null}
              <Card className="p-4">
                <p className="text-xs muted mb-3">
                  {t("redact.drawHint")}{" "}
                  {geometry ? (
                    <span className="muted">
                      {Math.round(geometry.width)} × {Math.round(geometry.height)} pt
                    </span>
                  ) : null}
                </p>
                <div className="mx-auto" style={{ maxWidth: 620 }}>
                  <PageCanvas
                    path={session.primary.path}
                    page={shownPage}
                    password={session.password || undefined}
                    maxWidth={820}
                    onDragRect={addBox}
                    overlay={overlayBoxes}
                  />
                </div>
              </Card>
              {detecting || scanningPatterns || matches.length || detected || patternCount !== null || scanError ? (
                <OptionCard
                  title={t("redact.detected")}
                  action={
                    <button className="btn btn-sm" type="button" onClick={() => void scan()} disabled={detecting}>
                      {detecting ? <Spinner size={13} /> : <Eye size={13} />} {t("redact.scan")}
                    </button>
                  }
                >
                  {scanError ? (
                    <p className="text-xs" style={{ color: "var(--danger)" }}>
                      {scanError}
                    </p>
                  ) : null}
                  {patternCount === 0 ? <p className="text-xs muted">{t("redact.patternNone")}</p> : null}
                  {matches.length === 0 && patternCount !== 0 ? (
                    <p className="text-xs muted">{detected ? t("redact.noneFound") : t("redact.scanHint")}</p>
                  ) : null}
                  {matches.length ? (
                    <div className="flex flex-col gap-1.5 max-h-64 overflow-auto">
                      {matches.map((row) => {
                        const included = boxes.some((box) => box.id === row.id);
                        return (
                          <label key={row.id} className="flex items-center gap-2 text-xs" style={{ cursor: "pointer" }}>
                            <input type="checkbox" checked={included} onChange={() => toggleMatch(row)} />
                            <Badge>{kindLabel(t, row.match.kind)}</Badge>
                            <span className="truncate" style={{ color: "var(--text-1)" }}>
                              {row.match.text}
                            </span>
                          </label>
                        );
                      })}
                    </div>
                  ) : null}
                </OptionCard>
              ) : null}
            </>
          )
        }
        side={
          <>
            <OutputBar
              session={session}
              runLabel={t("nav.redact")}
              onRun={() => void run()}
              disabled={!session.primary || !boxes.length}
            />
            <OptionCard title={t("nav.redact")}>
              <div className="flex items-center gap-2 text-xs muted">
                <ShieldCheck size={14} />
                {t("redact.removesText")}
              </div>
              <button
                className="btn btn-sm self-start"
                type="button"
                onClick={() => void scan()}
                disabled={!session.primary || detecting}
              >
                {detecting ? <Spinner size={13} /> : <Eye size={13} />} {t("redact.scan")}
              </button>
              <Toggle checked={autoAdd} onChange={setAutoAdd} label={t("redact.autoAdd")} />
              <Field label={t("redact.fill")}>
                <div className="flex items-center gap-2">
                  <input
                    type="color"
                    value={options.fill}
                    onChange={(event) => patch({ fill: event.target.value })}
                    style={{
                      width: 34,
                      height: 28,
                      border: "1px solid var(--border)",
                      borderRadius: 6,
                      background: "none",
                    }}
                  />
                  <span className="text-xs muted">{options.fill}</span>
                </div>
              </Field>
              <Field label={t("redact.imageMode")}>
                <Segmented<ImageRedactionMode>
                  value={options.images}
                  onChange={(value) => patch({ images: value })}
                  options={[
                    { value: "obscure", label: t("redact.imageObscure") },
                    { value: "removePixels", label: t("redact.imageRemove") },
                  ]}
                />
              </Field>
              <p className="text-xs muted">
                {options.images === "obscure" ? t("redact.imageObscureHint") : t("redact.imageRemoveHint")}
              </p>
              <Field label={t("redact.padding")}>
                <Slider
                  value={options.paddingPt}
                  min={0}
                  max={6}
                  step={0.5}
                  onChange={(value) => patch({ paddingPt: value })}
                />
              </Field>
              <Toggle
                checked={options.removeMetadata}
                onChange={(value) => patch({ removeMetadata: value })}
                label={t("redact.stripMetadata")}
              />
              {boxes.length ? (
                <div className="flex items-center justify-between text-xs">
                  <span className="muted">
                    {counts.total} {t("redact.box")}
                  </span>
                  <button className="btn btn-sm" type="button" onClick={() => setBoxes([])}>
                    <Trash2 size={13} /> {t("common.clearAll")}
                  </button>
                </div>
              ) : null}
            </OptionCard>
            <OptionCard title={t("redact.findPatterns")}>
              <Field label={t("redact.keywords")} hint={t("redact.keywordsHint")}>
                <TextArea
                  aria-label={t("redact.keywords")}
                  value={keywordsText}
                  rows={3}
                  spellCheck={false}
                  onChange={(event) => setKeywordsText(event.target.value)}
                />
              </Field>
              <Field label={t("redact.pattern")} hint={t("redact.patternHint")}>
                <TextInput
                  aria-label={t("redact.pattern")}
                  value={pattern}
                  spellCheck={false}
                  onChange={(event) => setPattern(event.target.value)}
                />
              </Field>
              <Checkbox checked={caseSensitive} onChange={setCaseSensitive} label={t("redact.caseSensitive")} />
              <div className="flex items-center gap-2">
                <button
                  className="btn btn-sm self-start"
                  type="button"
                  onClick={() => void findPatterns()}
                  disabled={!session.primary || !canFindPatterns || scanningPatterns}
                >
                  {scanningPatterns ? <Spinner size={13} /> : <Search size={13} />} {t("redact.findPatterns")}
                </button>
                {scanningPatterns && scanJobId ? (
                  <button className="btn btn-sm" type="button" onClick={() => void cancelJob(scanJobId)}>
                    {t("progress.cancel")}
                  </button>
                ) : null}
              </div>
              {scanningPatterns ? (
                <p className="text-xs muted">
                  {t("redact.scanningAll")}
                  {scanProgress && scanProgress.total > 0 ? ` · ${scanProgress.current} / ${scanProgress.total}` : ""}
                </p>
              ) : null}
            </OptionCard>
            {session.result ? <ResultCard result={session.result} onReset={session.resetResult} /> : null}
          </>
        }
      />
    </Screen>
  );
}
