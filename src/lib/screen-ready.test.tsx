import { render } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { useScreenReady } from "./screen-ready";

function Probe({ screen, ready, detail }: { screen: string; ready: boolean; detail?: string }) {
  useScreenReady(screen, ready, detail);
  return null;
}

const root = () => document.documentElement.dataset;

afterEach(() => {
  delete root().screenReady;
  delete root().screenReadyDetail;
});

describe("screen ready signal", () => {
  it("is set only once the screen has what it needs to run", () => {
    const view = render(<Probe screen="pdfStudio" ready={false} />);
    expect(root().screenReady).toBeUndefined();
    view.rerender(<Probe screen="pdfStudio" ready detail="sample-1.pdf" />);
    expect(root().screenReady).toBe("pdfStudio");
    expect(root().screenReadyDetail).toBe("sample-1.pdf");
  });

  it("is cleared when the screen goes away or loses its input", () => {
    const view = render(<Probe screen="redact" ready detail="a.pdf" />);
    view.rerender(<Probe screen="redact" ready={false} />);
    expect(root().screenReady).toBeUndefined();
    view.rerender(<Probe screen="redact" ready detail="a.pdf" />);
    view.unmount();
    expect(root().screenReady).toBeUndefined();
    expect(root().screenReadyDetail).toBeUndefined();
  });

  it("leaves a newer screen's signal alone when an older one unmounts", () => {
    const older = render(<Probe screen="merge" ready />);
    render(<Probe screen="redact" ready />);
    older.unmount();
    expect(root().screenReady).toBe("redact");
  });
});
