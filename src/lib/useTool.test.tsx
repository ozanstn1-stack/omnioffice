import { render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

/**
 * Output-suggestion regressions:
 * - multi-output tools (PDF -> images) must default to "create new" and write
 *   into a folder named after the document, so a second export no longer fails
 *   with "a file with this name already exists";
 * - single-output tools keep the overwrite prompt ("error");
 * - Android stages results in an app-private folder and creates it first,
 *   because the native commands reject a missing output directory.
 */
const invoke = vi.fn(async (command: string, _payload?: unknown) => {
  if (command === "file_sizes") return [1024];
  return null;
});
const appDataDir = vi.fn(async () => "/data");
const join = vi.fn(async (...parts: string[]) => parts.join("/"));

vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...(args as [string])) }));
vi.mock("@tauri-apps/api/path", () => ({
  appDataDir: (...args: unknown[]) => appDataDir(...(args as [])),
  join: (...args: unknown[]) => join(...(args as string[])),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(async () => null) }));

import { useTool } from "./useTool";

function Harness({
  multiOutput,
  initialPaths,
  accept = "pdf",
}: {
  multiOutput: boolean;
  initialPaths: string[];
  accept?: "pdf" | "document";
}) {
  const session = useTool({
    suffix: multiOutput ? "_images" : "_merged",
    accept,
    multiOutput,
    loadInfo: false,
    initialPaths,
  });
  return (
    <div>
      <span data-testid="dir">{session.outputDir}</span>
      <span data-testid="overwrite">{session.overwrite}</span>
      <span data-testid="path">{session.outputPath}</span>
      <span data-testid="count">{session.files.length}</span>
    </div>
  );
}

describe("useTool output suggestions", () => {
  beforeEach(() => {
    invoke.mockClear();
    appDataDir.mockClear();
    join.mockClear();
  });

  it("puts multi-output results in a per-document folder and creates it", async () => {
    render(<Harness multiOutput initialPaths={["C:/docs/sample-1.pdf"]} />);
    await waitFor(() => expect(screen.getByTestId("dir").textContent).toBe("C:/docs/sample-1_images"));
    expect(screen.getByTestId("overwrite").textContent).toBe("unique_name");
    expect(screen.getByTestId("path").textContent).toBe("C:/docs/sample-1_images/sample-1_images.pdf");
    expect(
      invoke.mock.calls.some(
        ([command, payload]) =>
          command === "ensure_dir" && (payload as { path: string }).path === "C:/docs/sample-1_images",
      ),
    ).toBe(true);
  });

  it("keeps the overwrite prompt for single-output tools", async () => {
    render(<Harness multiOutput={false} initialPaths={["C:/docs/sample-1.pdf"]} />);
    await waitFor(() => expect(screen.getByTestId("dir").textContent).toBe("C:/docs"));
    expect(screen.getByTestId("overwrite").textContent).toBe("error");
  });

  it("accepts office documents for the AI assistant and rejects unknown types", async () => {
    render(<Harness multiOutput={false} accept="document" initialPaths={["C:/docs/report.docx"]} />);
    await waitFor(() => expect(screen.getByTestId("count").textContent).toBe("1"));

    render(<Harness multiOutput={false} accept="document" initialPaths={["C:/docs/archive.zip"]} />);
    await waitFor(() => expect(screen.getAllByTestId("count")[1].textContent).toBe("0"));
  });

  it("stages Android output in the app folder and creates it", async () => {
    const userAgent = navigator.userAgent;
    Object.defineProperty(navigator, "userAgent", { value: "Mozilla/5.0 (Linux; Android 14)", configurable: true });
    // `isAndroid()` caches its answer per module instance, so the UA must be in
    // place before the hook's module graph is loaded.
    vi.resetModules();
    try {
      const { useTool: useToolAndroid } = await import("./useTool");
      function AndroidHarness() {
        const session = useToolAndroid({
          suffix: "_merged",
          accept: "pdf",
          loadInfo: false,
          initialPaths: ["/cache/imports/a.pdf"],
        });
        return (
          <div>
            <span data-testid="android-dir">{session.outputDir}</span>
          </div>
        );
      }
      render(<AndroidHarness />);
      await waitFor(() => expect(screen.getByTestId("android-dir").textContent).toBe("/data/Outputs"));
      expect(appDataDir).toHaveBeenCalled();
      expect(
        invoke.mock.calls.some(
          ([command, payload]) => command === "ensure_dir" && (payload as { path: string }).path === "/data/Outputs",
        ),
      ).toBe(true);
    } finally {
      Object.defineProperty(navigator, "userAgent", { value: userAgent, configurable: true });
      vi.resetModules();
    }
  });
});
