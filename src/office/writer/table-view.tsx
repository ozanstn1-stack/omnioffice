/**
 * Writer table view: the interactive table surface (cell editing, selection,
 * merge/split and column resize).
 *
 * Extracted verbatim from WriterEditor.tsx (B4 editor seam 2). `StaticTable`
 * stays in views.tsx because it renders its cell blocks through `StaticBlocks`
 * and moving it here would make the two modules import each other.
 */
import { useEffect, useRef, useState } from "react";
import { useT } from "../../lib/i18n";
import { newParaBlock, type Block, type TableData } from "../../lib/office-types";
import { Dialog } from "../office-ui";
import { cellStartColumns, isRectangular, mergeCells, splitCell, type CellRange } from "./table-ops";
import { runsToHtml } from "./writerDom";

export function TableView({
  block,
  index,
  zoom,
  onUpdate,
  onSyncCell,
}: {
  block: Extract<Block, { type: "table" }>;
  index: number;
  zoom: number;
  onUpdate: (block: Block) => void;
  onSyncCell: (path: [number, number, number, number], element: HTMLElement) => void;
}) {
  const t = useT();
  const table = block.table;
  const [menuCell, setMenuCell] = useState<{ row: number; cell: number } | null>(null);
  // Selected cell rectangle (grid coordinates) for merge, and the anchor a
  // drag or shift-click extends from.
  const [selection, setSelection] = useState<CellRange | null>(null);
  const anchorRef = useRef<{ row: number; col: number } | null>(null);
  const draggingRef = useRef(false);
  // Column resize: a local preview while the pointer moves, committed to the
  // model once on pointerup so the whole drag is one undo step.
  const resizeRef = useRef<{ col: number; startX: number; startWidthPt: number; widthPt: number } | null>(null);
  const [resize, setResize] = useState<{ col: number; widthPt: number } | null>(null);
  const updateTable = (mutate: (table: TableData) => TableData) => onUpdate({ type: "table", table: mutate(table) });

  useEffect(() => {
    const move = (event: PointerEvent) => {
      const drag = resizeRef.current;
      if (!drag) return;
      const widthPt = Math.max(24, drag.startWidthPt + ((event.clientX - drag.startX) * 72) / 96 / zoom);
      drag.widthPt = widthPt;
      setResize({ col: drag.col, widthPt });
    };
    const up = () => {
      const drag = resizeRef.current;
      resizeRef.current = null;
      setResize(null);
      draggingRef.current = false;
      if (!drag) return;
      updateTable((current) => ({
        ...current,
        columnWidthsPt: current.columnWidthsPt.map((width, columnIndex) =>
          columnIndex === drag.col ? drag.widthPt : width,
        ),
      }));
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
    window.addEventListener("pointercancel", up);
    return () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      window.removeEventListener("pointercancel", up);
    };
  });

  const starts = cellStartColumns(table);
  const inSelection = (row: number, col: number) =>
    selection !== null &&
    row >= selection.row0 &&
    row <= selection.row1 &&
    col >= selection.col0 &&
    col <= selection.col1;

  /** Sets (or, while extending, grows) the selected rectangle. */
  const selectPosition = (row: number, col: number, extend: boolean) => {
    const anchor = anchorRef.current;
    if (extend && anchor) {
      setSelection({
        row0: Math.min(anchor.row, row),
        col0: Math.min(anchor.col, col),
        row1: Math.max(anchor.row, row),
        col1: Math.max(anchor.col, col),
      });
      return;
    }
    anchorRef.current = { row, col };
    setSelection({ row0: row, col0: col, row1: row, col1: col });
  };

  const menuCanSplit = (() => {
    if (!menuCell) return false;
    const cell = table.rows[menuCell.row]?.cells[menuCell.cell];
    return Boolean(cell && (Math.max(1, cell.colspan) > 1 || Math.max(1, cell.rowspan) > 1));
  })();
  const selectionValid = selection !== null && isRectangular(table, selection);
  const resizeWidth = (columnIndex: number, width: number) => (resize?.col === columnIndex ? resize.widthPt : width);

  return (
    <div className="writer-table-wrap">
      <table
        className={`writer-table${table.borders ? "" : " no-borders"}`}
        style={{
          width: `${(table.columnWidthsPt.reduce((sum, value) => sum + value, 0) || 400) * (96 / 72) * zoom}px`,
        }}
        onPointerUp={() => {
          draggingRef.current = false;
        }}
      >
        <colgroup>
          {table.columnWidthsPt.map((width, columnIndex) => (
            <col key={columnIndex} style={{ width: `${resizeWidth(columnIndex, width) * (96 / 72) * zoom}px` }} />
          ))}
        </colgroup>
        <tbody>
          {table.rows.map((row, rowIndex) => (
            <tr key={rowIndex} className={row.header ? "is-header" : ""}>
              {row.cells.map((cell, cellIndex) => {
                const gridCol = starts[rowIndex]?.[cellIndex] ?? cellIndex;
                const lastCol = gridCol + Math.max(1, cell.colspan) - 1;
                return (
                  <td
                    key={cellIndex}
                    colSpan={Math.max(1, cell.colspan)}
                    rowSpan={Math.max(1, cell.rowspan)}
                    className={inSelection(rowIndex, gridCol) ? "is-selected" : undefined}
                    data-grid-row={rowIndex}
                    data-grid-col={gridCol}
                    style={{
                      background: cell.background ?? undefined,
                      textAlign: (cell.align || "left") as "left" | "center" | "right",
                      verticalAlign: (cell.valign || "top") as "top" | "middle" | "bottom",
                    }}
                    onPointerDown={(event) => {
                      if (event.button !== 0) return;
                      draggingRef.current = true;
                      selectPosition(rowIndex, gridCol, event.shiftKey);
                    }}
                    onPointerEnter={() => {
                      if (!draggingRef.current) return;
                      selectPosition(rowIndex, gridCol, true);
                    }}
                    onContextMenu={(event) => {
                      event.preventDefault();
                      if (!inSelection(rowIndex, gridCol)) selectPosition(rowIndex, gridCol, false);
                      setMenuCell({ row: rowIndex, cell: cellIndex });
                    }}
                  >
                    {(cell.blocks.length > 0 ? cell.blocks : [newParaBlock()]).map((inner, innerIndex) =>
                      inner.type === "paragraph" ? (
                        <CellParagraph
                          key={innerIndex}
                          block={inner}
                          tablePath={[index, rowIndex, cellIndex, innerIndex]}
                          onSyncCell={onSyncCell}
                        />
                      ) : null,
                    )}
                    <span
                      className="writer-col-resize"
                      data-col-resize={lastCol}
                      onPointerDown={(event) => {
                        event.preventDefault();
                        event.stopPropagation();
                        const width = table.columnWidthsPt[lastCol] ?? 64;
                        resizeRef.current = {
                          col: lastCol,
                          startX: event.clientX,
                          startWidthPt: width,
                          widthPt: width,
                        };
                        setResize({ col: lastCol, widthPt: width });
                        try {
                          event.currentTarget.setPointerCapture?.(event.pointerId);
                        } catch {
                          // Pointer capture is unavailable (jsdom, older webviews).
                        }
                      }}
                      onDoubleClick={(event) => event.stopPropagation()}
                    />
                  </td>
                );
              })}
            </tr>
          ))}
        </tbody>
      </table>
      {menuCell ? (
        <Dialog
          title={t("writer.tableCellMenu", { row: menuCell.row + 1, cell: menuCell.cell + 1 })}
          onClose={() => setMenuCell(null)}
        >
          <div className="row">
            <button
              type="button"
              className="btn btn-soft"
              onClick={() => {
                updateTable((current) => ({
                  ...current,
                  rows: [
                    ...current.rows.slice(0, menuCell.row + 1),
                    { ...current.rows[menuCell.row], header: false },
                    ...current.rows.slice(menuCell.row + 1),
                  ],
                }));
                setMenuCell(null);
              }}
            >
              {t("writer.tableAddRowBelow")}
            </button>
            <button
              type="button"
              className="btn btn-soft"
              onClick={() => {
                updateTable((current) =>
                  current.rows.length > 1
                    ? { ...current, rows: current.rows.filter((_, position) => position !== menuCell.row) }
                    : current,
                );
                setMenuCell(null);
              }}
            >
              {t("writer.tableDeleteRow")}
            </button>
            <button
              type="button"
              className="btn btn-soft"
              onClick={() => {
                updateTable((current) => ({
                  ...current,
                  rows: current.rows.map((row) => ({
                    ...row,
                    cells: [
                      ...row.cells.slice(0, menuCell.cell + 1),
                      { ...row.cells[menuCell.cell] },
                      ...row.cells.slice(menuCell.cell + 1),
                    ],
                  })),
                }));
                setMenuCell(null);
              }}
            >
              {t("writer.tableAddColumn")}
            </button>
            <button
              type="button"
              className="btn btn-soft"
              onClick={() => {
                updateTable((current) => ({
                  ...current,
                  rows: current.rows.map((row) =>
                    row.cells.length > 1
                      ? { ...row, cells: row.cells.filter((_, position) => position !== menuCell.cell) }
                      : row,
                  ),
                }));
                setMenuCell(null);
              }}
            >
              {t("writer.tableDeleteColumn")}
            </button>
            <button
              type="button"
              className="btn btn-soft"
              onClick={() => {
                updateTable((current) => ({
                  ...current,
                  rows: current.rows.map((row, r) =>
                    r !== menuCell.row
                      ? row
                      : {
                          ...row,
                          cells: row.cells.map((cell, c) =>
                            c !== menuCell.cell ? cell : { ...cell, background: cell.background ? null : "#EEF2FF" },
                          ),
                        },
                  ),
                }));
                setMenuCell(null);
              }}
            >
              {t("writer.tableToggleShade")}
            </button>
            <button
              type="button"
              className="btn btn-soft"
              onClick={() => {
                updateTable((current) => ({ ...current, borders: !current.borders }));
                setMenuCell(null);
              }}
            >
              {t("writer.tableToggleBorders")}
            </button>
            <button
              type="button"
              className="btn btn-soft"
              disabled={!selectionValid}
              onClick={() => {
                if (!selection) return;
                updateTable((current) => mergeCells(current, selection));
                setSelection(null);
                setMenuCell(null);
              }}
            >
              {t("writer.tableMerge")}
            </button>
            <button
              type="button"
              className="btn btn-soft"
              disabled={!menuCanSplit}
              onClick={() => {
                if (!menuCell) return;
                updateTable((current) => splitCell(current, menuCell.row, menuCell.cell));
                setSelection(null);
                setMenuCell(null);
              }}
            >
              {t("writer.tableSplit")}
            </button>
          </div>
        </Dialog>
      ) : null}
    </div>
  );
}

function CellParagraph({
  block,
  tablePath,
  onSyncCell,
}: {
  block: Extract<Block, { type: "paragraph" }>;
  tablePath: [number, number, number, number];
  onSyncCell: (path: [number, number, number, number], element: HTMLElement) => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [focused, setFocused] = useState(false);
  useEffect(() => {
    if (!focused && ref.current) ref.current.innerHTML = runsToHtml(block.runs);
  }, [block.runs, focused]);
  return (
    <div
      ref={ref}
      className="para cell-para"
      contentEditable
      suppressContentEditableWarning
      onFocus={() => setFocused(true)}
      onBlur={(event) => {
        setFocused(false);
        onSyncCell(tablePath, event.currentTarget);
      }}
      onInput={(event) => onSyncCell(tablePath, event.currentTarget)}
      style={{ textAlign: (block.props.align || "left") as "left" | "center" | "right" }}
    />
  );
}
