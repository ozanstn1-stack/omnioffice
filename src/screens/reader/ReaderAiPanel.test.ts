import { describe, expect, it } from "vitest";
import { chatHistory, readerSummaryOptions, splitCitations, summaryLanguage } from "./ReaderAiPanel";
import { useReaderImmersive } from "./immersive";

describe("splitCitations", () => {
  it("turns page markers with and without quotes into jumpable parts", () => {
    const parts = splitCitations('Total is 1200 [page 3: "toplam 1.200"] and due [sayfa 4].');
    expect(parts).toEqual([
      { kind: "text", text: "Total is 1200 " },
      { kind: "page", page: 3, quote: "toplam 1.200" },
      { kind: "text", text: " and due " },
      { kind: "page", page: 4, quote: undefined },
      { kind: "text", text: "." },
    ]);
  });

  it("keeps plain text untouched", () => {
    expect(splitCitations("No citations here.")).toEqual([{ kind: "text", text: "No citations here." }]);
  });
});

describe("chatHistory", () => {
  it("drops summary bubbles and empty turns", () => {
    expect(
      chatHistory([
        { role: "user", content: "Summarize", summary: true },
        { role: "assistant", content: "- point", summary: true },
        { role: "user", content: "What is the total?" },
        { role: "assistant", content: "1200 [page 3]" },
        { role: "assistant", content: "" },
      ]),
    ).toEqual([
      { role: "user", content: "What is the total?" },
      { role: "assistant", content: "1200 [page 3]" },
    ]);
  });
});

describe("reader focus mode", () => {
  it("is off by default and toggles", () => {
    expect(useReaderImmersive.getState().focus).toBe(false);
    useReaderImmersive.getState().setFocus(true);
    expect(useReaderImmersive.getState().focus).toBe(true);
    useReaderImmersive.getState().setFocus(false);
  });
});

describe("readerSummaryOptions", () => {
  it("summarizes in the app language, not the document's", () => {
    expect(summaryLanguage("tr")).toBe("Turkish");
    expect(summaryLanguage("en")).toBe("English");
    expect(readerSummaryOptions("tr", 16).language).toBe("Turkish");
  });

  it("asks for every page with page citations", () => {
    const options = readerSummaryOptions("tr", 16);
    expect(options.length).toBe("detailed");
    expect(options.focus).toContain("pages 1-16");
    expect(options.focus).toContain("[page N]");
  });
});
