/**
 * TypeScript mirrors of the office document model (crates/officecore/src/model.rs).
 * Field names are camelCase and match the Rust serde output exactly.
 */

export type OfficeKind = "writer" | "calc" | "impress";

export interface DocMetadata {
  title: string;
  author: string;
  subject: string;
  keywords: string;
  creator: string;
  lastModifiedBy: string;
  created: string;
  modified: string;
}

export interface ImageData {
  name: string;
  mime: string;
  dataBase64: string;
  alt: string;
}

// ---------------------------------------------------------------------------
// Writer
// ---------------------------------------------------------------------------

export interface PageSetup {
  size: string;
  widthPt: number;
  heightPt: number;
  orientation: string;
  marginTopPt: number;
  marginRightPt: number;
  marginBottomPt: number;
  marginLeftPt: number;
  columns: number;
  columnSpacingPt: number;
  headerDistancePt: number;
  footerDistancePt: number;
  differentFirstPage: boolean;
}

export interface ParaStyle {
  id: string;
  name: string;
  basedOn: string | null;
  next: string | null;
  font: string | null;
  sizePt: number | null;
  bold: boolean | null;
  italic: boolean | null;
  underline: boolean | null;
  strike: boolean | null;
  color: string | null;
  highlight: string | null;
  align: string | null;
  lineSpacing: number | null;
  spaceBeforePt: number | null;
  spaceAfterPt: number | null;
  indentLeftPt: number | null;
  indentRightPt: number | null;
  firstLinePt: number | null;
  outlineLevel: number | null;
  keepWithNext: boolean | null;
  pageBreakBefore: boolean | null;
}

export interface ListInfo {
  kind: "bullet" | "number" | string;
  level: number;
  start: number;
  marker: string;
}

export interface ParaProps {
  style: string;
  align: string;
  lineSpacing: number;
  spaceBeforePt: number;
  spaceAfterPt: number;
  indentLeftPt: number;
  indentRightPt: number;
  firstLinePt: number;
  list: ListInfo | null;
  pageBreakBefore: boolean;
  /**
   * Pagination rules. Optional because files saved before 2.5 do not carry
   * them; undefined reads as "off", which is Word's default.
   */
  keepWithNext?: boolean;
  keepTogether?: boolean;
}

/** One line of a table of contents. */
export interface TocEntry {
  text: string;
  /** Heading level, 1..6. */
  level: number;
  /** Page number as of the last update in the editor. */
  page: number;
  /** Index of the heading block, used to jump to it. */
  anchor: number;
}

/** Formatting captured before a tracked formatting change. */
export interface RunFormat {
  bold: boolean;
  italic: boolean;
  underline: boolean;
  strike: boolean;
  color: string | null;
  highlight: string | null;
  font: string | null;
  sizePt: number | null;
}

/**
 * A tracked revision attached to a run.
 *
 * Deleted text stays in the model until the revision is accepted or rejected,
 * which is what makes accept/reject lossless.
 */
export interface RevisionMark {
  id: string;
  kind: "insert" | "delete" | "format" | string;
  author: string;
  date: string;
  original?: RunFormat | null;
}

/** A resolved document field (page number, cross reference, date, ...). */
export interface FieldRef {
  kind: "page" | "pages" | "date" | "time" | "title" | "author" | "ref" | "refPage" | "footnote" | "bookmark" | "figure" | "table" | string;
  target: string;
  cached: string;
}

export interface Run {
  text: string;
  bold: boolean;
  italic: boolean;
  underline: boolean;
  strike: boolean;
  color: string | null;
  highlight: string | null;
  font: string | null;
  sizePt: number | null;
  link: string | null;
  comment: string | null;
  superscript: boolean;
  subscript: boolean;
  /** Footnote id this run is the reference for (V3). */
  footnote?: string | null;
  /** Endnote id this run is the reference for (V3). */
  endnote?: string | null;
  /** A document field rendered at this position (V3). */
  field?: FieldRef | null;
  /** Tracked revision (V3). */
  revision?: RevisionMark | null;
}

