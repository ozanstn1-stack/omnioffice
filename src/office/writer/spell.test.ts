import { describe, expect, it } from "vitest";
import {
  extractParagraphText,
  replaceMisspelledWord,
} from "./spell";
import type { Run } from "../../lib/office-store";

describe("Writer Turkish spell module", () => {
  it("extracts text correctly from multiple runs", () => {
    const runs: Run[] = [
      { text: "Merhaba " },
      { text: "dünya" },
      { text: "!" },
    ];
    const { text, mapping } = extractParagraphText(runs);
    expect(text).toBe("Merhaba dünya!");
    expect(mapping.length).toBe("Merhaba dünya!".length);
  });

  it("replaces misspelled word in a single run", () => {
    const runs: Run[] = [{ text: "Bu yanlız bir gün." }];
    // "yanlız" starts at 3, ends at 9
    const updated = replaceMisspelledWord(runs, 3, 9, "yalnız");
    expect(updated[0].text).toBe("Bu yalnız bir gün.");
  });

  it("replaces misspelled word across boundaries safely", () => {
    const runs: Run[] = [{ text: "her" }, { text: "şey" }];
    const updated = replaceMisspelledWord(runs, 0, 6, "her şey");
    expect(updated.length).toBeGreaterThan(0);
  });
});
