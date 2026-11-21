import { renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => null) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));

import { useRecent } from "../../lib/store";
import { READER_RECENT_TOOL, readerRecentEntry, useRememberReaderDocument } from "./remember-recent";

describe("useRememberReaderDocument", () => {
  const add = vi.fn(async () => undefined);
  beforeEach(() => {
    add.mockClear();
    useRecent.setState({ add });
  });

  it("lists a PDF opened in the reader under Recent once it has loaded", () => {
    const { rerender } = renderHook(({ loaded }) => useRememberReaderDocument("tok-1", "report.pdf", loaded), {
      initialProps: { loaded: false },
    });
    expect(add).not.toHaveBeenCalled();
    rerender({ loaded: true });
    expect(add).toHaveBeenCalledTimes(1);
    expect(add).toHaveBeenCalledWith(
      expect.objectContaining({ path: "tok-1", fileName: "report.pdf", tool: READER_RECENT_TOOL }),
    );
    // Re-renders of the same open do not add it again.
    rerender({ loaded: true });
    expect(add).toHaveBeenCalledTimes(1);
  });

  it("stores the time in seconds", () => {
    expect(readerRecentEntry("p", "n", 1_700_000_000_500).timestamp).toBe(1_700_000_000);
  });
});