/** True when the run is a tracked deletion. */
export function isDeletedRun(run: Run): boolean {
  return run.revision?.kind === "delete";
}

export interface TableCell {
  blocks: Block[];
  colspan: number;
  rowspan: number;
  background: string | null;
  align: string;
  valign: string;
  widthPt: number | null;
}

export interface TableRow {
  cells: TableCell[];
  heightPt: number | null;
  header: boolean;
}

export interface TableData {
  rows: TableRow[];
  columnWidthsPt: number[];
  borders: boolean;
  borderColor: string;
  align: string;
}

/**
 * Properties of one Writer section (V3).
 *
 * The document-level `page` / `header` / `footer` are the first section; every
 * section break carries the properties of the section that starts there.
 */
export interface SectionProps {
  page: PageSetup;
  header: Block[];
  footer: Block[];
  firstHeader: Block[];
  firstFooter: Block[];
  evenHeader: Block[];
  evenFooter: Block[];
  differentFirstPage: boolean;
  differentOddEven: boolean;
  columns: number;
  /** `newPage`, `continuous`, `oddPage` or `evenPage`. */
  start: string;
}

export type Block =
  | { type: "paragraph"; props: ParaProps; runs: Run[] }
  | { type: "table"; table: TableData }
  | { type: "image"; image: ImageData; widthPt: number; heightPt: number; align: string; caption: string }
  | { type: "pageBreak" }
  | { type: "rule" }
  | { type: "toc"; entries: TocEntry[] }
  | { type: "sectionBreak"; section: SectionProps };

/** A footnote or endnote; numbering is automatic by reference order. */
export interface Footnote {
  id: string;
  runs: Run[];
  marker: string;
}

export interface Bookmark {
  id: string;
  name: string;
  block: number;
  offset: number;
}

export interface CommentReply {
  author: string;
  text: string;
  created: string;
}

export interface DocComment {
  id: string;
  author: string;
  text: string;
  created: string;
  resolved: boolean;
  modified?: string;
  replies?: CommentReply[];
}

export interface TextDocument {
  id: string;
  title: string;
  page: PageSetup;
  styles: ParaStyle[];
  blocks: Block[];
  header: Block[];
  footer: Block[];
  comments: DocComment[];
  metadata: DocMetadata;
  /** V3 fields; all optional so documents saved by older builds still open. */
  footnotes?: Footnote[];
  endnotes?: Footnote[];
  bookmarks?: Bookmark[];
  trackChanges?: boolean;
  showRevisions?: boolean;
}

// ---------------------------------------------------------------------------
// Calc
// ---------------------------------------------------------------------------

export type CellValue =
  | { kind: "empty" }
  | { kind: "number"; value: number }
  | { kind: "text"; value: string }
  | { kind: "bool"; value: boolean }
  | { kind: "error"; value: string };

export interface BorderStyle {
  style: string;
  color: string;
}

export interface CellBorders {
  top: BorderStyle | null;
  right: BorderStyle | null;
  bottom: BorderStyle | null;
  left: BorderStyle | null;
}

export interface CellStyle {
  font: string | null;
  sizePt: number | null;
  bold: boolean;
  italic: boolean;
  underline: boolean;
  strike: boolean;
  color: string | null;
  fill: string | null;
  align: string;
  valign: string;
  wrap: boolean;
  rotation: number;
  borders: CellBorders;
  numberFormat: string;
}

export interface Cell {
  value: CellValue;
  formula: string | null;
  style: CellStyle;
  comment: string | null;
  /**
   * Hyperlink target; the cell text is the label.
   *
   * Optional because the Rust model defaults it, so a document written by an
   * older build simply has no `link` key.
   */
  link?: string | null;
}

export interface MergeRange {
  start: string;
  end: string;
}

