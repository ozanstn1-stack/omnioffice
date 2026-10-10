import { describe, expect, it, vi } from "vitest";
import { bytesToBase64, dataUrlForBase64, mimeForBase64, strokePath, strokesToPngBase64 } from "./signature-pad";

describe("signature pad helpers", () => {
  it("builds a stroke path", () => {
    expect(
      strokePath([
        { x: 1, y: 2 },
        { x: 3, y: 4 },
      ]),
    ).toBe("M 1 2 L 3 4");
    expect(strokePath([])).toBe("");
  });

  it("base64-encodes bytes and sniffs the stored image mime", () => {
    expect(bytesToBase64(new Uint8Array([65, 66, 67]))).toBe("QUJD");
    expect(mimeForBase64("iVBORw0KGgo=")).toBe("image/png");
    expect(mimeForBase64("/9j/4AAQ")).toBe("image/jpeg");
    expect(dataUrlForBase64("/9j/4AAQ")).toBe("data:image/jpeg;base64,/9j/4AAQ");
  });

  it("rasterizes strokes through a 2D context", () => {
    const context = {
      beginPath: vi.fn(),
      moveTo: vi.fn(),
      lineTo: vi.fn(),
      stroke: vi.fn(),
      clearRect: vi.fn(),
      strokeStyle: "",
      lineWidth: 0,
      lineCap: "",
      lineJoin: "",
    } as unknown as CanvasRenderingContext2D;
    vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(context);
    vi.spyOn(HTMLCanvasElement.prototype, "toDataURL").mockReturnValue("data:image/png;base64,U0lH");
    expect(
      strokesToPngBase64([
        [
          { x: 0, y: 0 },
          { x: 10, y: 10 },
        ],
      ]),
    ).toBe("U0lH");
    expect(context.moveTo).toHaveBeenCalledWith(0, 0);
    expect(context.lineTo).toHaveBeenCalledWith(10, 10);
  });
});
