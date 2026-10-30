import { describe, expect, it } from "vitest";

import { defaultParaProps, defaultRun, newTextDocument, type Block, type Run, type TextDocument } from "./office-types";
import { fillTemplate, hasTemplateFields, missingTemplateValues, templateFields } from "./template-fields";
import { TEMPLATES_WITH_FIELDS } from "../office/templates-fields";

function paragraph(runs: Run[], style = "Normal"): Block {
  return { type: "paragraph", props: defaultParaProps(style), runs };
}

function doc(blocks: Block[], header: Block[] = [], footer: Block[] = []): TextDocument {
  const document = newTextDocument("test");
  document.blocks = blocks;
  document.header = header;
  document.footer = footer;
  return document;
}

function text(document: TextDocument): string {
  const parts: string[] = [];
  for (const block of [...document.blocks, ...(document.header ?? []), ...(document.footer ?? [])]) {
    if (block.type === "paragraph") parts.push(block.runs.map((run) => run.text).join(""));
    if (block.type === "table") {
      for (const row of block.table.rows) {
        for (const cell of row.cells) {
          for (const inner of cell.blocks) {
            if (inner.type === "paragraph") parts.push(inner.runs.map((run) => run.text).join(""));
          }
        }
      }
    }
  }
  return parts.join("\n");
}

describe("templateFields", () => {
  it("collects fields from body, table cells and the header with their counts", () => {
    const table: Block = {
      type: "table",
      table: {
        rows: [
          {
            cells: [
              {
                blocks: [paragraph([defaultRun("Kurum: {{kurum}}")])],
                colspan: 1,
                rowspan: 1,
                background: null,
                align: "left",
                valign: "top",
                widthPt: null,
              },
            ],
            heightPt: null,
            header: false,
          },
        ],
        columnWidthsPt: [400],
        borders: true,
        borderColor: "#000",
        align: "left",
      },
    };
    const document = doc(
      [paragraph([defaultRun("Sayın {{ad_soyad}}, {{ad_soyad}}")]), table],
      [paragraph([defaultRun("{{tarih}}")])],
    );
    expect(templateFields(document)).toEqual([
      { name: "ad_soyad", count: 2 },
      { name: "kurum", count: 1 },
      { name: "tarih", count: 1 },
    ]);
    expect(hasTemplateFields(document)).toBe(true);
  });

  it("finds a placeholder split across runs", () => {
    const document = doc([paragraph([defaultRun("Sayın {{ad_"), defaultRun("soyad}},")])]);
    expect(templateFields(document)).toEqual([{ name: "ad_soyad", count: 1 }]);
  });

  it("ignores malformed and empty placeholders", () => {
    const document = doc([paragraph([defaultRun("{{}} {{boş luk}} {{ok}}")])]);
    expect(templateFields(document)).toEqual([{ name: "ok", count: 1 }]);
  });

  it("accepts Turkish letters in a field name", () => {
    const document = doc([paragraph([defaultRun("{{şehir}}")])]);
    expect(templateFields(document).map((field) => field.name)).toEqual(["şehir"]);
  });
});

describe("fillTemplate", () => {
  it("replaces placeholders and keeps the other runs untouched", () => {
    const document = doc([
      paragraph([defaultRun("Merhaba "), { ...defaultRun("{{ad}}"), bold: true }, defaultRun("!")]),
    ]);
    const filled = fillTemplate(document, { ad: "Ayşe Yılmaz" });
    expect(text(filled)).toBe("Merhaba Ayşe Yılmaz!");
    expect(filled.blocks[0]).toMatchObject({ type: "paragraph" });
    // The original document is not mutated.
    expect(text(document)).toBe("Merhaba {{ad}}!");
  });

  it("fills a placeholder split across runs by collapsing the paragraph", () => {
    const document = doc([paragraph([{ ...defaultRun("Sayın {{ad_"), bold: true }, defaultRun("soyad}},")])]);
    const filled = fillTemplate(document, { ad_soyad: "Mehmet Kaya" });
    expect(text(filled)).toBe("Sayın Mehmet Kaya,");
  });

  it("fills table cells and the header", () => {
    const table: Block = {
      type: "table",
      table: {
        rows: [
          {
            cells: [
              {
                blocks: [paragraph([defaultRun("{{kurum}}")])],
                colspan: 1,
                rowspan: 1,
                background: null,
                align: "left",
                valign: "top",
                widthPt: null,
              },
            ],
            heightPt: null,
            header: false,
          },
        ],
        columnWidthsPt: [400],
        borders: true,
        borderColor: "#000",
        align: "left",
      },
    };
    const filled = fillTemplate(doc([table], [paragraph([defaultRun("{{tarih}}")])]), {
      kurum: "İstanbul Valiliği",
      tarih: "30.10.2026",
    });
    expect(text(filled)).toBe("İstanbul Valiliği\n30.10.2026");
  });

  it("keeps a placeholder when its value is missing or blank", () => {
    const document = doc([paragraph([defaultRun("{{a}} / {{b}}")])]);
    const filled = fillTemplate(document, { a: "dolu", b: "   " });
    expect(text(filled)).toBe("dolu / {{b}}");
  });

  it("does not treat a value containing braces as a placeholder", () => {
    const document = doc([paragraph([defaultRun("{{a}}")])]);
    expect(text(fillTemplate(document, { a: "{{b}}" }))).toBe("{{b}}");
  });
});

describe("missingTemplateValues", () => {
  it("lists the fields without a value", () => {
    const fields = templateFields(doc([paragraph([defaultRun("{{a}} {{b}}")])]));
    expect(missingTemplateValues(fields, { a: "x", b: " " })).toEqual(["b"]);
    expect(missingTemplateValues(fields, { a: "x", b: "y" })).toEqual([]);
  });
});

describe("templates with fields", () => {
  it("each new template asks for at least four fields", () => {
    for (const template of TEMPLATES_WITH_FIELDS) {
      const model = template.build() as TextDocument;
      expect(templateFields(model).length).toBeGreaterThanOrEqual(4);
    }
  });

  it("the petition template asks for the party, institution, date and subject", () => {
    const petition = TEMPLATES_WITH_FIELDS.find((template) => template.id === "dilekce-alanli");
    expect(petition).toBeTruthy();
    const names = templateFields(petition!.build() as TextDocument).map((field) => field.name);
    for (const expected of ["ad_soyad", "kurum", "tarih", "konu"]) {
      expect(names).toContain(expected);
    }
  });
});