/**
 * A cell-based anchor for floating sheet objects (V3.1 XLSX pictures).
 *
 * The offsets are EMU distances from the anchor cell's top-left corner. The
 * `toAddress` corner is only present when the source drawing used a two-cell
 * anchor; the exporter itself writes one-cell anchors from the pixel size.
 */
export interface CellAnchor {
  address: string;
  colOffEmu: number;
  rowOffEmu: number;
  toAddress?: string | null;
  toColOffEmu?: number;
  toRowOffEmu?: number;
}

/** A picture floating over a worksheet; mirrors the Rust `SheetImage`. */
export interface SheetImage {
  image: ImageData;
  anchor: CellAnchor;
  widthPx: number;
  heightPx: number;
  /** Clockwise rotation in degrees. */
  rotationDeg: number;
}

export interface ChartSeries {
  name: string;
  range: string;
  color: string | null;
}

export interface ChartData {
  kind: string;
  title: string;
  categories: string;
  series: ChartSeries[];
  legend: boolean;
  xTitle: string;
  yTitle: string;
  stacked: boolean;
  showLabels: boolean;
  /**
   * Cached category labels (ChartML `c:strCache`, V3.1). Optional so documents
   * saved by older builds stay valid; when populated the PPTX exporter embeds
   * the values in a workbook so charts render without the original range.
   */
  categoriesCache?: string[];
  /** Cached values per series (ChartML `c:numCache`, V3.1), aligned with `series`. */
  seriesValuesCache?: number[][];
}

export interface ChartPlacement {
  id: string;
  chart: ChartData;
  anchor: string;
  widthPx: number;
  heightPx: number;
}

export interface CondRule {
  id: string;
  range: string;
  kind: string;
  values: string[];
  fill: string | null;
  color: string | null;
  topN: number | null;
  stopIfTrue: boolean;
}

export interface Validation {
  id: string;
  range: string;
  kind: string;
  values: string[];
  min: number | null;
  max: number | null;
  message: string;
  allowBlank: boolean;
}

export interface FilterState {
  range: string;
  column: number;
  values: string[];
}

/** One aggregated column of a pivot table. */
export interface PivotValueField {
  field: string;
  aggregation: "sum" | "count" | "average" | "min" | "max";
}

/** A filter on one source field; an empty list keeps everything. */
export interface PivotFilter {
  field: string;
  values: string[];
}

/**
 * A pivot table over a cell range whose first row holds the field names.
 *
 * The definition is the source of truth (kept in `.oswk`); the rendered grid
 * is computed from it on the fly, so a pivot never goes stale in the model.
 */
export interface PivotTable {
  id: string;
  name: string;
  sourceSheet: string;
  source: string;
  rows: string[];
  columns: string[];
  values: PivotValueField[];
  filters: PivotFilter[];
  anchor: string;
}

/**
 * Sheet protection exactly as Excel wrote it (V3.1). The editor never cracks
 * or bypasses the verifier; `options` lists the locked actions that were on.
 */
export interface SheetProtection {
  enabled: boolean;
  /** Legacy 16-bit `password` hash, when present. */
  passwordHash: string | null;
  algorithmName: string;
  hashValue: string;
  saltValue: string;
  spinCount: number;
  options: string[];
}

/**
 * A pivot cache/table imported raw from a package (V3.1) so a re-export keeps
 * the live Excel pivot. The records are base64 because `.oswk` is JSON.
 */
export interface PreservedPivot {
  name: string;
  sheet: string;
  cacheId: number;
  definitionXml: string;
  recordsBase64: string | null;
  tableXml: string;
  recordsPart: string | null;
  source: string;
  fields: string[];
}

/** One column of a structured spreadsheet table. */
export interface TableColumn {
  name: string;
  formula: string | null;
}
/**
 * A structured spreadsheet table (Excel "ListObject"): a named range with a
 * header row, an optional totals row, banded rows, an optional filter and
 * calculated columns. Structured references such as `Sales[Amount]` resolve
 * against `name` and the column names.
 */
