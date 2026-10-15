/**
 * Compare two Writer documents and produce a tracked-changes document.
 *
 * The result is the **revised** document's structure with differences marked as
 * revisions: text only in the revised document is an `insert`, text only in the
 * base is a `delete` (kept in the run list so the existing accept/reject flow
 * works). Blocks are aligned by type (a paragraph pairs with a paragraph even
 * when its text changed, so the word-level diff applies), and a block that
 * exists on only one side is shown as an insertion or a deletion. Each run
 * keeps its own formatting; a removed table becomes a delete-marked text block
 * because the model has no revision mark for tables. The diff is
 * case-sensitive, so Turkish `İ`/`i` and `I`/`ı` are distinguished.
 */
import { emptyRun } from "./runs";
import { newRevision } from "./revisions";
import { defaultParaProps, type Block, type Run, type TableCell, type TextDocument } from "../../lib/office-types";

/** Splits text into tokens, keeping whitespace runs so the text round-trips. */
function tokenize(text: string): string[] {
  return text.match(/\s+|\S+/g) ?? [];
}

/** Above this many cells the full LCS table is skipped to bound memory. */
const MAX_LCS_CELLS = 1_000_000;

/**
 * Longest common subsequence of two token lists. The common prefix and suffix
 * are trimmed first, so a long paragraph with a small change keeps a small
 * table instead of the `rows x cols` allocation the first version always made.
 */
function commonTokens(base: string[], revised: string[]): [number, number][] {
  let prefix = 0;
  while (prefix < base.length && prefix < revised.length && base[prefix] === revised[prefix]) prefix += 1;
  let suffix = 0;
  while (
    suffix < base.length - prefix &&
    suffix < revised.length - prefix &&
    base[base.length - 1 - suffix] === revised[revised.length - 1 - suffix]
  ) {
    suffix += 1;
  }

  const matches: [number, number][] = [];
  for (let index = 0; index < prefix; index += 1) matches.push([index, index]);

  const baseMid = base.slice(prefix, base.length - suffix);
  const revisedMid = revised.slice(prefix, revised.length - suffix);
  if (baseMid.length > 0 && revisedMid.length > 0 && baseMid.length * revisedMid.length <= MAX_LCS_CELLS) {
    const rows = baseMid.length;
    const cols = revisedMid.length;
    const table: number[][] = Array.from({ length: rows + 1 }, () => new Array<number>(cols + 1).fill(0));
    for (let i = rows - 1; i >= 0; i -= 1) {
      for (let j = cols - 1; j >= 0; j -= 1) {
        table[i][j] =
          baseMid[i] === revisedMid[j] ? table[i + 1][j + 1] + 1 : Math.max(table[i + 1][j], table[i][j + 1]);
      }
    }
    let i = 0;
    let j = 0;
    while (i < rows && j < cols) {
      if (baseMid[i] === revisedMid[j]) {
        matches.push([prefix + i, prefix + j]);
        i += 1;
        j += 1;
      } else if (table[i + 1][j] >= table[i][j + 1]) {
        i += 1;
      } else {
        j += 1;
      }
    }
  }
  // A middle too large for the table stays unmatched: the caller then emits one
  // deletion and one insertion for it, which is coarse but bounded.

  for (let index = 0; index < suffix; index += 1) {
    matches.push([base.length - suffix + index, revised.length - suffix + index]);
  }
  return matches;
}

function runsText(runs: Run[]): string {
  return runs.map((run) => run.text).join("");
}

/** A token together with the run it came from, so its formatting is known. */
interface StyledToken {
  text: string;
  style: Run;
}

function tokenizeRuns(runs: Run[]): StyledToken[] {
  const out: StyledToken[] = [];
  for (const run of runs) {
    for (const text of tokenize(run.text)) out.push({ text, style: run });
  }
  return out;
}

function sameStyle(a: Run, b: Run): boolean {
  return (
    a.bold === b.bold &&
    a.italic === b.italic &&
    a.underline === b.underline &&
    a.strike === b.strike &&
    a.sizePt === b.sizePt &&
    a.color === b.color &&
    a.highlight === b.highlight &&
    a.font === b.font &&
    a.link === b.link
  );
}

/** One run per group of consecutive tokens that share a style. */
function markedRuns(tokens: StyledToken[], kind: "insert" | "delete", author: string): Run[] {
  const runs: Run[] = [];
  for (const token of tokens) {
    const last = runs[runs.length - 1];
    if (last && sameStyle(last, token.style)) {
      last.text += token.text;
    } else {
      runs.push({ ...token.style, text: token.text, revision: newRevision(kind, author) });
    }
  }
  return runs;
}

