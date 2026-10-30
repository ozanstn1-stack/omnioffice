import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { OfficeDiffView } from "./office-diff-view";
import type { CalcDiff, ImpressDiff, WriterDiff } from "../lib/office-diff";

const writerDiff: WriterDiff = {
  kind: "writer",
  summary: { added: 1, removed: 0, changed: 1 },
  truncated: false,
  changes: [
    {
      kind: "changed",
      area: "body",
      blockIndex: 2,
      text: "Toplam 120 TL",
      segments: [
        { kind: "same", text: "Toplam " },
        { kind: "removed", text: "100" },
        { kind: "added", text: "120" },
        { kind: "same", text: " TL" },
      ],
    },
    {
      kind: "added",
      area: "body",
      blockIndex: 3,
      cell: { row: 2, col: 1 },
      text: "Yeni hücre",
    },
  ],
};

describe("OfficeDiffView", () => {
  it("shows the Writer summary and the changed paragraph with its segments", () => {
    render(<OfficeDiffView diff={writerDiff} leftLabel="v3" rightLabel="mevcut" />);
    expect(screen.getByText("Document comparison")).toBeInTheDocument();
    expect(screen.getByText("1 added")).toBeInTheDocument();
    expect(screen.getByText("1 changed")).toBeInTheDocument();
    expect(screen.getByText(/v3 → mevcut/)).toBeInTheDocument();
    expect(screen.getByText(/Paragraph 2/)).toBeInTheDocument();
    // The changed words are rendered as removable/added text pieces.
    expect(screen.getByText("100")).toBeInTheDocument();
    expect(screen.getByText("120")).toBeInTheDocument();
    expect(screen.getByText(/table cell 2:1/)).toBeInTheDocument();
  });

  it("renders Calc cell changes with before and after values", () => {
    const calc: CalcDiff = {
      kind: "calc",
      summary: { added: 1, removed: 0, changed: 1 },
      sheetsAdded: ["Yeni"],
      sheetsRemoved: [],
      truncated: false,
      cells: [{ sheet: "Veri", address: "A1", kind: "changed", before: "Ad", after: "Ad Soyad" }],
    };
    render(<OfficeDiffView diff={calc} />);
    expect(screen.getByText(/1 sheet\(s\) added/)).toBeInTheDocument();
    expect(screen.getByText("Veri!A1")).toBeInTheDocument();
    expect(screen.getByText("Ad Soyad")).toBeInTheDocument();
  });

  it("renders Impress slides with their title", () => {
    const impress: ImpressDiff = {
      kind: "impress",
      summary: { added: 1, removed: 0, changed: 0 },
      truncated: false,
      slides: [{ kind: "added", index: 3, title: "Yeni slayt", before: "", after: "Yeni slayt" }],
    };
    render(<OfficeDiffView diff={impress} />);
    expect(screen.getByText(/Slide 3/)).toBeInTheDocument();
    // The title appears in the slide heading and in the text segments.
    expect(screen.getAllByText("Yeni slayt").length).toBeGreaterThan(0);
  });

  it("says so when there is nothing to report", () => {
    render(
      <OfficeDiffView
        diff={{ kind: "writer", summary: { added: 0, removed: 0, changed: 0 }, truncated: false, changes: [] }}
      />,
    );
    expect(screen.getByText("There are no differences.")).toBeInTheDocument();
  });

  it("marks a truncated report", () => {
    render(<OfficeDiffView diff={{ ...writerDiff, truncated: true }} />);
    expect(screen.getByText("The list was cut at the report limit")).toBeInTheDocument();
  });
});