export interface SpreadsheetTable {
  id: string;
  name: string;
  range: string;
  hasHeaders: boolean;
  hasTotals: boolean;
  bandedRows: boolean;
  bandedColumns: boolean;
  headerFill: string | null;
  headerBold: boolean;
  styleName: string;
  columns: TableColumn[];
  filter: FilterState | null;
}

export interface Sheet {
  id: string;
  name: string;
  rowCount: number;
  colCount: number;
  cells: Record<string, Cell>;
  colWidths: Record<string, number>;
  rowHeights: Record<string, number>;
  merges: MergeRange[];
  freezeRows: number;
  freezeCols: number;
  charts: ChartPlacement[];
  /** Pictures floating over the sheet (V3.1). Optional on older documents. */
  images?: SheetImage[];
  pivotTables: PivotTable[];
  /** Structured tables (V3). */
  tables?: SpreadsheetTable[];
  conditional: CondRule[];
  validations: Validation[];
  filter: FilterState | null;
  showGridlines: boolean;
  tabColor: string | null;
  print: PrintSettings;
  /** Legacy sheet-protection hash; empty means the sheet is unprotected. */
  sheetProtection: string;
  /** Full protection state (V3.1); optional on documents from older builds. */
  protection?: SheetProtection;
}

/** Paper, orientation and print options; mirrors the Rust `PrintSettings`. */
export interface PrintSettings {
  /** Excel paper size code; 9 is A4, 1 is Letter. */
  paperSize: number;
  landscape: boolean;
  /** Percentage scale, 10..400. */
  scale: number;
  fitToWidth: number;
  fitToHeight: number;
  centerHorizontally: boolean;
  printGridlines: boolean;
  printHeadings: boolean;
  /** Row range repeated at the top of every page, e.g. "1:1". */
  printTitlesRows: string | null;
  differentFirstPage: boolean;
  differentOddEven: boolean;
  header: string;
  footer: string;
  /** V3.1 fields; optional so documents from older builds stay valid. */
  centerVertically?: boolean;
  /** Printed column range repeated at the left of every page, e.g. "A:A". */
  printTitlesCols?: string | null;
  /** The printed range as a relative A1 range, e.g. "A1:D40". */
  printArea?: string | null;
  /** Page margins in inches, as OOXML stores them. */
  marginLeft?: number;
  marginRight?: number;
  marginTop?: number;
  marginBottom?: number;
  marginHeader?: number;
  marginFooter?: number;
  firstHeader?: string;
  firstFooter?: string;
  evenHeader?: string;
  evenFooter?: string;
  /** Manual horizontal page breaks as 0-based row indexes. */
  rowBreaks?: number[];
  /** Manual vertical page breaks as 0-based column indexes. */
  colBreaks?: number[];
}

export function defaultPrintSettings(): PrintSettings {
  return {
    paperSize: 9,
    landscape: false,
    scale: 100,
    fitToWidth: 1,
    fitToHeight: 0,
    centerHorizontally: false,
    printGridlines: false,
    printHeadings: false,
    printTitlesRows: null,
    differentFirstPage: false,
    differentOddEven: false,
    header: "",
    footer: "",
    centerVertically: false,
    printTitlesCols: null,
    printArea: null,
    marginLeft: 0.7,
    marginRight: 0.7,
    marginTop: 0.75,
    marginBottom: 0.75,
    marginHeader: 0.3,
    marginFooter: 0.3,
    firstHeader: "",
    firstFooter: "",
    evenHeader: "",
    evenFooter: "",
    rowBreaks: [],
    colBreaks: [],
  };
}

/**
 * A workbook- or sheet-scoped defined name.
 *
 * `definition` holds the raw target - a range (`Data!A1:A99`), a cell, a
 * constant or a formula - so a name can point at anything a formula can
 * express. `sheet` is null for a workbook-level name, which is what makes
 * `VAT_RATE` visible from every sheet.
 */
export interface NamedRange {
  name: string;
  definition: string;
  sheet: string | null;
  comment?: string;
}