/** Word-level diff of one paragraph's runs, returning runs with revision marks. */
export function diffRuns(base: Run[], revised: Run[], author: string): Run[] {
  const baseTokens = tokenizeRuns(base);
  const revisedTokens = tokenizeRuns(revised);
  const matches = commonTokens(
    baseTokens.map((token) => token.text),
    revisedTokens.map((token) => token.text),
  );
  const runs: Run[] = [];
  let baseIndex = 0;
  let revisedIndex = 0;
  const boundaries: [number, number][] = [...matches, [baseTokens.length, revisedTokens.length]];
  for (const [matchBase, matchRevised] of boundaries) {
    const deleted = baseTokens.slice(baseIndex, matchBase);
    const inserted = revisedTokens.slice(revisedIndex, matchRevised);
    if (deleted.length) runs.push(...markedRuns(deleted, "delete", author));
    if (inserted.length) runs.push(...markedRuns(inserted, "insert", author));
    if (matchBase < baseTokens.length) {
      const token = revisedTokens[matchRevised];
      // An unchanged token keeps the revised run's own formatting.
      runs.push({ ...token.style, text: token.text, revision: null });
    }
    baseIndex = matchBase + 1;
    revisedIndex = matchRevised + 1;
  }
  return runs.length ? runs : [emptyRun()];
}

/** The text of a table, so a removed table can be shown as a deletion. */
function cellText(cell: TableCell): string {
  return cell.blocks
    .map((block) =>
      block.type === "paragraph" ? runsText(block.runs) : block.type === "table" ? tableText(block) : "",
    )
    .join(" ")
    .trim();
}

function tableText(block: Block): string {
  if (block.type !== "table") return "";
  return block.table.rows.map((row) => row.cells.map(cellText).join(" | ")).join(" ¶ ");
}

/** A base-only block as a delete-marked paragraph (the model has no block-level
 * revision mark, so a removed table/image becomes deleted text). */
function deletedBlock(block: Block, author: string): Block {
  const text =
    block.type === "paragraph" ? runsText(block.runs) : block.type === "table" ? tableText(block) : `[${block.type}]`;
  const template = block.type === "paragraph" ? block.runs[0] : undefined;
  return {
    type: "paragraph",
    props: block.type === "paragraph" ? { ...block.props } : defaultParaProps(),
    runs: [{ ...(template ?? emptyRun()), text, revision: newRevision("delete", author) }],
  };
}

/** A revised-only block as an insert-marked paragraph. */
function insertedBlock(block: Block, author: string): Block {
  if (block.type === "paragraph") {
    return { ...block, runs: block.runs.map((run) => ({ ...run, revision: newRevision("insert", author) })) };
  }
  // Tables and other blocks have no revision mark; keep them as they are.
  return structuredClone(block);
}

/**
 * Returns the revised document with the differences against `base` marked as
 * tracked changes. Blocks are aligned by type: paragraphs pair with paragraphs
 * (word-level diff), and a block present on only one side is inserted or
 * deleted. A matched table is carried over from the revised document.
 */
export function diffDocuments(base: TextDocument, revised: TextDocument, author = "Comparison"): TextDocument {
  const document: TextDocument = structuredClone(revised);
  const baseTypes = base.blocks.map((block) => block.type);
  const revisedTypes = revised.blocks.map((block) => block.type);
  const matches = commonTokens(baseTypes, revisedTypes);
  const blocks: Block[] = [];
  let baseIndex = 0;
  let revisedIndex = 0;
  const boundaries: [number, number][] = [...matches, [base.blocks.length, revised.blocks.length]];
  for (const [matchBase, matchRevised] of boundaries) {
    for (let index = baseIndex; index < matchBase; index += 1) blocks.push(deletedBlock(base.blocks[index], author));
    for (let index = revisedIndex; index < matchRevised; index += 1) {
      blocks.push(insertedBlock(revised.blocks[index], author));
    }
    if (matchBase < base.blocks.length) {
      const baseBlock = base.blocks[matchBase];
      const revisedBlock = revised.blocks[matchRevised];
      if (baseBlock.type === "paragraph" && revisedBlock.type === "paragraph") {
        blocks.push({ ...revisedBlock, runs: diffRuns(baseBlock.runs, revisedBlock.runs, author) });
      } else {
        blocks.push(structuredClone(revisedBlock));
      }
    }
    baseIndex = matchBase + 1;
    revisedIndex = matchRevised + 1;
  }
  document.blocks = blocks;
  document.showRevisions = true;
  return document;
}
