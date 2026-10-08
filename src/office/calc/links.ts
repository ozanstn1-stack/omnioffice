/**
 * Hyperlinks of Calc cells: the target allow-list, internal "place in this
 * document" targets, and the model operations behind Insert link and Remove
 * link. Pure functions over the workbook model.
 *
 * A link target is one of three things and nothing else: a web address
 * (`http:`/`https:`), a mail address (`mailto:`) or a place in the document
 * (`#Sheet2!A1`, `#'My Sheet'!A1`, `#Name`). `safeLinkTarget` is the same rule as
 * `safe_link_target` in crates/officecore/src/model.rs, which the file readers
 * and writers apply: a document must not be able to open a local program or leak
 * credentials to a network share when its link is followed.
 */
import { emptyCell, type Cell, type Workbook } from "../../lib/office-types";
import { parseAddress } from "./formula";
import { withCellAt } from "./cells";

/** The longest target kept, in UTF-8 bytes, like the Rust side. */
const MAX_LINK_TARGET = 8_192;

/** Unicode Cc, what Rust's `char::is_control` rejects. */
function hasControlCharacter(text: string): boolean {
  for (const ch of text) {
    const code = ch.codePointAt(0)!;
    if (code <= 0x1f || (code >= 0x7f && code <= 0x9f)) return true;
  }
  return false;
}

/** The trimmed target when it is a web, mail or internal address; null for anything else. */
export function safeLinkTarget(target: string): string | null {
  const trimmed = target.trim();
  if (trimmed === "" || hasControlCharacter(trimmed)) return null;
  if (new TextEncoder().encode(trimmed).length > MAX_LINK_TARGET) return null;
  // ASCII-only lower-casing: the scheme check must not see "İ" as an "i".
  const lower = trimmed.replace(/[A-Z]/g, (letter) => letter.toLowerCase());
  const after = (scheme: string) => {
    if (!lower.startsWith(scheme)) return false;
    return lower.slice(scheme.length).replace(/^\/+/, "") !== "";
  };
  const allowed =
    after("http://") || after("https://") || after("mailto:") || (lower.startsWith("#") && lower.length > 1);
  return allowed ? trimmed : null;
}

export type LinkKind = "web" | "mail" | "place";

export function linkKind(target: string): LinkKind {
  const lower = target.trim().toLowerCase();
  if (lower.startsWith("#")) return "place";
  return lower.startsWith("mailto:") ? "mail" : "web";
}

/** True for a target that leaves the document (the opener handles it), false for `#...`. */
export function isExternalLink(target: string): boolean {
  return linkKind(target) !== "place";
}

const MAIL_ADDRESS = /^[^\s@/:]+@[^\s@/:]+$/;

/**
 * What the user typed in the address field of a web or mail link, as a target:
 * a bare mail address gets `mailto:`, an address without a scheme gets
 * `https://`. A scheme the allow-list refuses stays as typed, so the check
 * after it rejects it instead of silently rewriting it into something else.
 */
export function targetFromInput(kind: "web" | "mail", input: string): string {
  const text = input.trim();
  if (text === "") return "";
  if (kind === "mail") return /^mailto:/i.test(text) ? text : `mailto:${text}`;
  if (MAIL_ADDRESS.test(text)) return `mailto:${text}`;
  return /^[a-z][a-z0-9+.-]*:/i.test(text) ? text : `https://${text}`;
}

// ---------------------------------------------------------------------------
// Places in the document
// ---------------------------------------------------------------------------

/** `#Sheet2!A1`, quoting the sheet name when it is not a plain word. */
export function placeTarget(sheetName: string, reference: string): string {
  const plain = /^[A-Za-z_][A-Za-z0-9_.]*$/.test(sheetName);
  const name = plain ? sheetName : `'${sheetName.replace(/'/g, "''")}'`;
  return `#${name}!${reference.trim().toUpperCase()}`;
}

/** Splits `#Sheet!A1` / `#'My Sheet'!A1` / `#A1` / `#Name`; the sheet is null when the target names none. */
export function parsePlaceTarget(target: string): { sheet: string | null; reference: string } | null {
  const text = target.trim();
  if (!text.startsWith("#") || text.length < 2) return null;
  const body = text.slice(1);
  if (body.startsWith("'")) {
    let sheet = "";
    let index = 1;
    for (; index < body.length; index += 1) {
      if (body[index] !== "'") {
        sheet += body[index];
        continue;
      }
      if (body[index + 1] === "'") {
        sheet += "'";
        index += 1;
        continue;
      }
      break;
    }
    if (body[index] !== "'" || body[index + 1] !== "!" || body.length <= index + 2) return null;
    return { sheet, reference: body.slice(index + 2) };
  }
  const bang = body.lastIndexOf("!");
  if (bang < 0) return { sheet: null, reference: body };
  if (bang === 0 || bang === body.length - 1) return null;
  return { sheet: body.slice(0, bang), reference: body.slice(bang + 1) };
}