export interface Workbook {
  id: string;
  title: string;
  sheets: Sheet[];
  activeSheet: number;
  /** Defined names, workbook-level and per-sheet. */
  names: NamedRange[];
  metadata: DocMetadata;
  /** Pivot caches/tables imported raw from a package (V3.1). */
  preservedPivots?: PreservedPivot[];
}

// ---------------------------------------------------------------------------
// Impress
// ---------------------------------------------------------------------------

export interface SlideSize {
  preset: string;
  widthPt: number;
  heightPt: number;
}

export interface TextParagraph {
  text: string;
  level: number;
  bold: boolean;
  italic: boolean;
  underline: boolean;
  sizePt: number | null;
  color: string | null;
  align: string;
  bullet: boolean;
  runs: Run[];
}

export interface TextFrame {
  paragraphs: TextParagraph[];
  valign: string;
  font: string | null;
  sizePt: number | null;
  color: string | null;
  align: string;
}

export interface ShapeStyle {
  fill: string | null;
  stroke: string | null;
  strokeWidthPt: number;
  opacity: number;
  cornerRadiusPt: number;
  shadow: boolean;
}

export interface LineSpec {
  x2: number;
  y2: number;
  beginArrow: boolean;
  endArrow: boolean;
  dash: string;
}

export interface SlideObject {
  id: string;
  kind: string;
  x: number;
  y: number;
  w: number;
  h: number;
  rotation: number;
  z: number;
  text: TextFrame | null;
  image: ImageData | null;
  style: ShapeStyle | null;
  line: LineSpec | null;
  table: TableData | null;
  chart: ChartData | null;
  groupId: string | null;
  /** Children of a `group` object; coordinates are absolute (V3). */
  children?: SlideObject[];
  /** Placeholder role on a master/layout (V3). */
  placeholder?: string | null;
  name: string;
}

/** One slideshow animation (V3). */
export interface Animation {
  id: string;
  objectId: string;
  kind: "entrance" | "emphasis" | "exit" | string;
  effect: string;
  trigger: "onClick" | "withPrevious" | "afterPrevious" | string;
  durationMs: number;
  delayMs: number;
  order: number;
}

/** A layout inside a master; placeholder objects are inherited by slides. */
export interface SlideLayout {
  id: string;
  name: string;
  kind: string;
  objects: SlideObject[];
}

/** A slide master: theme, background and layouts (V3). */
export interface SlideMaster {
  id: string;
  name: string;
  theme: string;
  background: string | null;
  objects: SlideObject[];
  layouts: SlideLayout[];
}

export interface Slide {
  id: string;
  layout: string;
  masterId?: string | null;
  layoutId?: string | null;
  background: string | null;
  transition: string | null;
  transitionMs: number;
  objects: SlideObject[];
  animations?: Animation[];
  notes: string;
}

export interface Deck {
  id: string;
  title: string;
  size: SlideSize;
  theme: string;
  slides: Slide[];
  /** Masters with layouts (V3). */
  masters?: SlideMaster[];
  metadata: DocMetadata;
}

// ---------------------------------------------------------------------------
// Factories and defaults
// ---------------------------------------------------------------------------

export function uid(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }
  return `id-${Math.random().toString(36).slice(2)}-${Date.now().toString(36)}`;
}

export function emptyMetadata(): DocMetadata {
  return { title: "", author: "", subject: "", keywords: "", creator: "", lastModifiedBy: "", created: "", modified: "" };
}

export const PLATFORM_FONT = "'Segoe UI', 'PT Sans', Calibri, sans-serif";

