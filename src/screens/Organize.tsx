import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  ArrowDown,
  ArrowUp,
  Copy,
  FilePlus2,
  Layers,
  Plus,
  Redo2,
  RotateCcw,
  RotateCw,
  Save,
  Scissors,
  Trash2,
  Undo2,
} from "lucide-react";
import { Badge, Button, Card, IconButton, Modal, Spinner, TextInput } from "../components/ui";
import { DropZone, InfoStrip, OutputBar, ResultCard } from "../components/files";
import { OptionCard, Screen, TwoColumn } from "../components/layout";
import type { PageItem } from "../components/pages";
import { useT } from "../lib/i18n";
import { useTool } from "../lib/useTool";
import { applyPagePlan, pageThumbnail, pdfSetOutline } from "../lib/api";
import type { OutlineEntry, PagePlanItem } from "../lib/types";

/** A page slot in the organizer; `blank` slots insert a new empty page. */
type OrganizeItem = PageItem & { blank?: boolean };

/** An outline row with a stable identity for React keys across reorders. */
type OutlineRow = OutlineEntry & { id: string };

function planItem(page: OrganizeItem): PagePlanItem {
  if (page.blank) {
    return { source_page: 0, rotation_delta: 0, blank: true, width_pt: null, height_pt: null };
  }
  return { source_page: page.sourcePage, rotation_delta: page.rotationDelta };
}

// The shared `Thumb` component cannot skip its thumbnail request, and a blank
// page has nothing to render: the local copy below adds exactly that branch.
function PageThumb({
  path,
  page,
  size,
  password,
  refreshedAt,
}: {
  path: string;
  page: number;
  size: number;
  password?: string;
  refreshedAt: number;
}) {
  const [src, setSrc] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);
  const requested = useRef(false);
  // A refresh (or a new path/page) is a different thumbnail: the reset is
  // derived from the identity below instead of a setState effect.
  const identity = `${path}:${page}:${refreshedAt}`;
  const [lastIdentity, setLastIdentity] = useState(identity);
  if (lastIdentity !== identity) {
    setLastIdentity(identity);
    setSrc(null);
    setFailed(false);
  }

  useEffect(() => {
    const element = containerRef.current;
    if (!element || requested.current) return;
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((entry) => entry.isIntersecting) && !requested.current) {
          requested.current = true;
          void pageThumbnail(path, page, size, password)
            .then((thumb) => setSrc(thumb.dataUrl))
            .catch(() => setFailed(true));
        }
      },
      { rootMargin: "320px" },
    );
    observer.observe(element);
    return () => {
      observer.disconnect();
      requested.current = false;
    };
  }, [path, page, size, password, identity]);

  return (
    <div
      ref={containerRef}
      className="w-full h-full flex items-center justify-center overflow-hidden"
      style={{ background: "var(--surface-2)" }}
    >
      {src ? (
        <img src={src} alt={`Page ${page}`} className="w-full h-full object-contain" draggable={false} />
      ) : failed ? (
        <span className="text-xs muted">—</span>
      ) : (
        <Spinner size={18} />
      )}
    </div>
  );
}

function OrganizeThumb({
  path,
  item,
  size,
  password,
  refreshedAt,
}: {
  path: string;
  item: OrganizeItem;
  size: number;
  password?: string;
  refreshedAt: number;
}) {
  const t = useT();
  if (item.blank) {
    return (
      <div className="organize-blank">
        <FilePlus2 size={20} />
        <span className="text-[11px] font-medium">{t("organize.blankPage")}</span>
      </div>
    );
  }
  return <PageThumb path={path} page={item.sourcePage} size={size} password={password} refreshedAt={refreshedAt} />;
}

