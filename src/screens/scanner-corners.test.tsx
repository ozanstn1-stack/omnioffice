import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import en from "../lib/i18n/en.json";

// The corner picker is the on-device replacement for the ML Kit scanner: these
// tests pin the contract that matters for privacy and for the flattening step -
// the preview comes from the backend, the four handles move with the keyboard,
// and "save" hands back pixel corners (or `undefined` when the whole photo is
// kept).
const scanPreview = vi.fn(async () => ({
  dataUrl: "data:image/png;base64,AAAA",
  width: 1000,
  height: 800,
}));
const scanDetect = vi.fn(async () => null);

vi.mock("../lib/office-api", () => ({
  scanPreview: (...args: unknown[]) => scanPreview(...(args as [])),
}));

vi.mock("../lib/scan-detect", async () => {
  const actual = await vi.importActual<typeof import("../lib/scan-detect")>("../lib/scan-detect");
  return {
    ...actual,
    scanDetect: (...args: unknown[]) => scanDetect(...(args as [])),
    detectedCorners: () => null,
  };
});

import { CornerPicker } from "./scanner-corners";

function renderPicker(overrides: Partial<Parameters<typeof CornerPicker>[0]> = {}) {
  const onSave = vi.fn();
  const onClose = vi.fn();
  render(
    <CornerPicker name="photo-1.jpg" path="C:/scan/photo-1.jpg" onSave={onSave} onClose={onClose} {...overrides} />,
  );
  return { onSave, onClose };
}

const handle = (corner: string) => screen.getByRole("button", { name: en[corner as keyof typeof en] });

describe("CornerPicker", () => {
  beforeEach(() => {
    scanPreview.mockClear();
    scanDetect.mockClear();
  });

  it("loads the preview from the backend and shows four handles", async () => {
    renderPicker();
    await waitFor(() => expect(screen.getByAltText("photo-1.jpg")).toBeInTheDocument());
    expect(scanPreview).toHaveBeenCalledWith("C:/scan/photo-1.jpg");
    for (const corner of [
      "scanner.cornerTopLeft",
      "scanner.cornerTopRight",
      "scanner.cornerBottomRight",
      "scanner.cornerBottomLeft",
    ]) {
      expect(handle(corner)).toBeInTheDocument();
    }
  });

  it("asks the detector and says so when it finds nothing", async () => {
    renderPicker();
    expect(await screen.findByText(en["scanner.autoMissing"])).toBeInTheDocument();
    expect(scanDetect).toHaveBeenCalledWith("C:/scan/photo-1.jpg");
  });

  it("moves a handle with the arrow keys by one percent (five with shift)", async () => {
    renderPicker();
    await waitFor(() => expect(screen.getByAltText("photo-1.jpg")).toBeInTheDocument());
    const topLeft = handle("scanner.cornerTopLeft");
    const beforeLeft = Number.parseFloat(topLeft.style.left);
    const beforeTop = Number.parseFloat(topLeft.style.top);
    fireEvent.keyDown(topLeft, { key: "ArrowRight" });
    expect(Number.parseFloat(topLeft.style.left)).toBeCloseTo(beforeLeft + 1, 5);
    fireEvent.keyDown(topLeft, { key: "ArrowDown", shiftKey: true });
    expect(Number.parseFloat(topLeft.style.top)).toBeCloseTo(beforeTop + 5, 5);
    expect(Number.parseFloat(topLeft.style.left)).toBeCloseTo(beforeLeft + 1, 5);
  });

  it("saves the corners in pixels of the full-size photo", async () => {
    const { onSave } = renderPicker();
    await waitFor(() => expect(screen.getByAltText("photo-1.jpg")).toBeInTheDocument());
    fireEvent.click(screen.getByRole("button", { name: en["common.save"] }));
    expect(onSave).toHaveBeenCalledTimes(1);
    const saved = onSave.mock.calls[0][0] as [number, number][] | undefined;
    expect(saved).toHaveLength(4);
    // The default inset (about 6 %) scaled by the preview's 1000x800 pixels.
    expect(saved?.[0][0]).toBeGreaterThan(0);
    expect(saved?.[0][0]).toBeLessThan(1000);
    expect(saved?.[0][1]).toBeLessThan(800);
  });

  it("keeps the whole photo when the frame is the full page", async () => {
    const { onSave } = renderPicker();
    await waitFor(() => expect(screen.getByAltText("photo-1.jpg")).toBeInTheDocument());
    fireEvent.click(screen.getByRole("button", { name: en["scanner.cornersWhole"] }));
    fireEvent.click(screen.getByRole("button", { name: en["common.save"] }));
    // No corners means "do not crop": the backend keeps the photo as it is.
    expect(onSave).toHaveBeenCalledWith(undefined);
  });

  it("reports a failure to load the preview", async () => {
    scanPreview.mockImplementationOnce(async () => {
      throw new Error("preview failed");
    });
    renderPicker();
    expect(await screen.findByText("preview failed")).toBeInTheDocument();
  });
});
