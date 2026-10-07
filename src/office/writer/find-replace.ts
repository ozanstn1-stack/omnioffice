/**
 * Pure find & replace for the Writer model.
 *
 * Matching runs on the plain text of each paragraph (its runs joined), so a
 * match may cross a formatting boundary and the regex anchors `^` / `$` mean
 * the paragraph edges. A replacement is spliced back into the runs: text
 * outside the match keeps its own run, the replacement takes the formatting of
 * the run the match starts in, and empty anchor runs (notes, fields) inside a
 * match are kept. Paragraphs, table cells (at any depth), the header and the
 * footer are searched, in that order.
 */
import type { Block, Run, TextDocument } from "../../lib/office-types";
import { normalizeParagraphRuns, runsText } from "./runs";

export interface FindOptions {
  matchCase: boolean;
  wholeWord: boolean;
  /** Treat the query as a JavaScript regular expression (`$1` groups in the replacement). */
  regex: boolean;
}

export type CompiledSearch = { ok: true; pattern: RegExp } | { ok: false; error: string };

/** Letters and digits of any script: `\b` only knows ASCII, so "çay" never matched as a whole word. */
const WORD_CHAR = "[\\p{L}\\p{N}_]";

/** The live count stops here so a pattern like `.` cannot build a huge list on every keystroke. */
export const MATCH_LIMIT = 10_000;

