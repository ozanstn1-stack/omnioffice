import { describe, expect, it } from "vitest";
import { diffDocuments, diffRuns } from "./compare";
import { emptyRun } from "./runs";
import type { Block, Run, TextDocument } from "../../lib/office-types";

function run(text: string): Run {
  return { ...emptyRun(), text };
}

function document(texts: string[]): TextDocument {
  return {
    id: "doc",
    title: "Doc",
    page: {} as TextDocument["page"],
    styles: [],
    blocks: texts.map((text) => ({ type: "paragraph" as const, props: {} as never, runs: [run(text)] })),
    header: [],
    footer: [],
    comments: [],
    metadata: {} as TextDocument["metadata"],
  };
}

function kinds(runs: Run[]): string[] {
  return runs.map((entry) => entry.revision?.kind ?? "same");
}

function text(runs: Run[]): string {
  return runs.map((entry) => entry.text).join("");
}

describe("document comparison", () => {
  it("marks an inserted word", () => {
    const runs = diffRuns([run("Merhaba dünya")], [run("Merhaba güzel dünya")], "A");
    expect(kinds(runs)).toEqual(["same", "same", "insert", "same"]);
    expect(text(runs)).toBe("Merhaba güzel dünya");
  });

  it("keeps a deleted word in the run list", () => {
    const runs = diffRuns([run("Merhaba güzel dünya")], [run("Merhaba dünya")], "A");
    expect(kinds(runs)).toEqual(["same", "same", "delete", "same"]);
    // The deleted text stays so accept/reject is lossless.
    expect(text(runs)).toBe("Merhaba güzel dünya");
  });

  it("is case-sensitive for Turkish letters", () => {
    const runs = diffRuns([run("İstanbul")], [run("istanbul")], "A");
    expect(kinds(runs)).toContain("delete");
    expect(kinds(runs)).toContain("insert");
    // The unchanged case is a single run.
    expect(diffRuns([run("Ankara")], [run("Ankara")], "A").every((entry) => !entry.revision)).toBe(true);
  });

  it("treats an extra paragraph as an insertion and a missing one as a deletion", () => {
    const paragraphRuns = (block: Block): Run[] => (block.type === "paragraph" ? block.runs : []);
    const added = diffDocuments(document(["Bir"]), document(["Bir", "İki"]), "A");
    expect(added.blocks).toHaveLength(2);
    expect(kinds(paragraphRuns(added.blocks[1]))).toEqual(["insert"]);
    const removed = diffDocuments(document(["Bir", "İki"]), document(["Bir"]), "A");
    expect(removed.blocks).toHaveLength(2);
    expect(kinds(paragraphRuns(removed.blocks[1]))).toEqual(["delete"]);
    // Revisions are visible.
    expect(removed.showRevisions).toBe(true);
  });

  it("leaves an identical document unmarked", () => {
    const result = diffDocuments(document(["Aynı metin"]), document(["Aynı metin"]), "A");
    const first = result.blocks[0];
    expect(first.type === "paragraph" ? first.runs.every((entry) => !entry.revision) : false).toBe(true);
  });
});