export function defaultStyles(): ParaStyle[] {
  const style = (partial: Partial<ParaStyle> & { id: string; name: string }): ParaStyle => ({
    basedOn: null,
    next: null,
    font: "Calibri",
    sizePt: 11,
    bold: null,
    italic: null,
    underline: null,
    strike: null,
    color: "#1f2328",
    highlight: null,
    align: null,
    lineSpacing: null,
    spaceBeforePt: null,
    spaceAfterPt: 8,
    indentLeftPt: null,
    indentRightPt: null,
    firstLinePt: null,
    outlineLevel: null,
    keepWithNext: null,
    pageBreakBefore: null,
    ...partial,
    id: partial.id,
    name: partial.name,
  });
  return [
    style({ id: "Normal", name: "Normal", lineSpacing: 1.15, spaceAfterPt: 8 }),
    style({ id: "Title", name: "Title", font: "Calibri Light", sizePt: 28, bold: true, color: "#0f172a", spaceAfterPt: 6, next: "Subtitle" }),
    style({ id: "Subtitle", name: "Subtitle", sizePt: 15, italic: true, color: "#475569", spaceAfterPt: 14, next: "Normal" }),
    style({ id: "Heading1", name: "Heading 1", font: "Calibri Light", sizePt: 20, bold: true, color: "#1d4ed8", spaceBeforePt: 16, spaceAfterPt: 4, outlineLevel: 0, keepWithNext: true, next: "Normal" }),
    style({ id: "Heading2", name: "Heading 2", font: "Calibri Light", sizePt: 16, bold: true, color: "#334155", spaceBeforePt: 12, spaceAfterPt: 4, outlineLevel: 1, keepWithNext: true, next: "Normal" }),
    style({ id: "Heading3", name: "Heading 3", font: "Calibri Light", sizePt: 13, bold: true, color: "#334155", spaceBeforePt: 12, spaceAfterPt: 4, outlineLevel: 2, keepWithNext: true, next: "Normal" }),
    style({ id: "Quote", name: "Quote", italic: true, color: "#334155", indentLeftPt: 24, indentRightPt: 24, spaceBeforePt: 8, spaceAfterPt: 8 }),
    style({ id: "Caption", name: "Caption", sizePt: 9.5, italic: true, align: "center", color: "#64748b" }),
    style({ id: "Code", name: "Code", font: "Consolas", sizePt: 10, spaceAfterPt: 0 }),
  ];
}

export function defaultParaProps(styleId = "Normal"): ParaProps {
  return {
    style: styleId,
    align: "left",
    lineSpacing: 1.15,
    spaceBeforePt: 0,
    spaceAfterPt: 8,
    indentLeftPt: 0,
    indentRightPt: 0,
    firstLinePt: 0,
    list: null,
    pageBreakBefore: false,
    keepWithNext: false,
    keepTogether: false,
  };
}

export function defaultRun(text = ""): Run {
  return {
    text,
    bold: false,
    italic: false,
    underline: false,
    strike: false,
    color: null,
    highlight: null,
    font: null,
    sizePt: null,
    link: null,
    comment: null,
    superscript: false,
    subscript: false,
  };
}

export function defaultPageSetup(size = "a4", orientation = "portrait"): PageSetup {
  const sizes: Record<string, [number, number]> = {
    a4: [595.28, 841.89],
    a5: [419.53, 595.28],
    letter: [612, 792],
    legal: [612, 1008],
    a3: [841.89, 1190.55],
  };
  const [width, height] = sizes[size] ?? sizes.a4;
  const landscape = orientation === "landscape";
  return {
    size,
    widthPt: landscape ? height : width,
    heightPt: landscape ? width : height,
    orientation,
    marginTopPt: 72,
    marginRightPt: 72,
    marginBottomPt: 72,
    marginLeftPt: 72,
    columns: 1,
    columnSpacingPt: 24,
    headerDistancePt: 36,
    footerDistancePt: 36,
    differentFirstPage: false,
  };
}

export function defaultSectionProps(page = defaultPageSetup()): SectionProps {
  return {
    page,
    header: [],
    footer: [],
    firstHeader: [],
    firstFooter: [],
    evenHeader: [],
    evenFooter: [],
    differentFirstPage: false,
    differentOddEven: false,
    columns: page.columns,
    start: "newPage",
  };
}

export function newFootnote(id = uid()): Footnote {
  return { id, runs: [defaultRun()], marker: "" };
}

