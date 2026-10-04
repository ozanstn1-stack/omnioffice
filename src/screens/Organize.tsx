import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Copy, Layers, RotateCcw, RotateCw, Redo2, Save, Scissors, Trash2, Undo2 } from "lucide-react";
import { Badge, Button, Card, IconButton, Modal } from "../components/ui";
import { DropZone, InfoStrip, OutputBar, ResultCard } from "../components/files";
import { OptionCard, Screen, TwoColumn } from "../components/layout";
import { ThumbGrid, type PageItem } from "../components/pages";
import { useT } from "../lib/i18n";
import { useTool } from "../lib/useTool";
import { applyPagePlan } from "../lib/api";

export function Organize({ initialFiles, dragging }: { initialFiles?: string[]; dragging: boolean }) {
  const t = useT();
  const session = useTool({ suffix: "_organized", accept: "pdf", initialPaths: initialFiles });
  const [pages, setPages] = useState<PageItem[]>([]);
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [history, setHistory] = useState<PageItem[][]>([]);
  const [future, setFuture] = useState<PageItem[][]>([]);
  const [thumbWidth, setThumbWidth] = useState(180);
  const [refreshedAt, setRefreshedAt] = useState(0);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const copyRef = useRef(1);

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

  const changed = useMemo(() => {
    return (
      pages.some((page, index) => page.sourcePage !== index + 1 || page.rotationDelta !== 0) ||
      pages.length !== pageCount
    );
  }, [pageCount, pages]);

  const commit = useCallback(
    (next: PageItem[]) => {
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
    const next: PageItem[] = [];
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

  const reorder = (from: number, to: number) => {
    if (from === to) return;
    const next = [...pages];
    const [item] = next.splice(from, 1);
    next.splice(to, 0, item);
    commit(next);
  };

  const plan = useMemo(
    () => pages.map((page) => ({ source_page: page.sourcePage, rotation_delta: page.rotationDelta })),
    [pages],
  );

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
      const subsetPlan = subset.map((page) => ({ source_page: page.sourcePage, rotation_delta: page.rotationDelta }));
      const target = session.outputPath.replace(/_organized(\.pdf)?$/i, "_extracted.pdf");
      return applyPagePlan(
        session.primary?.path ?? "",
        subsetPlan,
        session.outputSpec(overwrite, target),
        jobId,
        session.password || undefined,
      );
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

              <ThumbGrid
                path={session.primary.path}
                pages={pages}
                selected={selected}
                onSelectionChange={setSelected}
                onReorder={reorder}
                password={session.password || undefined}
                thumbWidth={thumbWidth}
                refreshedAt={refreshedAt}
              />
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
