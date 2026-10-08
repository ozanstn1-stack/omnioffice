/**
 * The Calc ribbon: tabs and tool groups. It owns no state beyond what it is
 * given; every button calls back into the editor.
 */
import {
  AlignCenter,
  AlignLeft,
  AlignRight,
  ArrowDownAZ,
  ArrowUpAZ,
  BarChart3,
  Bold,
  Columns3,
  Copy,
  CopyX,
  Eraser,
  Eye,
  EyeOff,
  FileText,
  Filter,
  FolderOpen,
  GitBranch,
  Grid3x3,
  Italic,
  Merge,
  Minus,
  Plus,
  Printer,
  Redo2,
  Save,
  Sigma,
  Snowflake,
  Sparkles,
  Table2,
  Tag,
  Underline,
  Undo2,
  XCircle,
} from "lucide-react";
import { useT } from "../../../lib/i18n";
import type { CellStyle } from "../../../lib/office-types";
import { Ribbon, RibbonGroup, ToolButton, ToolColor, ToolSelect } from "../../office-ui";
import { openIntoWorkspace } from "../../useOfficeSession";

/** Types `=NAME(` into the formula bar, where the autocomplete takes over. */
function insertFunction(name: string) {
  const active = window.document.activeElement as HTMLElement | null;
  const input = window.document.querySelector<HTMLInputElement>(".formula-input");
  if (input) {
    input.focus();
    const start = input.selectionStart ?? input.value.length;
    input.value = `${input.value.slice(0, start)}=${name}(`;
    input.dispatchEvent(new Event("input", { bubbles: true }));
  }
  void active;
}

export interface CalcRibbonActions {
  undo: () => void;
  redo: () => void;
  copy: () => void;
  clear: () => void;
  applyStyle: (patch: Partial<CellStyle>) => void;
  merge: () => void;
  borders: () => void;
  openChart: () => void;
  openPivot: () => void;
  openTable: () => void;
  toggleTablesPanel: () => void;
  trace: (kind: "precedents" | "dependents") => void;
  clearTrace: () => void;
  openConditional: () => void;
  openValidation: () => void;
  sort: (ascending: boolean) => void;
  filter: () => void;
  textToColumns: () => void;
  removeDuplicates: () => void;
  aiSummarize: () => void;
  aiFormula: () => void;
  insertRow: () => void;
  deleteRow: () => void;
  insertColumn: () => void;
  deleteColumn: () => void;
  openNames: () => void;
  openPrint: () => void;
  toggleFreeze: () => void;
  toggleGridlines: () => void;
  hideRows: () => void;
  unhideRows: () => void;
  hideColumns: () => void;
  unhideColumns: () => void;
  addSheet: () => void;
  save: () => void;
  saveAs: () => void;
  print: () => void;
}

export interface CalcRibbonProps {
  active: string;
  onSelect: (id: string) => void;
  canUndo: boolean;
  canRedo: boolean;
  /** Style of the active cell, for the toggle states of the font and align buttons. */
  activeStyle: CellStyle | undefined;
  frozen: boolean;
  showGridlines: boolean;
  tablesPanelOpen: boolean;
  traceActive: boolean;
  aiConfigured: boolean;
  /** A save is running; Save and Save as wait for it. */
  busy: boolean;
  actions: CalcRibbonActions;
}