/** Where an internal link points: a sheet and the first cell of its reference. */
export interface PlaceTarget {
  sheetIndex: number;
  row: number;
  col: number;
}

function firstCell(reference: string): { row: number; col: number } | null {
  const head = reference.trim().split(":")[0].replace(/\$/g, "");
  return parseAddress(head);
}

/**
 * Resolves an internal target to a cell, following one defined name
 * (`#TaxRate` -> `Rates!$B$2`). Null when the sheet, the name or the cell does
 * not exist.
 */
export function resolvePlaceTarget(workbook: Workbook, currentSheet: number, target: string): PlaceTarget | null {
  const parsed = parsePlaceTarget(target);
  if (!parsed) return null;
  const sheetByName = (name: string) =>
    workbook.sheets.findIndex((sheet) => sheet.name.toLowerCase() === name.toLowerCase());
  if (parsed.sheet !== null) {
    const sheetIndex = sheetByName(parsed.sheet);
    const cell = firstCell(parsed.reference);
    return sheetIndex >= 0 && cell ? { sheetIndex, ...cell } : null;
  }
  const own = firstCell(parsed.reference);
  if (own) return { sheetIndex: currentSheet, ...own };
  const name = (workbook.names ?? []).find(
    (entry) => entry.name.toLowerCase() === parsed.reference.trim().toLowerCase(),
  );
  if (!name) return null;
  const definition = name.definition.replace(/^=/, "");
  const at = definition.lastIndexOf("!");
  if (at < 0) {
    const cell = firstCell(definition);
    return cell ? { sheetIndex: currentSheet, ...cell } : null;
  }
  const sheetName = definition
    .slice(0, at)
    .replace(/^'(.*)'$/, "$1")
    .replace(/''/g, "'");
  const sheetIndex = sheetByName(sheetName);
  const cell = firstCell(definition.slice(at + 1));
  return sheetIndex >= 0 && cell ? { sheetIndex, ...cell } : null;
}

// ---------------------------------------------------------------------------
// Cells
// ---------------------------------------------------------------------------

/** The cell's own link target when it passes the allow-list, else null. */
export function cellLinkTarget(cell: Cell | undefined): string | null {
  return cell?.link ? safeLinkTarget(cell.link) : null;
}

const HYPERLINK_CALL = /^\s*=?\s*HYPERLINK\s*\(/i;

/** True when the grid draws the cell as a link: a stored link that passes the allow-list, or a HYPERLINK formula. */
export function isLinkCell(cell: Cell | undefined): boolean {
  return cellLinkTarget(cell) !== null || HYPERLINK_CALL.test(cell?.formula ?? "");
}

/** The source of the first argument of a formula that is a `HYPERLINK(...)` call, else null. */
export function hyperlinkFormulaArgument(formula: string | null | undefined): string | null {
  const match = HYPERLINK_CALL.exec(formula ?? "");
  if (!match || !formula) return null;
  let depth = 1;
  let quote = false;
  const start = match[0].length;
  for (let index = start; index < formula.length; index += 1) {
    const ch = formula[index];
    if (ch === '"') quote = !quote;
    if (quote) continue;
    if (ch === "(" || ch === "{") depth += 1;
    else if (ch === ")" || ch === "}") {
      depth -= 1;
      if (depth === 0) return formula.slice(start, index).trim() || null;
    } else if (ch === "," && depth === 1) return formula.slice(start, index).trim() || null;
  }
  return null;
}

/** What the Insert link dialog collects. */
export interface LinkDraft {
  /** The target, already normalised; refused unless `safeLinkTarget` accepts it. */
  target: string;
  /**
   * The cell text. Null leaves the cell as it is; an empty string shows the
   * target; anything else replaces the cell's content with that text.
   */
  text: string | null;
  tooltip: string;
}

/** Puts a link on a cell as one model change; an unsafe target leaves the workbook as it is. */
export function setCellLink(workbook: Workbook, sheetIndex: number, address: string, draft: LinkDraft): Workbook {
  const target = safeLinkTarget(draft.target);
  const sheet = workbook.sheets[sheetIndex];
  if (!target || !sheet || !parseAddress(address)) return workbook;
  const current = sheet.cells[address] ?? emptyCell();
  let staged: Cell = {
    ...current,
    link: target,
    linkTooltip: draft.tooltip.trim() || null,
    linkDisplay: null,
  };
  if (draft.text !== null) {
    staged = { ...staged, formula: null, value: { kind: "text", value: draft.text === "" ? target : draft.text } };
  }
  return withCellAt(workbook, sheetIndex, address, staged);
}

/** Removes the link of a cell and keeps its text. */
export function clearCellLink(workbook: Workbook, sheetIndex: number, address: string): Workbook {
  const current = workbook.sheets[sheetIndex]?.cells[address];
  if (!current || (!current.link && !current.linkTooltip && !current.linkDisplay)) return workbook;
  return withCellAt(workbook, sheetIndex, address, { ...current, link: null, linkTooltip: null, linkDisplay: null });
}
