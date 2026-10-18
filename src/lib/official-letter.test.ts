import { describe, expect, it } from "vitest";
import { checkOfficialLetter, fixOfficialLetterViolation, REGULATION_RULES } from "./official-letter";
import { defaultParaProps, newTextDocument } from "./office-types";

describe("Official Letter Regulation Checker (v5.3 - Issue #246)", () => {
  it("detects narrow page margins and fixes them with 1-click", () => {
    const doc = newTextDocument("Resmî Yazı");
    doc.page.marginTopPt = 40; // Less than 70.8 pt
    doc.page.marginLeftPt = 50;

    const violations = checkOfficialLetter(doc);
    const marginViolation = violations.find((v) => v.id === "margin-violation");
    expect(marginViolation).toBeDefined();
    expect(marginViolation?.canAutoFix).toBe(true);

    const fixed = fixOfficialLetterViolation(doc, "margin-violation");
    expect(fixed.page.marginTopPt).toBeGreaterThanOrEqual(REGULATION_RULES.MARGINS.minPt);
    expect(fixed.page.marginLeftPt).toBeGreaterThanOrEqual(REGULATION_RULES.MARGINS.minPt);
  });

  it("detects non-compliant fonts and auto-fixes to Times New Roman", () => {
    const doc = newTextDocument("Test");
    doc.blocks = [
      {
        type: "paragraph",
        props: {
          ...defaultParaProps(),
          align: "left",
          lineSpacing: 1.15,
          spaceBeforePt: 0,
          spaceAfterPt: 0,
        },
        runs: [
          {
            text: "Bu bir resmî yazı metnidir.",
            font: "Comic Sans MS",
            sizePt: 12,
            bold: false,
            italic: false,
            underline: false,
            strike: false,
            color: null,
            highlight: null,
            link: null,
            comment: null,
            superscript: false,
            subscript: false,
          },
        ],
      },
    ];

    const violations = checkOfficialLetter(doc);
    const fontViolation = violations.find((v) => v.id === "font-family-0");
    expect(fontViolation).toBeDefined();
    expect(fontViolation?.canAutoFix).toBe(true);

    const fixed = fixOfficialLetterViolation(doc, "font-family-0");
    const fixedBlock = fixed.blocks[0];
    if (fixedBlock.type === "paragraph") {
      expect(fixedBlock.runs[0].font).toBe("Times New Roman");
    }
  });

  it("detects missing Number, Subject and Date metadata lines", () => {
    const doc = newTextDocument("Boş Belge");
    const violations = checkOfficialLetter(doc);

    expect(violations.some((v) => v.id === "missing-number")).toBe(true);
    expect(violations.some((v) => v.id === "missing-subject")).toBe(true);
    expect(violations.some((v) => v.id === "missing-date")).toBe(true);
  });

  it("detects excessive line spacing and missing first-line indent on body paragraphs and auto-fixes both", () => {
    const doc = newTextDocument("Resmî Yazı");
    doc.blocks = [
      {
        type: "paragraph",
        props: {
          ...defaultParaProps(),
          align: "justify",
          lineSpacing: 2.0,
          firstLinePt: 0,
        },
        runs: [
          {
            text: "İlgi yazınız incelenmiş olup kurumumuzca yürütülen çalışmalar kapsamında gerekli tedbirlerin ivedilikle alınması hususunda bilgilerinizi ve gereğini arz ederim.",
            font: "Times New Roman",
            sizePt: 12,
            bold: false,
            italic: false,
            underline: false,
            strike: false,
            color: null,
            highlight: null,
            link: null,
            comment: null,
            superscript: false,
            subscript: false,
          },
        ],
      },
    ];

    const violations = checkOfficialLetter(doc);
    expect(violations.some((v) => v.id === "line-spacing-0")).toBe(true);
    expect(violations.some((v) => v.id === "indent-0")).toBe(true);

    let fixed = fixOfficialLetterViolation(doc, "line-spacing-0");
    fixed = fixOfficialLetterViolation(fixed, "indent-0");
    const block = fixed.blocks[0];
    if (block.type === "paragraph") {
      expect(block.props.lineSpacing).toBe(1.15);
      expect(block.props.firstLinePt).toBeCloseTo(35.4, 1);
    }
  });
});
