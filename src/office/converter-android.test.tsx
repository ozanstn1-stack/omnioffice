import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

/**
 * Android converter regression: the screen used the desktop dialog plugin,
 * which cannot open the Android picker. It must use the SAF bridge, stage the
 * result in the app cache and publish each finished file to a visible place.
 */
const invoke = vi.fn(async (_command: string, _payload?: unknown) => null);
const conversionTargets = vi.fn(async () => ["pdf", "docx"]);
const convertFile = vi.fn(async (input: string, output: string, options?: Record<string, unknown>) => ({
  input,
  output,
  converted: true,
  warnings: [],
  options,
}));
const publishOutputs = vi.fn(async () => []);
const pickOfficeFiles = vi.fn(async () => ["/cache/imports/doc.docx"]);

vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...(args as [string])) }));
vi.mock("@tauri-apps/api/path", () => ({
  appCacheDir: vi.fn(async () => "/cache"),
  join: vi.fn(async (...parts: string[]) => parts.join("/")),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(async () => null),
  save: vi.fn(async () => null),
}));
vi.mock("@tauri-apps/plugin-fs", () => ({
  readFile: vi.fn(async () => new Uint8Array()),
  writeFile: vi.fn(async () => undefined),
}));
vi.mock("../lib/mobile", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../lib/mobile")>()),
  isAndroid: () => true,
  pickOfficeFiles: (...args: unknown[]) => pickOfficeFiles(...(args as [])),
  pickAndroidFolder: vi.fn(async () => null),
  publishOutputs: (...args: unknown[]) => publishOutputs(...(args as [])),
}));
vi.mock("../lib/office-api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../lib/office-api")>()),
  conversionTargets: (...args: unknown[]) => conversionTargets(...(args as [])),
  convertFile: (...args: unknown[]) => convertFile(...(args as [string, string, Record<string, unknown>?])),
  openDocument: vi.fn(async () => {
    throw new Error("not an office document");
  }),
  cleanDocument: vi.fn(async () => ({ bytesBefore: 1, bytesAfter: 1, actions: [], warnings: [] })),
  imageFootprint: vi.fn(async () => 0),
}));

import { ConverterScreen } from "./ToolsScreens";

describe("converter on Android", () => {
  beforeEach(() => {
    invoke.mockClear();
    conversionTargets.mockClear();
    convertFile.mockClear();
    publishOutputs.mockClear();
    pickOfficeFiles.mockClear();
  });

  it("picks through SAF, stages the output and publishes it", async () => {
    const user = userEvent.setup();
    render(<ConverterScreen />);
    await user.click(screen.getByRole("button", { name: /choose files/i }));
    await waitFor(() => expect(conversionTargets).toHaveBeenCalledWith("docx"));

    await user.click(screen.getByRole("button", { name: /^convert$/i }));
    await waitFor(() => expect(convertFile).toHaveBeenCalled());
    const [input, output] = convertFile.mock.calls[0] as unknown as [string, string];
    expect(input).toBe("/cache/imports/doc.docx");
    expect(output.startsWith("/cache/converts/")).toBe(true);
    expect(output.endsWith(".pdf")).toBe(true);
    await waitFor(() => expect(publishOutputs).toHaveBeenCalledWith([output], undefined));
  });

  it("sends the PDF password in the conversion options", async () => {
    pickOfficeFiles.mockResolvedValueOnce(["/cache/imports/secret.pdf"]);
    const user = userEvent.setup();
    render(<ConverterScreen />);
    await user.click(screen.getByRole("button", { name: /choose files/i }));

    await user.type(await screen.findByPlaceholderText(/pdf password/i), "s3cret");
    await user.click(screen.getByRole("button", { name: /^convert$/i }));
    await waitFor(() => expect(convertFile).toHaveBeenCalled());

    const [input, , options] = convertFile.mock.calls[0] as unknown as [string, string, Record<string, unknown>];
    expect(input).toBe("/cache/imports/secret.pdf");
    expect(options).toEqual({ password: "s3cret" });
  });

  it("shows the table recovery hint for a PDF to DOCX target", async () => {
    pickOfficeFiles.mockResolvedValueOnce(["/cache/imports/report.pdf"]);
    const user = userEvent.setup();
    render(<ConverterScreen />);
    await user.click(screen.getByRole("button", { name: /choose files/i }));

    await user.selectOptions(await screen.findByRole("combobox"), "docx");
    expect(screen.getByText(/recovered as tables/i)).toBeInTheDocument();
  });

  it("shows the backend password error on the file row", async () => {
    pickOfficeFiles.mockResolvedValueOnce(["/cache/imports/locked.pdf"]);
    convertFile.mockRejectedValueOnce({ code: "wrong_password", message: "The PDF password is incorrect." });
    const user = userEvent.setup();
    render(<ConverterScreen />);
    await user.click(screen.getByRole("button", { name: /choose files/i }));

    await user.type(await screen.findByPlaceholderText(/pdf password/i), "nope");
    await user.click(screen.getByRole("button", { name: /^convert$/i }));
    expect(await screen.findByText("The PDF password is incorrect.")).toBeInTheDocument();
  });
});