function OrganizeGrid({
  path,
  pages,
  selected,
  onSelectionChange,
  onReorder,
  password,
  thumbWidth,
  refreshedAt,
}: {
  path: string;
  pages: OrganizeItem[];
  selected: Set<number>;
  onSelectionChange: (next: Set<number>) => void;
  onReorder?: (from: number, to: number) => void;
  password?: string;
  thumbWidth: number;
  refreshedAt: number;
}) {
  const t = useT();
  // State (not a ref) so the render can honestly reflect the drag highlight.
  const [dragFromIndex, setDragFromIndex] = useState<number | null>(null);
  const dragFrom = useRef<number | null>(null);
  const [overIndex, setOverIndex] = useState<number | null>(null);
  const [shiftAnchor, setShiftAnchor] = useState<number | null>(null);

  const handleClick = useCallback(
    (index: number, event: React.MouseEvent) => {
      const next = new Set(selected);
      if (event.shiftKey && shiftAnchor !== null) {
        const [a, b] = [Math.min(shiftAnchor, index), Math.max(shiftAnchor, index)];
        for (let i = a; i <= b; i += 1) next.add(i);
      } else if (event.ctrlKey || event.metaKey) {
        if (next.has(index)) next.delete(index);
        else next.add(index);
        setShiftAnchor(index);
      } else {
        if (next.size === 1 && next.has(index)) next.clear();
        else {
          next.clear();
          next.add(index);
        }
        setShiftAnchor(index);
      }
      onSelectionChange(next);
    },
    [onSelectionChange, selected, shiftAnchor],
  );

  const handlePointerDown = (index: number) => (event: React.PointerEvent) => {
    if (!onReorder) return;
    if (event.button !== 0) return;
    if ((event.target as HTMLElement).closest("button")) return;
    dragFrom.current = index;
    setDragFromIndex(index);
    setOverIndex(index);
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
  };

  const handlePointerMove = (event: React.PointerEvent) => {
    if (dragFrom.current === null) return;
    const elements = document.elementsFromPoint(event.clientX, event.clientY);
    const target = elements.find(
      (element) => element instanceof HTMLElement && element.dataset.pageIndex !== undefined,
    ) as HTMLElement | undefined;
    if (target) setOverIndex(Number(target.dataset.pageIndex));
  };

  const handlePointerUp = () => {
    if (dragFrom.current !== null && overIndex !== null && dragFrom.current !== overIndex) {
      onReorder?.(dragFrom.current, overIndex);
    }
    dragFrom.current = null;
    setDragFromIndex(null);
    setOverIndex(null);
  };

  const columns = useMemo(() => `repeat(auto-fill, minmax(${thumbWidth}px, 1fr))`, [thumbWidth]);

  return (
    <div
      className="grid gap-3"
      style={{ gridTemplateColumns: columns, touchAction: "none" }}
      onPointerMove={handlePointerMove}
      onPointerUp={handlePointerUp}
      onPointerCancel={handlePointerUp}
    >
      {pages.map((page, index) => (
        <div key={page.id} className="flex flex-col gap-1.5">
          <div
            data-page-index={index}
            className="thumb"
            data-selected={selected.has(index)}
            data-dragging={dragFromIndex === index}
            data-droptarget={overIndex === index && dragFromIndex !== null && dragFromIndex !== index}
            style={{ aspectRatio: "1 / 1.3" }}
            onPointerDown={handlePointerDown(index)}
            onClick={(event) => handleClick(index, event)}
            role="button"
            tabIndex={0}
            aria-label={`${t("common.page")} ${index + 1}`}
            onKeyDown={(event) => {
              if (event.key === " " || event.key === "Enter") {
                event.preventDefault();
                handleClick(index, event as unknown as React.MouseEvent);
              }
            }}
            title={page.rotationDelta ? `${t("common.rotation")}: ${page.rotationDelta}°` : undefined}
          >
            <OrganizeThumb
              path={path}
              item={page}
              size={thumbWidth * 2}
              password={password}
              refreshedAt={refreshedAt}
            />
            <div className="absolute top-1.5 left-1.5 flex items-center gap-1">
              <span
                className="text-[11px] font-semibold px-1.5 py-0.5 rounded-md"
                style={{ background: "rgb(0 0 0 / 0.55)", color: "white" }}
              >
                {index + 1}
              </span>
              {!page.blank && page.sourcePage !== index + 1 ? (
                <span
                  className="text-[10px] px-1.5 py-0.5 rounded-md"
                  style={{ background: "rgb(0 0 0 / 0.4)", color: "white" }}
                  title={`Source page ${page.sourcePage}`}
                >
                  ←{page.sourcePage}
                </span>
              ) : null}
              {page.rotationDelta ? (
                <span
                  className="text-[10px] px-1.5 py-0.5 rounded-md flex items-center gap-0.5"
                  style={{ background: "rgb(0 0 0 / 0.45)", color: "white" }}
                >
                  <RotateCw size={9} />
                  {page.rotationDelta}°
                </span>
              ) : null}
            </div>
          </div>
        </div>
      ))}
    </div>
  );
}

