import { describe, expect, it } from "vitest";
import { defaultParaProps, newTextDocument, type Run } from "../../lib/office-types";
import {
  acceptAll,
  acceptRevision,
  nextRevision,
  rejectAll,
  rejectRevision,
  revisionCount,
  revisionList,
  trackRunChanges,
} from "./revisions";

function run(text: string, extra: Partial<Run> = {}): Run {
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
    ...extra,
  };
}

function reviewedDocument() {
  const document = newTextDocument("Review");
  document.blocks = [
    {
      type: "paragraph",
      props: defaultParaProps(),
      runs: [
        run("kept "),
        run("added", {
          revision: { id: "i1", kind: "insert", author: "Ada", date: "2026-01-01T00:00:00Z", original: null },
        }),
      ],
    },
    {
      type: "paragraph",
      props: defaultParaProps(),
      runs: [
        run("gone", {
          revision: { id: "d1", kind: "delete", author: "Ada", date: "2026-01-01T00:00:00Z", original: null },
        }),
        run(" stays"),
      ],
    },
  ];
  return document;
}

describe("writer tracked changes", () => {
  it("lists revisions in document order", () => {
    const document = reviewedDocument();
    const list = revisionList(document);
    expect(list.map((revision) => revision.id)).toEqual(["i1", "d1"]);
    expect(revisionCount(document)).toBe(2);
  });

  it("accepts an insertion and rejects a deletion", () => {
    let document = reviewedDocument();
    document = acceptRevision(document, "i1");
    document = rejectRevision(document, "d1");
    const text = document.blocks
      .map((block) => (block.type === "paragraph" ? block.runs.map((item) => item.text).join("") : ""))
      .join("\n");
    expect(text).toContain("added");
    expect(text).toContain("gone");
    expect(revisionCount(document)).toBe(0);
  });

  it("rejects an insertion and accepts a deletion", () => {
    let document = reviewedDocument();
    document = rejectRevision(document, "i1");
    document = acceptRevision(document, "d1");
    const text = document.blocks
      .map((block) => (block.type === "paragraph" ? block.runs.map((item) => item.text).join("") : ""))
      .join("\n");
    expect(text).not.toContain("added");
    expect(text).not.toContain("gone");
  });

  it("accept all and reject all clear every revision", () => {
    const accepted = acceptAll(reviewedDocument());
    expect(revisionCount(accepted)).toBe(0);
    expect(accepted.trackChanges).toBe(false);
    const rejected = rejectAll(reviewedDocument());
    expect(revisionCount(rejected)).toBe(0);
  });

  it("walks revisions in both directions", () => {
    const document = reviewedDocument();
    expect(nextRevision(document, null, true)).toBe("i1");
    expect(nextRevision(document, "i1", true)).toBe("d1");
    expect(nextRevision(document, "i1", false)).toBe("d1");
  });

  it("records typed text as an insertion and backspaced text as a deletion", () => {
    const previous = [run("hello")];
    const inserted = trackRunChanges(previous, [run("hello world")], "Ada");
    expect(inserted).toHaveLength(2);
    expect(inserted[0].text).toBe("hello");
    expect(inserted[1].revision?.kind).toBe("insert");
    expect(inserted[1].text).toBe(" world");

    const deleted = trackRunChanges([run("hello world")], [run("hello")], "Ada");
    expect(deleted.some((item) => item.revision?.kind === "delete" && item.text === " world")).toBe(true);
  });

  it("keeps every surrounding run's own formatting while typing", () => {
    const previous = [run("hel", { bold: true }), run("lo", { italic: true })];
    const next = [run("hel", { bold: true }), run("lo", { italic: true }), run("!")];
    const result = trackRunChanges(previous, next, "Ada");
    expect(result.map((item) => item.text).join("")).toBe("hello!");
    expect(result[0]).toMatchObject({ text: "hel", bold: true, italic: false, revision: null });
    expect(result[1]).toMatchObject({ text: "lo", bold: false, italic: true, revision: null });
    expect(result[2].revision?.kind).toBe("insert");
    expect(result[2].text).toBe("!");
  });

  it("keeps the revision id of text inserted earlier in the same suggestion", () => {
    const insert = { id: "i1", kind: "insert", author: "Ada", date: "2026-01-01T00:00:00Z", original: null };
    const previous = [run("a"), run("bcd", { revision: insert })];
    const next = [run("a"), run("bcde", { revision: insert })];
    const result = trackRunChanges(previous, next, "Ada");
    expect(result.map((item) => item.text).join("")).toBe("abcde");
    const marked = result.filter((item) => item.revision?.kind === "insert");
    expect(marked).toHaveLength(1);
    expect(marked[0].revision?.id).toBe("i1");
  });

  it("cancels an insertion when its own text is deleted instead of marking a deletion", () => {
    const insert = { id: "i1", kind: "insert", author: "Ada", date: "2026-01-01T00:00:00Z", original: null };
    const previous = [run("keep "), run("gone", { revision: insert })];
    const next = [run("keep ")];
    const result = trackRunChanges(previous, next, "Ada");
    expect(result.map((item) => item.text).join("")).toBe("keep ");
    expect(result.some((item) => item.revision)).toBe(false);
  });

  it("carries footnote anchors through a text edit", () => {
    const previous = [run("a"), run("", { footnote: "fn1" }), run("b")];
    const next = [run("aX"), run("", { footnote: "fn1" }), run("b")];
    const result = trackRunChanges(previous, next, "Ada");
    expect(result.map((item) => item.text).join("")).toBe("aXb");
    expect(result.filter((item) => item.footnote === "fn1")).toHaveLength(1);
    expect(result.some((item) => item.revision?.kind === "insert" && item.text === "X")).toBe(true);
  });

  it("keeps a hidden pending deletion instead of silently accepting it", () => {
    const remove = { id: "d1", kind: "delete", author: "Ada", date: "2026-01-01T00:00:00Z", original: null };
    const previous = [run("keep"), run("gone", { revision: remove })];
    const next = [run("keep")];
    const result = trackRunChanges(previous, next, "Ada");
    expect(result.map((item) => item.text).join("")).toBe("keepgone");
    const deletion = result.find((item) => item.revision?.kind === "delete");
    expect(deletion?.text).toBe("gone");
    expect(deletion?.revision?.id).toBe("d1");
  });
});
