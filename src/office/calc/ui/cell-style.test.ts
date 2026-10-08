import { describe, expect, it } from "vitest";
import { defaultCellStyle, type CellStyle } from "../../../lib/office-types";
import { cellTextStyle } from "./cell-style";

const base = { numeric: false, headerBold: false, onHeaderFill: false, linked: false };
const styled = (patch: Partial<CellStyle>): CellStyle => ({ ...defaultCellStyle(), ...patch });

describe("cellTextStyle", () => {
  it("leaves a plain cell unstyled and left-aligned", () => {
    expect(cellTextStyle({ ...base, style: defaultCellStyle() })).toEqual({
      fontWeight: undefined,
      fontStyle: undefined,
      textDecoration: undefined,
      color: undefined,
      textAlign: "left",
      justifyContent: "flex-start",
    });
  });

  it("aligns numbers right under general alignment and honours an explicit alignment", () => {
    const general = cellTextStyle({ ...base, style: defaultCellStyle(), numeric: true });
    expect(general.textAlign).toBe("right");
    expect(general.justifyContent).toBe("flex-end");
    const centered = cellTextStyle({ ...base, style: styled({ align: "center" }), numeric: true });
    expect(centered.textAlign).toBe("center");
    expect(centered.justifyContent).toBe("center");
  });

  it("draws a link underlined in the accent colour unless the cell has a colour of its own", () => {
    const link = cellTextStyle({ ...base, style: defaultCellStyle(), linked: true });
    expect(link.textDecoration).toBe("underline");
    expect(link.color).toBe("var(--accent)");
    const custom = cellTextStyle({ ...base, style: styled({ color: "#ff0000", strike: true }), linked: true });
    expect(custom.color).toBe("#ff0000");
    expect(custom.textDecoration).toBe("underline line-through");
  });

  it("puts white text on a table header fill", () => {
    expect(cellTextStyle({ ...base, style: defaultCellStyle(), onHeaderFill: true, headerBold: true })).toMatchObject({
      color: "#ffffff",
      fontWeight: 700,
    });
  });

  it("lets a conditional format add bold and italic and override the colour", () => {
    const result = cellTextStyle({
      ...base,
      style: styled({ color: "#112233" }),
      rule: { bold: true, italic: true, color: "#cc0000" },
    });
    expect(result).toMatchObject({ fontWeight: 700, fontStyle: "italic", color: "#cc0000" });
    const untouched = cellTextStyle({ ...base, style: styled({ color: "#112233" }), rule: { bold: false } });
    expect(untouched).toMatchObject({ fontWeight: undefined, color: "#112233" });
  });
});
