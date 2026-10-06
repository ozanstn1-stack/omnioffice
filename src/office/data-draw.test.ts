import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));

import { jpegToPdf } from "../lib/image-pdf";
import { parseCsv, parseJsonTable } from "./ToolsScreens";

describe("Data import parsers", () => {
  it("keeps quoted delimiters, doubled quotes and quoted line breaks in one cell", () => {
    const { columns, rows } = parseCsv(
      'name,city,note\r\n"Doe, Jane",Ankara,"said ""hi"""\n"Ali",İzmir,"two\nlines"\n',
    );
    expect(columns).toEqual(["name", "city", "note"]);
    expect(rows).toEqual([
      ["Doe, Jane", "Ankara", 'said "hi"'],
      ["Ali", "İzmir", "two\nlines"],
    ]);
  });

  it("detects semicolon CSV, strips a BOM, skips blank lines and pads short rows", () => {
    const { columns, rows } = parseCsv("﻿a;b;c\n\n1;2,5\n");
    expect(columns).toEqual(["a", "b", "c"]);
    expect(rows).toEqual([["1", "2,5", ""]]);
  });

  it("reads lists of objects with differing keys, single objects and arrays of arrays", () => {
    expect(parseJsonTable('[{"a":1},{"b":true,"a":null}]')).toEqual({
      columns: ["a", "b"],
      rows: [
        ["1", ""],
        ["", "true"],
      ],
    });
    expect(parseJsonTable('{"x":{"y":1}}')).toEqual({ columns: ["x"], rows: [['{"y":1}']] });
    expect(parseJsonTable('[["h1","h2"],[1,2]]')).toEqual({ columns: ["h1", "h2"], rows: [["1", "2"]] });
    expect(() => parseJsonTable("[1,2]")).toThrow(/object/);
  });
});

describe("Draw PDF export", () => {
  it("wraps a JPEG into a one-page PDF with a valid cross-reference table", () => {
    const jpeg = new Uint8Array([0xff, 0xd8, 0xff, 0xe0, 0x00, 0x10, 0xff, 0xd9]);
    const pdf = jpegToPdf(jpeg, 1600, 1200, 800, 600);
    const text = new TextDecoder("latin1").decode(pdf);
    expect(text.startsWith("%PDF-1.4")).toBe(true);
    expect(text).toContain("/MediaBox [0 0 800 600]");
    expect(text).toContain("/Width 1600 /Height 1200");
    expect(text).toContain(`/Filter /DCTDecode /Length ${jpeg.length}`);
    expect(text.trimEnd().endsWith("%%EOF")).toBe(true);

    // Every xref entry must point at the start of its object.
    const startxref = Number(/startxref\n(\d+)/.exec(text)?.[1]);
    expect(text.slice(startxref, startxref + 4)).toBe("xref");
    const entries = [...text.slice(startxref).matchAll(/^(\d{10}) 00000 n $/gm)].map((match) => Number(match[1]));
    expect(entries).toHaveLength(5);
    entries.forEach((offset, index) => expect(text.slice(offset, offset + 8)).toBe(`${index + 1} 0 obj\n`));
  });
});
