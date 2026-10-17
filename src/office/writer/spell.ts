/**
 * Writer offline Turkish spell-checking integration (v5.3).
 *
 * Scans text nodes within paragraphs, identifies misspelled Turkish words using
 * the offline backend, and provides suggestion helpers and text replacement.
 */
import { checkTurkishText } from "../../lib/office-api";
import type { TextDocument, Run } from "../../lib/office-store";

export interface WordSpellError {
  blockId: string;
  runIndex: number;
  word: string;
  start: number;
  end: number;
  suggestions: string[];
}

/**
 * Extracts plain text representation and index mapping from paragraph runs.
 */
export function extractParagraphText(runs: Run[]): {
  text: string;
  mapping: Array<{ runIndex: number; runOffset: number; globalOffset: number }>;
} {
  let text = "";
  const mapping: Array<{ runIndex: number; runOffset: number; globalOffset: number }> = [];

  for (let rIdx = 0; rIdx < runs.length; rIdx++) {
    const runText = runs[rIdx].text || "";
    for (let cIdx = 0; cIdx < runText.length; cIdx++) {
      mapping.push({
        runIndex: rIdx,
        runOffset: cIdx,
        globalOffset: text.length,
      });
      text += runText[cIdx];
    }
  }

  return { text, mapping };
}

/**
 * Checks a single paragraph block for spelling errors.
 */
export async function checkBlockSpelling(
  blockId: string,
  runs: Run[],
  suggestionsLimit = 4,
): Promise<WordSpellError[]> {
  const { text } = extractParagraphText(runs);
  if (!text.trim()) return [];

  try {
    const res = await checkTurkishText(text, suggestionsLimit);
    return res.misspellings.map((m) => ({
      blockId,
      runIndex: 0,
      word: m.word,
      start: m.start,
      end: m.end,
      suggestions: m.suggestions || [],
    }));
  } catch {
    // If backend isn't available (e.g. unit test runner without Tauri mock), gracefully return empty
    return [];
  }
}

/**
 * Checks all paragraphs in a TextDocument.
 */
export async function checkDocumentSpelling(
  doc: TextDocument,
  suggestionsLimit = 4,
): Promise<Map<string, WordSpellError[]>> {
  const result = new Map<string, WordSpellError[]>();

  for (const block of doc.blocks) {
    if (block.type === "paragraph" && block.runs && block.runs.length > 0) {
      const errors = await checkBlockSpelling(block.id, block.runs, suggestionsLimit);
      if (errors.length > 0) {
        result.set(block.id, errors);
      }
    }
  }

  return result;
}

/**
 * Applies a suggestion replacement to a paragraph's runs.
 */
export function replaceMisspelledWord(
  runs: Run[],
  start: number,
  end: number,
  replacement: string,
): Run[] {
  const { text } = extractParagraphText(runs);
  if (start < 0 || end > text.length || start >= end) {
    return runs;
  }

  const newText = text.slice(0, start) + replacement + text.slice(end);

  // If there's only one run or no formatting complexity, simplify to single run preserving styles of first run
  if (runs.length <= 1) {
    const first = runs[0] || { text: "" };
    return [{ ...first, text: newText }];
  }

  // Multi-run reconstruction: replace proportional slice
  let currentOffset = 0;
  const newRuns: Run[] = [];

  for (const run of runs) {
    const runLen = (run.text || "").length;
    const runStart = currentOffset;
    const runEnd = currentOffset + runLen;

    if (end <= runStart || start >= runEnd) {
      // Unchanged run
      newRuns.push({ ...run });
    } else if (start <= runStart && end >= runEnd) {
      // Run is completely covered by misspelled word
      if (start === runStart) {
        newRuns.push({ ...run, text: replacement });
      }
    } else {
      // Partial overlap
      const localStart = Math.max(0, start - runStart);
      const localEnd = Math.min(runLen, end - runStart);
      const replaced =
        run.text.slice(0, localStart) +
        (start >= runStart ? replacement : "") +
        run.text.slice(localEnd);
      newRuns.push({ ...run, text: replaced });
    }

    currentOffset = runEnd;
  }

  return newRuns;
}