export function CalcRibbon({
  active,
  onSelect,
  canUndo,
  canRedo,
  activeStyle,
  frozen,
  showGridlines,
  tablesPanelOpen,
  traceActive,
  aiConfigured,
  busy,
  actions,
}: CalcRibbonProps) {
  const t = useT();
  return (
    <Ribbon
      tabs={[
        { id: "home", label: t("calc.tabHome") },
        { id: "insert", label: t("calc.tabInsert") },
        { id: "formulas", label: t("calc.tabFormulas") },
        { id: "data", label: t("calc.tabData") },
        { id: "view", label: t("calc.tabView") },
      ]}
      active={active}
      onSelect={onSelect}
    >
      {active === "home" ? (
        <>
          <RibbonGroup label={t("writer.clipboard")}>
            <ToolButton
              icon={<Undo2 size={16} />}
              onClick={actions.undo}
              disabled={!canUndo}
              title={t("common.undo")}
            />
            <ToolButton
              icon={<Redo2 size={16} />}
              onClick={actions.redo}
              disabled={!canRedo}
              title={t("common.redo")}
            />
            <ToolButton icon={<Copy size={16} />} onClick={actions.copy} title={t("common.copy")} />
            <ToolButton icon={<Eraser size={16} />} onClick={actions.clear} title={t("calc.clearCells")} />
          </RibbonGroup>
          <RibbonGroup label={t("writer.font")}>
            <ToolButton
              icon={<Bold size={16} />}
              onClick={() => actions.applyStyle({ bold: !(activeStyle?.bold ?? false) })}
              active={activeStyle?.bold}
              title={t("writer.bold")}
            />
            <ToolButton
              icon={<Italic size={16} />}
              onClick={() => actions.applyStyle({ italic: !(activeStyle?.italic ?? false) })}
              active={activeStyle?.italic}
              title={t("writer.italic")}
            />
            <ToolButton
              icon={<Underline size={16} />}
              onClick={() => actions.applyStyle({ underline: !(activeStyle?.underline ?? false) })}
              active={activeStyle?.underline}
              title={t("writer.underline")}
            />
            <ToolColor
              value={activeStyle?.color ?? "#1f2328"}
              onChange={(color) => actions.applyStyle({ color })}
              title={t("writer.textColor")}
            />
            <ToolColor
              value={activeStyle?.fill ?? "#ffffff"}
              onChange={(fill) => actions.applyStyle({ fill })}
              title={t("calc.fillColor")}
            />
          </RibbonGroup>
          <RibbonGroup label={t("writer.paragraph")}>
            <ToolButton
              icon={<AlignLeft size={16} />}
              onClick={() => actions.applyStyle({ align: "left" })}
              active={activeStyle?.align === "left"}
              title={t("writer.alignLeft")}
            />
            <ToolButton
              icon={<AlignCenter size={16} />}
              onClick={() => actions.applyStyle({ align: "center" })}
              active={activeStyle?.align === "center"}
              title={t("writer.alignCenter")}
            />
            <ToolButton
              icon={<AlignRight size={16} />}
              onClick={() => actions.applyStyle({ align: "right" })}
              active={activeStyle?.align === "right"}
              title={t("writer.alignRight")}
            />
            <ToolButton icon={<Merge size={16} />} onClick={actions.merge} title={t("calc.mergeCells")} />
            <ToolButton icon={<Grid3x3 size={16} />} onClick={actions.borders} title={t("calc.borders")} />
          </RibbonGroup>
          <RibbonGroup label={t("calc.numberFormat")}>
            <ToolSelect
              value={activeStyle?.numberFormat ?? "General"}
              onChange={(numberFormat) => actions.applyStyle({ numberFormat })}
              options={[
                { value: "General", label: t("calc.formatGeneral") },
                { value: "0", label: "1234" },
                { value: "0.00", label: "12.34" },
                { value: "#,##0", label: "1,234" },
                { value: "#,##0.00", label: "1,234.56" },
                { value: "0%", label: "12%" },
                { value: "$#,##0.00", label: "$1,234.56" },
                { value: "dd.mm.yyyy", label: "31.12.2025" },
                { value: "hh:mm", label: "13:45" },
              ]}
              width={120}
            />
          </RibbonGroup>
        </>
      ) : null}

      {active === "insert" ? (
        <>
          <RibbonGroup label={t("calc.charts")}>
            <ToolButton icon={<BarChart3 size={16} />} label={t("calc.chart")} onClick={actions.openChart} />
          </RibbonGroup>
          <RibbonGroup label={t("calc.pivotTable")}>
            <ToolButton icon={<Grid3x3 size={16} />} label={t("calc.pivotTable")} onClick={actions.openPivot} />
          </RibbonGroup>
          <RibbonGroup label={t("calc.structuredTables")}>
            <ToolButton icon={<Table2 size={16} />} label={t("calc.insertTable")} onClick={actions.openTable} />
            <ToolButton
              icon={<Eye size={16} />}
              label={t("calc.tableList")}
              onClick={actions.toggleTablesPanel}
              active={tablesPanelOpen}
            />
          </RibbonGroup>
        </>
      ) : null}

      {active === "formulas" ? (
        <>
          <RibbonGroup label={t("calc.functions")}>
            <ToolButton icon={<Sigma size={16} />} label="SUM" onClick={() => insertFunction("SUM")} />
            <ToolButton label="AVERAGE" onClick={() => insertFunction("AVERAGE")} />
            <ToolButton label="IF" onClick={() => insertFunction("IF")} />
            <ToolButton label="COUNT" onClick={() => insertFunction("COUNT")} />
            <ToolButton label="ROUND" onClick={() => insertFunction("ROUND")} />
            <ToolButton label="VLOOKUP" onClick={() => insertFunction("VLOOKUP")} />
          </RibbonGroup>
          <RibbonGroup label={t("calc.auditing")}>
            <ToolButton
              icon={<GitBranch size={16} />}
              label={t("calc.tracePrecedents")}
              onClick={() => actions.trace("precedents")}
            />
            <ToolButton
              icon={<GitBranch size={16} />}
              label={t("calc.traceDependents")}
              onClick={() => actions.trace("dependents")}
            />
            <ToolButton
              icon={<XCircle size={16} />}
              label={t("calc.clearTrace")}
              onClick={actions.clearTrace}
              disabled={!traceActive}
            />
          </RibbonGroup>
          <RibbonGroup label={t("calc.structuredTables")}>
            <ToolButton icon={<Table2 size={16} />} label={t("calc.insertTable")} onClick={actions.openTable} />
            <ToolButton
              icon={<Eye size={16} />}
              label={t("calc.tableList")}
              onClick={actions.toggleTablesPanel}
              active={tablesPanelOpen}
            />
          </RibbonGroup>
          <RibbonGroup label={t("calc.conditional")}>
            <ToolButton
              icon={<Filter size={16} />}
              label={t("calc.conditionalFormatting")}
              onClick={actions.openConditional}
            />
            <ToolButton label={t("calc.dataValidation")} onClick={actions.openValidation} />
          </RibbonGroup>
        </>
      ) : null}

      {active === "data" ? (
        <>
          <RibbonGroup label={t("calc.sort")}>
            <ToolButton icon={<ArrowUpAZ size={16} />} label={t("calc.sortAsc")} onClick={() => actions.sort(true)} />
            <ToolButton
              icon={<ArrowDownAZ size={16} />}
              label={t("calc.sortDesc")}
              onClick={() => actions.sort(false)}
            />
            <ToolButton icon={<Filter size={16} />} label={t("calc.filter")} onClick={actions.filter} />
          </RibbonGroup>
          <RibbonGroup label={t("calc.dataTools")}>
            <ToolButton icon={<Columns3 size={16} />} label={t("calc.textToColumns")} onClick={actions.textToColumns} />
            <ToolButton
              icon={<CopyX size={16} />}
              label={t("calc.removeDuplicates")}
              onClick={actions.removeDuplicates}
            />
          </RibbonGroup>
          <RibbonGroup label={t("ai.edit.group")}>
            <ToolButton
              icon={<FileText size={16} />}
              label={t("ai.edit.btn.summarizeColumn")}
              onClick={actions.aiSummarize}
              disabled={!aiConfigured}
              title={aiConfigured ? undefined : `${t("ai.edit.btn.summarizeColumn")} - ${t("ai.edit.notConfigured")}`}
            />
            <ToolButton
              icon={<Sparkles size={16} />}
              label={t("ai.edit.btn.suggestFormula")}
              onClick={actions.aiFormula}
              disabled={!aiConfigured}
              title={aiConfigured ? undefined : `${t("ai.edit.btn.suggestFormula")} - ${t("ai.edit.notConfigured")}`}
            />
          </RibbonGroup>
          <RibbonGroup label={t("calc.structure")}>
            <ToolButton icon={<Plus size={16} />} label={t("calc.insertRow")} onClick={actions.insertRow} />
            <ToolButton icon={<Minus size={16} />} label={t("calc.deleteRow")} onClick={actions.deleteRow} />
            <ToolButton icon={<Plus size={16} />} label={t("calc.insertColumn")} onClick={actions.insertColumn} />
            <ToolButton icon={<Minus size={16} />} label={t("calc.deleteColumn")} onClick={actions.deleteColumn} />
          </RibbonGroup>
          <RibbonGroup label={t("calc.names")}>
            <ToolButton icon={<Tag size={16} />} label={t("calc.nameManager")} onClick={actions.openNames} />
          </RibbonGroup>
          <RibbonGroup label={t("calc.printLayout")}>
            <ToolButton icon={<Printer size={16} />} label={t("calc.printSetup")} onClick={actions.openPrint} />
          </RibbonGroup>
        </>
      ) : null}

      {active === "view" ? (
        <>
          <RibbonGroup label={t("calc.view")}>
            <ToolButton
              icon={<Snowflake size={16} />}
              label={frozen ? t("calc.unfreezePanes") : t("calc.freezePanes")}
              onClick={actions.toggleFreeze}
              active={frozen}
            />
            <ToolButton
              label={showGridlines ? t("calc.hideGridlines") : t("calc.showGridlines")}
              onClick={actions.toggleGridlines}
            />
          </RibbonGroup>
          <RibbonGroup label={t("calc.showHide")}>
            <ToolButton icon={<EyeOff size={16} />} label={t("calc.hideRows")} onClick={actions.hideRows} />
            <ToolButton icon={<Eye size={16} />} label={t("calc.unhideRows")} onClick={actions.unhideRows} />
            <ToolButton icon={<EyeOff size={16} />} label={t("calc.hideColumns")} onClick={actions.hideColumns} />
            <ToolButton icon={<Eye size={16} />} label={t("calc.unhideColumns")} onClick={actions.unhideColumns} />
          </RibbonGroup>
          <RibbonGroup label={t("calc.sheets")}>
            <ToolButton icon={<Plus size={16} />} label={t("calc.addSheet")} onClick={actions.addSheet} />
          </RibbonGroup>
        </>
      ) : null}

      <div className="ribbon-spacer" />
      <RibbonGroup>
        <ToolButton icon={<FolderOpen size={16} />} label={t("common.open")} onClick={() => void openIntoWorkspace()} />
        <ToolButton icon={<Save size={16} />} label={t("common.save")} onClick={actions.save} disabled={busy} />
        <ToolButton label={t("common.saveAs")} onClick={actions.saveAs} disabled={busy} />
        <ToolButton icon={<Printer size={16} />} label={t("common.print")} onClick={actions.print} />
      </RibbonGroup>
    </Ribbon>
  );
}