export function newTextDocument(title = "Untitled document"): TextDocument {
  return {
    id: uid(),
    title,
    page: defaultPageSetup(),
    styles: defaultStyles(),
    blocks: [{ type: "paragraph", props: defaultParaProps(), runs: [defaultRun()] }],
    header: [],
    footer: [],
    comments: [],
    metadata: { ...emptyMetadata(), title },
    footnotes: [],
    endnotes: [],
    bookmarks: [],
    trackChanges: false,
    showRevisions: true,
  };
}

export function defaultCellStyle(): CellStyle {
  return {
    font: null,
    sizePt: 11,
    bold: false,
    italic: false,
    underline: false,
    strike: false,
    color: null,
    fill: null,
    align: "general",
    valign: "bottom",
    wrap: false,
    rotation: 0,
    borders: { top: null, right: null, bottom: null, left: null },
    numberFormat: "General",
  };
}

export function emptyCell(): Cell {
  return { value: { kind: "empty" }, formula: null, style: defaultCellStyle(), comment: null, link: null };
}

export function cellText(cell: Cell | undefined): string {
  if (!cell) return "";
  if (cell.formula) return cell.formula;
  switch (cell.value.kind) {
    case "number":
      return String(cell.value.value);
    case "text":
      return cell.value.value;
    case "bool":
      return cell.value.value ? "TRUE" : "FALSE";
    case "error":
      return cell.value.value;
    default:
      return "";
  }
}

export function newSheet(name: string): Sheet {
  return {
    id: uid(),
    name,
    rowCount: 200,
    colCount: 26,
    cells: {},
    colWidths: {},
    rowHeights: {},
    merges: [],
    freezeRows: 0,
    freezeCols: 0,
    charts: [],
    pivotTables: [],
    tables: [],
    conditional: [],
    validations: [],
    filter: null,
    showGridlines: true,
    tabColor: null,
    print: defaultPrintSettings(),
    sheetProtection: "",
  };
}

export function newSpreadsheetTable(name: string, range: string, columns: string[]): SpreadsheetTable {
  return {
    id: uid(),
    name,
    range,
    hasHeaders: true,
    hasTotals: false,
    bandedRows: true,
    bandedColumns: false,
    headerFill: "#1D4ED8",
    headerBold: true,
    styleName: "TableStyleMedium2",
    columns: columns.map((column) => ({ name: column, formula: null })),
    filter: null,
  };
}

export function newWorkbook(title = "Untitled spreadsheet"): Workbook {
  return {
    id: uid(),
    title,
    sheets: [newSheet("Sheet1")],
    activeSheet: 0,
    names: [],
    metadata: { ...emptyMetadata(), title },
  };
}

export function newTextFrame(text: string, sizePt = 18): TextFrame {
  return {
    paragraphs: [{ text, level: 0, bold: false, italic: false, underline: false, sizePt, color: null, align: "left", bullet: false, runs: [] }],
    valign: "top",
    font: null,
    sizePt,
    color: null,
    align: "left",
  };
}

export function newSlideObject(kind: string, x: number, y: number, w: number, h: number): SlideObject {
  return {
    id: uid(),
    kind,
    x,
    y,
    w,
    h,
    rotation: 0,
    z: 1,
    text: null,
    image: null,
    style: { fill: kind === "rect" || kind === "ellipse" ? "#2563eb" : null, stroke: null, strokeWidthPt: 1.5, opacity: 1, cornerRadiusPt: 0, shadow: false },
    line: null,
    table: null,
    chart: null,
    groupId: null,
    children: [],
    placeholder: null,
    name: `${kind} ${Math.round(x)},${Math.round(y)}`,
  };
}

export function newAnimation(objectId: string, kind = "entrance", effect = "fade", trigger = "onClick", order = 1): Animation {
  return { id: uid(), objectId, kind, effect, trigger, durationMs: 500, delayMs: 0, order };
}

