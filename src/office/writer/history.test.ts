import { describe, expect, it } from "vitest";
import { defaultParaProps, newTextDocument, type Block, type TextDocument } from "../../lib/office-types";
import {
  MAX_UNDO_STEPS,
  applyOperation,
  applyTransaction,
  blockEdit,
  emptyHistory,
  paragraphEdit,
  record,
  redo,
  undo,
  type EditOperation,
  type Transaction,
} from "./history";

function paragraph(text: string): Block {
  return { type: "paragraph", props: defaultParaProps(), runs: [{ ...emptyRun(text), text }] };
}

function emptyRun(text: string) {
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

function documentWith(blocks: Block[]): TextDocument {
  return { ...newTextDocument(), blocks };
}

function tx(label: string, operations: EditOperation[], after: TextDocument): Transaction {
  return { label, operations, after };
}

describe("history", () => {
  it("undoes a block edit back to the checkpoint", () => {
    const before = documentWith([paragraph("hello")]);
    let state = emptyHistory();
    const after = documentWith([paragraph("hello world")]);
    const operation = paragraphEdit(
      0,
      before.blocks[0],
      after.blocks[0].type === "paragraph" ? after.blocks[0].runs : [],
    );
    state = record(state, before, tx("insert", [operation.operation], after));

    const result = undo(state)!;
    expect(result.model.blocks[0]).toEqual(before.blocks[0]);
    expect(result.state.undo).toHaveLength(0);
    expect(result.state.redo).toHaveLength(1);
  });

  it("redoes after an undo", () => {
    const before = documentWith([paragraph("a")]);
    const after = documentWith([paragraph("ab")]);
    const state = record(emptyHistory(), before, tx("type", [blockEdit(0, before.blocks[0], after.blocks[0])], after));
    const undone = undo(state)!;
    const redone = redo(undone.state)!;
    expect(redone.model.blocks[0]).toEqual(after.blocks[0]);
  });

  it("a new edit clears the redo branch", () => {
    const before = documentWith([paragraph("a")]);
    const after = documentWith([paragraph("ab")]);
    const state = record(emptyHistory(), before, tx("type", [blockEdit(0, before.blocks[0], after.blocks[0])], after));
    const undone = undo(state)!;
    const branched = record(
      undone.state,
      before,
      tx("different", [blockEdit(0, before.blocks[0], before.blocks[0])], before),
    );
    expect(branched.redo).toHaveLength(0);
    expect(branched.undo).toHaveLength(1);
  });

  it("walks back multiple transactions to the checkpoint", () => {
    const original = documentWith([paragraph("a")]);
    let state = emptyHistory();
    let current = original;
    for (let step = 0; step < 5; step += 1) {
      const next = documentWith([paragraph("a".repeat(step + 2))]);
      const op = blockEdit(0, current.blocks[0], next.blocks[0]);
      state = record(state, current, tx(`step ${step}`, [op], next));
      current = next;
    }
    expect(state.undo).toHaveLength(5);
    // Undo everything.
    let model = current;
    for (let step = 0; step < 5; step += 1) {
      const result = undo(state)!;
      state = result.state;
      model = result.model;
    }
    expect(model.blocks[0]).toEqual(original.blocks[0]);
    expect(undo(state)).toBeNull();
  });

  it("caps the undo stack without losing the checkpoint", () => {
    const start = documentWith([paragraph("start")]);
    const seed = record(emptyHistory(), start, tx("seed", [], start));
    let state = seed;
    let current = start;
    for (let step = 0; step < MAX_UNDO_STEPS + 50; step += 1) {
      const next = documentWith([paragraph(`v${step}`)]);
      state = record(state, current, tx(`step ${step}`, [blockEdit(0, current.blocks[0], next.blocks[0])], next));
      current = next;
    }
    expect(state.undo.length).toBeLessThanOrEqual(MAX_UNDO_STEPS);
    // Undo bottoms out at the checkpoint (the seed model), never crashes.
    let cursor = state;
    for (let step = 0; step < MAX_UNDO_STEPS; step += 1) {
      const result = undo(cursor);
      if (!result) break;
      cursor = result.state;
    }
    expect(undo(cursor)).toBeNull();
  });

  it("insert and delete block operations are exact inverses", () => {
    const document = documentWith([paragraph("a"), paragraph("b")]);
    const inserted: EditOperation = { kind: "insertBlock", index: 1, block: paragraph("middle") };
    const applied = applyOperation(document, inserted);
    expect(applied.model.blocks.map((block) => (block.type === "paragraph" ? block.runs[0].text : ""))).toEqual([
      "a",
      "middle",
      "b",
    ]);
    const reverted = applyOperation(applied.model, applied.inverse[0]);
    expect(reverted.model.blocks).toEqual(document.blocks);
  });

  it("header and footer edits undo independently of the body", () => {
    const before = { ...documentWith([paragraph("body")]), header: [paragraph("H1")], footer: [paragraph("F1")] };
    let state = emptyHistory();
    const after = { ...before, header: [paragraph("H2")] };
    state = record(
      state,
      before,
      tx("header", [{ kind: "updateHeader", before: before.header, after: after.header }], after),
    );
    const result = undo(state)!;
    expect(result.model.header).toEqual(before.header);
    expect(result.model.blocks).toEqual(before.blocks);
  });

  it("replays a transaction onto an arbitrary model", () => {
    const model = documentWith([paragraph("a"), paragraph("b")]);
    const operation = blockEdit(1, model.blocks[1], paragraph("B!"));
    const replayed = applyTransaction(model, tx("edit", [operation], model));
    expect((replayed.blocks[1] as Extract<Block, { type: "paragraph" }>).runs[0].text).toBe("B!");
  });
});
