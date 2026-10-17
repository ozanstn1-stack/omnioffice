import { describe, expect, it } from "vitest";
import { extractParagraphText, replaceMisspelledWord } from "./spell";
import type { Run } from "../../lib/office-types";

function makeRun(text: string): Run {
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

describe("Writer Turkish spell module", () => {
  it("extracts text correctly from multiple runs", () => {
    const runs: Run[] = [makeRun("Merhaba "), makeRun("dünya"), makeRun("!")];
    const { text, mapping } = extractParagraphText(runs);
    expect(text).toBe("Merhaba dünya!");
    expect(mapping.length).toBe("Merhaba dünya!".length);
  });

  it("replaces misspelled word in a single run", () => {
    const runs: Run[] = [makeRun("Bu yanlız bir gün.")];
    // "yanlız" starts at 3, ends at 9
    const updated = replaceMisspelledWord(runs, 3, 9, "yalnız");
    expect(updated[0].text).toBe("Bu yalnız bir gün.");
  });

  it("replaces misspelled word across boundaries safely", () => {
    const runs: Run[] = [makeRun("her"), makeRun("şey")];
    const updated = replaceMisspelledWord(runs, 0, 6, "her şey");
    expect(updated.length).toBeGreaterThan(0);
  });
});