export function newSlide(layout = "titleContent"): Slide {
  return {
    id: uid(),
    layout,
    masterId: null,
    layoutId: null,
    background: null,
    transition: null,
    transitionMs: 500,
    objects: [],
    animations: [],
    notes: "",
  };
}

export function newSlideMaster(name = "Master"): SlideMaster {
  return {
    id: uid(),
    name,
    theme: "minimal",
    background: null,
    objects: [],
    layouts: [
      { id: uid(), name: "Title slide", kind: "title", objects: [] },
      { id: uid(), name: "Title and content", kind: "titleContent", objects: [] },
      { id: uid(), name: "Blank", kind: "blank", objects: [] },
    ],
  };
}

export function newDeck(title = "Untitled presentation"): Deck {
  return {
    id: uid(),
    title,
    size: { preset: "16:9", widthPt: 960, heightPt: 540 },
    theme: "minimal",
    slides: [newSlide()],
    masters: [],
    metadata: { ...emptyMetadata(), title },
  };
}

/** Plain text of a Writer block (used for word count and search). */
export function blockText(block: Block): string {
  switch (block.type) {
    case "paragraph":
      return block.runs.map((run) => run.text).join("");
    case "table":
      return block.table.rows
        .map((row) => row.cells.map((cell) => cell.blocks.map(blockText).join(" ")).join("\t"))
        .join("\n");
    case "image":
      return block.caption;
    case "pageBreak":
    case "sectionBreak":
      return "";
    default:
      return "";
  }
}

/** Document section list: first section from the document fields, then breaks. */
export function documentSections(document: TextDocument): SectionProps[] {
  const first = defaultSectionProps(document.page);
  first.header = document.header;
  first.footer = document.footer;
  first.differentFirstPage = document.page.differentFirstPage;
  const sections = [first];
  for (const block of document.blocks) {
    if (block.type === "sectionBreak") sections.push(block.section);
  }
  return sections;
}

/** The section in effect for a block index. */
export function sectionForBlock(document: TextDocument, blockIndex: number): SectionProps {
  const sections = documentSections(document);
  let current = sections[0];
  let breakIndex = 1;
  for (let index = 0; index <= blockIndex && index < document.blocks.length; index += 1) {
    if (document.blocks[index].type === "sectionBreak") {
      current = sections[breakIndex] ?? current;
      breakIndex += 1;
    }
  }
  return current;
}

/** Footnote ids in reference order across the document. */
export function footnoteOrder(document: TextDocument): string[] {
  const order: string[] = [];
  const collect = (runs: Run[]) => {
    for (const run of runs) {
      if (run.footnote && !order.includes(run.footnote)) order.push(run.footnote);
      if (run.endnote && !order.includes(run.endnote)) order.push(run.endnote);
    }
  };
  const walk = (block: Block) => {
    if (block.type === "paragraph") collect(block.runs);
    if (block.type === "table") {
      for (const row of block.table.rows) for (const cell of row.cells) cell.blocks.forEach(walk);
    }
  };
  document.blocks.forEach(walk);
  return order;
}

export function documentText(document: TextDocument): string {
  return document.blocks.map(blockText).join("\n");
}

export function wordCount(document: TextDocument): { words: number; characters: number; paragraphs: number } {
  const text = documentText(document);
  return {
    words: text.split(/\s+/).filter(Boolean).length,
    characters: text.length,
    paragraphs: document.blocks.length,
  };
}

export function newParaBlock(styleId = "Normal", text = ""): Block {
  return { type: "paragraph", props: defaultParaProps(styleId), runs: [defaultRun(text)] };
}

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
export function effectiveStyle(document: TextDocument, props: ParaProps): Required<Pick<ParaStyleLike, "font" | "sizePt" | "bold" | "italic" | "underline" | "strike" | "color" | "align" | "lineSpacing" | "spaceBeforePt" | "spaceAfterPt" | "indentLeftPt" | "indentRightPt" | "firstLinePt">> & { highlight: string | null } {
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