export function escapeRegExp(text: string): string {
  return text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/**
 * Builds the global search pattern, or null for an empty query. An invalid
 * regular expression is reported as an error value instead of throwing, so
 * the dialog can show it inline while the user is still typing.
 */
export function compileSearch(query: string, options: FindOptions): CompiledSearch | null {
  if (!query) return null;
  const flags = options.matchCase ? "gu" : "giu";
  const source = options.regex ? query : escapeRegExp(query);
  try {
    // Validate the query on its own first: wrapped in `(?:…)` an unbalanced
    // `a)|(b` would compile and silently mean something else.
    new RegExp(source, flags);
    const bounded = options.wholeWord ? `(?<!${WORD_CHAR})(?:${source})(?!${WORD_CHAR})` : source;
    return { ok: true, pattern: new RegExp(bounded, flags) };
  } catch (error) {
    return { ok: false, error: error instanceof Error ? error.message : String(error) };
  }
}

/** Non-empty matches of a global pattern in `text`, in order. */
export function findInText(text: string, pattern: RegExp, limit = Number.POSITIVE_INFINITY): RegExpMatchArray[] {
  const out: RegExpMatchArray[] = [];
  if (limit <= 0) return out;
  for (const match of text.matchAll(pattern)) {
    // Zero-length matches (`^`, `x*`) select nothing and cannot be replaced
    // one at a time, so they are not matches here.
    if (match[0].length === 0) continue;
    out.push(match);
    if (out.length >= limit) break;
  }
  return out;
}

/**
 * Expands `$$`, `$&`, `` $` ``, `$'`, `$1`…`$99` and `$<name>` in a regex
 * replacement exactly like `String.prototype.replace` does.
 */
export function expandReplacement(template: string, match: RegExpMatchArray): string {
  const input = match.input ?? match[0];
  const position = match.index ?? 0;
  const matched = match[0];
  const captures = match.length - 1;
  let out = "";
  for (let index = 0; index < template.length; index += 1) {
    const character = template[index];
    const next = template[index + 1];
    if (character !== "$" || next === undefined) {
      out += character;
      continue;
    }
    if (next === "$") {
      out += "$";
      index += 1;
    } else if (next === "&") {
      out += matched;
      index += 1;
    } else if (next === "`") {
      out += input.slice(0, position);
      index += 1;
    } else if (next === "'") {
      out += input.slice(position + matched.length);
      index += 1;
    } else if (next >= "0" && next <= "9") {
      // Two digits win when they name an existing group ("$12" with twelve
      // groups); otherwise one digit does and the second stays literal.
      const two = template.slice(index + 1, index + 3);
      const one = Number(next);
      if (/^\d\d$/.test(two) && Number(two) >= 1 && Number(two) <= captures) {
        out += match[Number(two)] ?? "";
        index += 2;
      } else if (one >= 1 && one <= captures) {
        out += match[one] ?? "";
        index += 1;
      } else {
        out += character;
      }
    } else if (next === "<" && match.groups && template.indexOf(">", index + 2) >= 0) {
      const close = template.indexOf(">", index + 2);
      out += match.groups[template.slice(index + 2, close)] ?? "";
      index = close;
    } else {
      out += character;
    }
  }
  return out;
}

/** The text a match is replaced with: literal unless regex mode expands groups. */
export function replacementFor(match: RegExpMatchArray, replacement: string, regex: boolean): string {
  return regex ? expandReplacement(replacement, match) : replacement;
}

/**
 * Replaces the plain-text range [from, to) with `text`. Runs outside the range
 * keep their formatting, the new text takes the formatting of the run that
 * held `from`, and empty anchor runs inside the range survive.
 */
export function spliceRuns(runs: Run[], from: number, to: number, text: string): Run[] {
  const out: Run[] = [];
  let position = 0;
  let inserted = false;
  for (const run of runs) {
    const start = position;
    const end = start + run.text.length;
    position = end;
    if (end <= from || start >= to || run.text.length === 0) {
      out.push(run);
      continue;
    }
    const head = run.text.slice(0, Math.max(0, from - start));
    const tail = run.text.slice(Math.max(0, to - start));
    if (head) out.push({ ...run, text: head });
    if (!inserted) {
      inserted = true;
      if (text) out.push({ ...run, text });
    }
    if (tail) out.push({ ...run, text: tail });
  }
  return normalizeParagraphRuns(out);
}

/** Replaces every match in one paragraph's runs. */
export function replaceAllInRuns(
  runs: Run[],
  pattern: RegExp,
  replacement: string,
  regex: boolean,
): { runs: Run[]; count: number } {
  const matches = findInText(runsText(runs), pattern);
  let next = runs;
  // Last match first, so the offsets of the earlier ones stay valid.
  for (let index = matches.length - 1; index >= 0; index -= 1) {
    const match = matches[index];
    const start = match.index ?? 0;
    next = spliceRuns(next, start, start + match[0].length, replacementFor(match, replacement, regex));
  }
  return { runs: next, count: matches.length };
}

// ---------------------------------------------------------------------------
// Document level
// ---------------------------------------------------------------------------

export type MatchScope = "body" | "header" | "footer";

export interface MatchLocation {
  scope: MatchScope;
  /** Block index, then (row, cell, block) for every table level below it. */
  path: number[];
  start: number;
  end: number;
}

/** A point in document order; a match is ordered by its start. */
export type MatchPosition = Pick<MatchLocation, "scope" | "path" | "start">;

const SCOPES: MatchScope[] = ["body", "header", "footer"];

type Paragraph = Extract<Block, { type: "paragraph" }>;

function scopeBlocks(document: TextDocument, scope: MatchScope): Block[] {
  return scope === "body" ? document.blocks : scope === "header" ? document.header : document.footer;
}

function withScopeBlocks(document: TextDocument, scope: MatchScope, blocks: Block[]): TextDocument {
  return scope === "body"
    ? { ...document, blocks }
    : scope === "header"
      ? { ...document, header: blocks }
      : { ...document, footer: blocks };
}

function visitParagraphs(blocks: Block[], prefix: number[], visit: (paragraph: Paragraph, path: number[]) => void) {
  blocks.forEach((block, index) => {
    const path = [...prefix, index];
    if (block.type === "paragraph") visit(block, path);
    else if (block.type === "table")
      block.table.rows.forEach((row, rowIndex) =>
        row.cells.forEach((cell, cellIndex) => visitParagraphs(cell.blocks, [...path, rowIndex, cellIndex], visit)),
      );
  });
}

/** Maps every paragraph of a block list, keeping untouched blocks and tables as they were. */
function mapParagraphs(blocks: Block[], map: (paragraph: Paragraph) => Paragraph): Block[] {
  let changed = false;
  const next = blocks.map((block) => {
    let result: Block = block;
    if (block.type === "paragraph") result = map(block);
    else if (block.type === "table") {
      let tableChanged = false;
      const rows = block.table.rows.map((row) => {
        let rowChanged = false;
        const cells = row.cells.map((cell) => {
          const cellBlocks = mapParagraphs(cell.blocks, map);
          if (cellBlocks === cell.blocks) return cell;
          rowChanged = true;
          return { ...cell, blocks: cellBlocks };
        });
        if (!rowChanged) return row;
        tableChanged = true;
        return { ...row, cells };
      });
      if (tableChanged) result = { ...block, table: { ...block.table, rows } };
    }
    if (result !== block) changed = true;
    return result;
  });
  return changed ? next : blocks;
}

/** Replaces the paragraph at `path`, rebuilding only the tables on the way. */
function updateParagraphAt(blocks: Block[], path: number[], update: (paragraph: Paragraph) => Paragraph): Block[] {
  const [index, rowIndex, cellIndex, ...rest] = path;
  const block = blocks[index];
  if (!block) return blocks;
  let next: Block = block;
  if (path.length === 1 && block.type === "paragraph") next = update(block);
  else if (path.length > 3 && block.type === "table") {
    const rows = block.table.rows.map((row, r) =>
      r !== rowIndex
        ? row
        : {
            ...row,
            cells: row.cells.map((cell, c) =>
              c !== cellIndex ? cell : { ...cell, blocks: updateParagraphAt(cell.blocks, rest, update) },
            ),
          },
    );
    next = { ...block, table: { ...block.table, rows } };
  }
  if (next === block) return blocks;
  const out = [...blocks];
  out[index] = next;
  return out;
}

function paragraphAt(blocks: Block[], path: number[]): Paragraph | null {
  const [index, rowIndex, cellIndex, ...rest] = path;
  const block = blocks[index];
  if (!block) return null;
  if (path.length === 1) return block.type === "paragraph" ? block : null;
  if (block.type !== "table") return null;
  const cell = block.table.rows[rowIndex]?.cells[cellIndex];
  return cell ? paragraphAt(cell.blocks, rest) : null;
}

/** Every match in document order, up to `limit`. */
export function documentMatches(document: TextDocument, pattern: RegExp, limit = MATCH_LIMIT): MatchLocation[] {
  const out: MatchLocation[] = [];
  for (const scope of SCOPES) {
    visitParagraphs(scopeBlocks(document, scope), [], (paragraph, path) => {
      if (out.length >= limit) return;
      for (const match of findInText(runsText(paragraph.runs), pattern, limit - out.length)) {
        const start = match.index ?? 0;
        out.push({ scope, path, start, end: start + match[0].length });
      }
    });
  }
  return out;
}

/** Replaces every match in the body, the tables, the header and the footer. */
export function replaceAllInDocument(
  document: TextDocument,
  pattern: RegExp,
  replacement: string,
  regex: boolean,
): { document: TextDocument; count: number } {
  let count = 0;
  const map = (paragraph: Paragraph): Paragraph => {
    const result = replaceAllInRuns(paragraph.runs, pattern, replacement, regex);
    if (result.count === 0) return paragraph;
    count += result.count;
    return { ...paragraph, runs: result.runs };
  };
  let next = document;
  for (const scope of SCOPES) {
    const blocks = scopeBlocks(document, scope);
    const mapped = mapParagraphs(blocks, map);
    if (mapped !== blocks) next = withScopeBlocks(next, scope, mapped);
  }
  return { document: next, count };
}

/**
 * Replaces one match. Returns null when the location no longer holds a match
 * of `pattern` (the text changed since it was found); `end` is the offset just
 * after the inserted text, where the search continues.
 */
export function replaceMatch(
  document: TextDocument,
  location: MatchLocation,
  pattern: RegExp,
  replacement: string,
  regex: boolean,
): { document: TextDocument; end: number } | null {
  const blocks = scopeBlocks(document, location.scope);
  const paragraph = paragraphAt(blocks, location.path);
  if (!paragraph) return null;
  const match = findInText(runsText(paragraph.runs), pattern).find(
    (candidate) => candidate.index === location.start && location.start + candidate[0].length === location.end,
  );
  if (!match) return null;
  const text = replacementFor(match, replacement, regex);
  const runs = spliceRuns(paragraph.runs, location.start, location.end, text);
  const updated = updateParagraphAt(blocks, location.path, (current) => ({ ...current, runs }));
  return { document: withScopeBlocks(document, location.scope, updated), end: location.start + text.length };
}

/** Document order of two positions (negative when `a` comes first). */
export function compareMatchPositions(a: MatchPosition, b: MatchPosition): number {
  const scope = SCOPES.indexOf(a.scope) - SCOPES.indexOf(b.scope);
  if (scope !== 0) return scope;
  const length = Math.min(a.path.length, b.path.length);
  for (let index = 0; index < length; index += 1) {
    if (a.path[index] !== b.path[index]) return a.path[index] - b.path[index];
  }
  if (a.path.length !== b.path.length) return a.path.length - b.path.length;
  return a.start - b.start;
}

export function sameMatch(a: MatchLocation, b: MatchLocation): boolean {
  return compareMatchPositions(a, b) === 0 && a.end === b.end;
}

/**
 * Index of the match to go to from `from`, wrapping around the document:
 * forward picks the first match after it (or at it, when `inclusive`),
 * backward the last match before it. Without a position the search starts at
 * the matching end of the document; -1 when there are no matches.
 */
export function nextMatchIndex(
  matches: MatchLocation[],
  from: MatchPosition | null,
  forward: boolean,
  inclusive = false,
): number {
  if (matches.length === 0) return -1;
  if (!from) return forward ? 0 : matches.length - 1;
  if (forward) {
    const index = matches.findIndex((match) => {
      const order = compareMatchPositions(match, from);
      return inclusive ? order >= 0 : order > 0;
    });
    return index >= 0 ? index : 0;
  }
  for (let index = matches.length - 1; index >= 0; index -= 1) {
    if (compareMatchPositions(matches[index], from) < 0) return index;
  }
  return matches.length - 1;
}