export function Organize({ initialFiles, dragging }: { initialFiles?: string[]; dragging: boolean }) {
  const t = useT();
  const session = useTool({ suffix: "_organized", accept: "pdf", initialPaths: initialFiles });
  const [pages, setPages] = useState<OrganizeItem[]>([]);
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [history, setHistory] = useState<OrganizeItem[][]>([]);
  const [future, setFuture] = useState<OrganizeItem[][]>([]);
  const [thumbWidth, setThumbWidth] = useState(180);
  const [refreshedAt, setRefreshedAt] = useState(0);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [outline, setOutline] = useState<OutlineRow[]>([]);
  const [outlineSaved, setOutlineSaved] = useState(false);
  const copyRef = useRef(1);
  const outlineRef = useRef(1);

  const pageCount = session.info?.pageCount ?? 0;

  // Build the initial page model whenever the document changes. This is a
  // render-phase reset derived from the document identity: no extra render pass
  // and no stale thumbnails between documents. Ids are deterministic from the
  // document key, so no mutable counter is read during render.
  const docKey = `${session.primary?.path ?? ""}:${pageCount}`;
  const [builtFor, setBuiltFor] = useState<string | null>(null);
  if (builtFor !== docKey) {
    setBuiltFor(docKey);
    if (!session.primary || !pageCount) {
      setPages([]);
    } else {
      setPages(
        Array.from({ length: pageCount }, (_, index) => ({
          id: `${docKey}#${index + 1}`,
          sourcePage: index + 1,
          rotationDelta: 0,
        })),
      );
      setRefreshedAt((value) => value + 1);
    }
    setSelected(new Set());
    setHistory([]);
    setFuture([]);
  }

  // Seed the bookmarks editor from the loaded document outline. Keyed on the
  // same document identity as the page model, but only once pdf_info arrived.
  const outlineKey = session.primary && session.info ? docKey : null;
  const [outlineBuiltFor, setOutlineBuiltFor] = useState<string | null>(null);
  if (outlineKey !== null && outlineBuiltFor !== outlineKey) {
    setOutlineBuiltFor(outlineKey);
    setOutline((session.info?.outline ?? []).map((entry, index) => ({ ...entry, id: `${outlineKey}#entry${index}` })));
    setOutlineSaved(false);
  }

  const changed = useMemo(() => {
    return (
      pages.some((page, index) => page.blank || page.sourcePage !== index + 1 || page.rotationDelta !== 0) ||
      pages.length !== pageCount
    );
  }, [pageCount, pages]);

  const commit = useCallback(
    (next: OrganizeItem[]) => {
      setHistory((previous) => [...previous.slice(-99), pages]);
      setFuture([]);
      setPages(next);
      setSelected(new Set());
    },
    [pages],
  );

  const undo = useCallback(() => {
    setHistory((previous) => {
      if (!previous.length) return previous;
      const last = previous[previous.length - 1];
      setFuture((f) => [pages, ...f].slice(0, 100));
      setPages(last);
      setSelected(new Set());
      return previous.slice(0, -1);
    });
  }, [pages]);

  const redo = useCallback(() => {
    setFuture((previous) => {
      if (!previous.length) return previous;
      const [first, ...rest] = previous;
      setHistory((h) => [...h.slice(-99), pages]);
      setPages(first);
      setSelected(new Set());
      return rest;
    });
  }, [pages]);

  const rotateSelection = (delta: number) => {
    if (!selected.size) return;
    const next = pages.map((page, index) =>
      selected.has(index) ? { ...page, rotationDelta: (((page.rotationDelta + delta) % 360) + 360) % 360 } : page,
    );
    commit(next);
  };

  const duplicateSelection = () => {
    if (!selected.size) return;
    const next: OrganizeItem[] = [];
    pages.forEach((page, index) => {
      next.push(page);
      if (selected.has(index)) {
        next.push({ ...page, id: `${page.id}-copy${copyRef.current++}` });
      }
    });
    commit(next);
  };

  const deleteSelection = () => {
    if (!selected.size) return;
    if (selected.size >= pages.length) return;
    const next = pages.filter((_, index) => !selected.has(index));
    commit(next);
    setConfirmDelete(false);
  };

  const addBlankPage = () => {
    commit([
      ...pages,
      {
        id: `${docKey}#blank${copyRef.current++}`,
        sourcePage: 0,
        rotationDelta: 0,
        blank: true,
      },
    ]);
  };

  const reorder = (from: number, to: number) => {
    if (from === to) return;
    const next = [...pages];
    const [item] = next.splice(from, 1);
    next.splice(to, 0, item);
    commit(next);
  };

  const plan = useMemo(() => pages.map(planItem), [pages]);

  session.registerAutoRun(() => void save());
  const save = () =>
    session.run(async (jobId, overwrite) => {
      return applyPagePlan(
        session.primary?.path ?? "",
        plan,
        session.outputSpec(overwrite),
        jobId,
        session.password || undefined,
      );
    });

  const extractSelection = () =>
    session.run(async (jobId, overwrite) => {
      const subset = pages.filter((_, index) => selected.has(index));
      if (!subset.length) throw { code: "invalid_input", message: t("errors.invalid_input") };
      const subsetPlan = subset.map(planItem);
      const target = session.outputPath.replace(/_organized(\.pdf)?$/i, "_extracted.pdf");
      return applyPagePlan(
        session.primary?.path ?? "",
        subsetPlan,
        session.outputSpec(overwrite, target),
        jobId,
        session.password || undefined,
      );
    });

  const patchOutline = (index: number, values: Partial<OutlineEntry>) => {
    setOutlineSaved(false);
    setOutline((previous) => previous.map((entry, i) => (i === index ? { ...entry, ...values } : entry)));
  };

  const removeOutline = (index: number) => {
    setOutlineSaved(false);
    setOutline((previous) => previous.filter((_, i) => i !== index));
  };

  const moveOutline = (index: number, delta: number) => {
    const target = index + delta;
    setOutlineSaved(false);
    setOutline((previous) => {
      if (target < 0 || target >= previous.length) return previous;
      const next = [...previous];
      const [item] = next.splice(index, 1);
      next.splice(target, 0, item);
      return next;
    });
  };

  const addOutline = () => {
    setOutlineSaved(false);
    setOutline((previous) => [
      ...previous,
      { id: `${docKey}#entry-new${outlineRef.current++}`, title: "", page: 1, depth: 0 },
    ]);
  };

  const saveOutline = () =>
    session.run(async (jobId, overwrite) => {
      const entries: OutlineEntry[] = outline.map((entry) => ({
        title: entry.title,
        page: Math.min(Math.max(1, Math.round(entry.page) || 1), Math.max(1, pageCount)),
        depth: Math.max(0, Math.round(entry.depth) || 0),
      }));
      const result = await pdfSetOutline(
        session.primary?.path ?? "",
        session.outputSpec(overwrite),
        entries,
        jobId,
        session.password || undefined,
      );
      setOutlineSaved(true);
      return result;
    });

  // Keyboard shortcuts for the organizer.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement;
      const typing = ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName);
      if (typing) return;
      if (event.ctrlKey && event.key.toLowerCase() === "z") {
        event.preventDefault();
        undo();
      } else if (
        event.ctrlKey &&
        (event.key.toLowerCase() === "y" || (event.shiftKey && event.key.toLowerCase() === "z"))
      ) {
        event.preventDefault();
        redo();
      } else if (event.ctrlKey && event.key.toLowerCase() === "a") {
        event.preventDefault();
        setSelected(new Set(pages.map((_, index) => index)));
      } else if (event.key === "Delete" && selected.size) {
        event.preventDefault();
        setConfirmDelete(true);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [pages, redo, selected.size, undo]);

  return (
    <Screen
      title={t("organize.title")}
      subtitle={t("organize.subtitle")}
      actions={
        <div className="flex items-center gap-1.5">
          <IconButton label={t("common.undo")} onClick={undo} disabled={!history.length}>
            <Undo2 size={16} />
          </IconButton>
          <IconButton label={t("common.redo")} onClick={redo} disabled={!future.length}>
            <Redo2 size={16} />
          </IconButton>
          <div className="w-px h-5 mx-1" style={{ background: "var(--border)" }} />
          <IconButton label={t("common.rotate")} onClick={() => rotateSelection(-90)} disabled={!selected.size}>
            <RotateCcw size={16} />
          </IconButton>
          <IconButton label={t("common.rotate")} onClick={() => rotateSelection(90)} disabled={!selected.size}>
            <RotateCw size={16} />
          </IconButton>
          <IconButton label={t("common.duplicate")} onClick={duplicateSelection} disabled={!selected.size}>
            <Copy size={16} />
          </IconButton>
          <IconButton label={t("common.extract")} onClick={() => void extractSelection()} disabled={!selected.size}>
            <Scissors size={16} />
          </IconButton>
          <IconButton label={t("common.delete")} onClick={() => setConfirmDelete(true)} disabled={!selected.size}>
            <Trash2 size={16} />
          </IconButton>
        </div>
      }
    >
      <TwoColumn
        main={
          !session.primary ? (
            <DropZone onPaths={(paths) => void session.addPaths(paths)} dragging={dragging} accept="pdf" />
          ) : (
            <>
              <Card className="p-4 flex flex-wrap items-center gap-3">
                <InfoStrip info={session.info} error={session.infoError} />
                <div className="ml-auto flex items-center gap-2">
                  <Badge tone={selected.size ? "accent" : "default"}>
                    {t("organize.selectedCount", { count: selected.size })}
                  </Badge>
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={() => setSelected(new Set(pages.map((_, index) => index)))}
                  >
                    {t("common.selectAll")}
                  </Button>
                  <Button size="sm" variant="ghost" onClick={() => setSelected(new Set())}>
                    {t("common.clear")}
                  </Button>
                  <select
                    className="select input-sm"
                    style={{ width: 110 }}
                    value={thumbWidth}
                    onChange={(event) => setThumbWidth(Number(event.target.value))}
                    aria-label="Thumbnail size"
                  >
                    <option value={130}>Small</option>
                    <option value={180}>Medium</option>
                    <option value={240}>Large</option>
                  </select>
                </div>
              </Card>

              {changed ? (
                <div className="text-xs flex items-center gap-2" style={{ color: "var(--warn)" }}>
                  <Layers size={13} />
                  {t("organize.changes", { count: history.length })}
                </div>
              ) : null}

              <OrganizeGrid
                path={session.primary.path}
                pages={pages}
                selected={selected}
                onSelectionChange={setSelected}
                onReorder={reorder}
                password={session.password || undefined}
                thumbWidth={thumbWidth}
                refreshedAt={refreshedAt}
              />

              <Card className="p-4 flex flex-col gap-3">
                <div className="flex flex-wrap items-start justify-between gap-3">
                  <div>
                    <h3 className="text-[13px] font-bold uppercase tracking-wider muted">{t("organize.outline")}</h3>
                    <p className="text-xs muted mt-1">{t("organize.outlineHint")}</p>
                  </div>
                  <div className="flex items-center gap-2">
                    {outlineSaved ? <Badge tone="ok">{t("organize.outlineSaved")}</Badge> : null}
                    <Button size="sm" variant="ghost" icon={<Plus size={14} />} onClick={addOutline}>
                      {t("organize.outlineAdd")}
                    </Button>
                    <Button
                      size="sm"
                      variant="primary"
                      icon={<Save size={14} />}
                      onClick={() => void saveOutline()}
                      disabled={session.running}
                    >
                      {t("organize.outlineSave")}
                    </Button>
                  </div>
                </div>
                {outline.length === 0 ? (
                  <p className="text-xs muted">{t("organize.outlineNone")}</p>
                ) : (
                  <div className="flex flex-col gap-1.5">
                    <div className="organize-outline-row text-[11px] uppercase tracking-wider muted">
                      <span>{t("organize.outlineTitle")}</span>
                      <span>{t("organize.outlinePage")}</span>
                      <span>{t("organize.outlineDepth")}</span>
                      <span />
                      <span />
                      <span />
                    </div>
                    {outline.map((entry, index) => (
                      <div key={entry.id} className="organize-outline-row">
                        <TextInput
                          className="input-sm"
                          value={entry.title}
                          aria-label={`${t("organize.outlineTitle")} ${index + 1}`}
                          onChange={(event) => patchOutline(index, { title: event.target.value })}
                        />
                        <TextInput
                          className="input-sm"
                          type="number"
                          min={1}
                          max={pageCount}
                          value={entry.page}
                          aria-label={`${t("organize.outlinePage")} ${index + 1}`}
                          onChange={(event) =>
                            patchOutline(index, { page: Math.max(1, Number(event.target.value) || 1) })
                          }
                        />
                        <TextInput
                          className="input-sm"
                          type="number"
                          min={0}
                          value={entry.depth}
                          aria-label={`${t("organize.outlineDepth")} ${index + 1}`}
                          onChange={(event) =>
                            patchOutline(index, { depth: Math.max(0, Number(event.target.value) || 0) })
                          }
                        />
                        <IconButton
                          label={t("common.moveUp")}
                          onClick={() => moveOutline(index, -1)}
                          disabled={index === 0}
                        >
                          <ArrowUp size={13} />
                        </IconButton>
                        <IconButton
                          label={t("common.moveDown")}
                          onClick={() => moveOutline(index, 1)}
                          disabled={index === outline.length - 1}
                        >
                          <ArrowDown size={13} />
                        </IconButton>
                        <IconButton label={t("common.remove")} onClick={() => removeOutline(index)}>
                          <Trash2 size={13} />
                        </IconButton>
                      </div>
                    ))}
                  </div>
                )}
              </Card>

              {session.result ? <ResultCard result={session.result} onReset={session.resetResult} /> : null}
            </>
          )
        }
        side={
          session.primary ? (
            <>
              <OutputBar session={session} runLabel={t("organize.apply")} onRun={() => void save()} />
              <OptionCard title={t("organize.title")}>
                <div className="flex flex-col gap-2 text-[13px]">
                  <p className="muted">{t("merge.dragHint")}</p>
                  <div className="flex flex-wrap gap-2">
                    <Button
                      size="sm"
                      icon={<FilePlus2 size={14} />}
                      onClick={addBlankPage}
                      title={t("organize.addBlank")}
                    >
                      {t("organize.addBlank")}
                    </Button>
                    <Button
                      size="sm"
                      icon={<RotateCcw size={14} />}
                      onClick={() => rotateSelection(-90)}
                      disabled={!selected.size}
                    >
                      90°
                    </Button>
                    <Button size="sm" onClick={() => rotateSelection(180)} disabled={!selected.size}>
                      180°
                    </Button>
                    <Button
                      size="sm"
                      icon={<RotateCw size={14} />}
                      onClick={() => rotateSelection(90)}
                      disabled={!selected.size}
                    >
                      90°
                    </Button>
                    <Button size="sm" icon={<Copy size={14} />} onClick={duplicateSelection} disabled={!selected.size}>
                      {t("common.duplicate")}
                    </Button>
                    <Button
                      size="sm"
                      variant="danger"
                      icon={<Trash2 size={14} />}
                      onClick={() => setConfirmDelete(true)}
                      disabled={!selected.size}
                    >
                      {t("common.delete")}
                    </Button>
                    <Button
                      size="sm"
                      icon={<Scissors size={14} />}
                      onClick={() => void extractSelection()}
                      disabled={!selected.size}
                    >
                      {t("common.extract")}
                    </Button>
                  </div>
                </div>
              </OptionCard>
              <Card soft className="p-4 text-xs muted flex items-start gap-2">
                <Save size={14} style={{ marginTop: 2 }} />
                <span>
                  {t("organize.deleteConfirmBody")}
                  <br />
                  <span className="kbd">Ctrl</span> <span className="kbd">A</span> · <span className="kbd">Del</span> ·{" "}
                  <span className="kbd">Ctrl</span> <span className="kbd">Z</span>
                </span>
              </Card>
            </>
          ) : null
        }
      />

      {confirmDelete ? (
        <Modal
          title={t("organize.deleteConfirm", { count: selected.size })}
          onClose={() => setConfirmDelete(false)}
          footer={
            <>
              <Button variant="ghost" onClick={() => setConfirmDelete(false)}>
                {t("common.cancel")}
              </Button>
              <Button variant="primary" onClick={deleteSelection}>
                {t("common.delete")}
              </Button>
            </>
          }
        >
          <p className="text-[13.5px]">{t("organize.deleteConfirmBody")}</p>
        </Modal>
      ) : null}
    </Screen>
  );
}
