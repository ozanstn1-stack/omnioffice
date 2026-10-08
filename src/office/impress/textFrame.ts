import type { TextFrame, TextParagraph } from "../../lib/office-types";

/** A plain paragraph, used when a frame that had none gets its first text. */
function plainParagraph(): TextParagraph {
  return {
    text: "",
    level: 0,
    bold: false,
    italic: false,
    underline: false,
    sizePt: null,
    color: null,
    align: "left",
    bullet: false,
    runs: [],
  };
}

/** `template` carrying a new text. The runs described the old text, so they go. */
function withText(template: TextParagraph, text: string): TextParagraph {
  return { ...template, text, runs: [] };
}

/**
 * The paragraphs of a text frame after its text was edited in the plain
 * textarea. One paragraph per line; level, bullet, alignment, size and the
 * other paragraph formatting are kept per paragraph. A paragraph whose text
 * is unchanged is returned as is (its runs still describe it), a paragraph
 * whose text changed loses its runs, because the exporter writes `runs`
 * whenever present and would put the old text back.
 *
 * Lines are matched to the old paragraphs by the unchanged lines at both
 * ends, then by position. A line that has no old paragraph (the user pressed
 * Enter) inherits the formatting of the paragraph above it.
 */
export function editedParagraphs(previous: TextParagraph[], value: string): TextParagraph[] {
  const lines = value.split(/\r\n|\r|\n/);
  if (lines.length === previous.length && lines.every((line, index) => line === previous[index].text)) {
    return previous;
  }
  const shared = Math.min(lines.length, previous.length);
  let head = 0;
  while (head < shared && lines[head] === previous[head].text) head += 1;
  let tail = 0;
  while (tail < shared - head && lines[lines.length - 1 - tail] === previous[previous.length - 1 - tail].text) {
    tail += 1;
  }

  const result: TextParagraph[] = previous.slice(0, head);
  const changed = previous.slice(head, previous.length - tail);
  const edited = lines.slice(head, lines.length - tail);
  // Formatting for lines beyond the old ones when nothing precedes them.
  const after = previous[previous.length - tail] ?? previous[head] ?? previous[0] ?? plainParagraph();
  edited.forEach((line, index) => {
    const paired = changed[index];
    if (paired) result.push(line === paired.text ? paired : withText(paired, line));
    else result.push(withText(result[result.length - 1] ?? after, line));
  });
  return result.concat(previous.slice(previous.length - tail));
}

/** Applies one font size to every paragraph (and the runs that would override it). */
export function withFrameSize(frame: TextFrame, sizePt: number): TextFrame {
  return {
    ...frame,
    sizePt,
    paragraphs: frame.paragraphs.map((paragraph) => ({
      ...paragraph,
      sizePt,
      runs: paragraph.runs.map((run) => ({ ...run, sizePt: null })),
    })),
  };
}

/** Applies one alignment to every paragraph of the frame. */
export function withFrameAlign(frame: TextFrame, align: string): TextFrame {
  return { ...frame, paragraphs: frame.paragraphs.map((paragraph) => ({ ...paragraph, align })) };
}
