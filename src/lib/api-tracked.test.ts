import { readFileSync } from "node:fs";
import path from "node:path";
import { beforeEach, describe, expect, it, vi } from "vitest";

// The long jobs record a retry entry with the command name and `{ request }`
// before the generated wrapper runs; the Jobs screen later replays exactly
// that with `invoke`. Both must name the same command (3.4).
const invokeMock = vi.fn(async (_command: string, _args?: unknown) => ({}) as unknown);
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invokeMock(...(args as [string])) }));
const trackMock = vi.fn();
vi.mock("./job-retries", () => ({ trackJobInvocation: (...args: unknown[]) => trackMock(...args) }));

import * as api from "./api";

type Case = [name: string, command: string, run: (request: never) => Promise<unknown>];

const CASES: Case[] = [
  ["aiSummarize", "ai_summarize", api.aiSummarize],
  ["aiTranslate", "ai_translate", api.aiTranslate],
  ["aiAsk", "ai_ask", api.aiAsk],
  ["aiChat", "ai_chat", api.aiChat],
  ["aiExtractTable", "ai_extract_table", api.aiExtractTable],
  ["aiBatchRun", "ai_batch_run", api.aiBatchRun],
  ["aiCleanupText", "ai_cleanup_text", api.aiCleanupText],
  ["vaultScan", "vault_scan", api.vaultScan],
];

describe("tracked jobs use the typed wrappers (3.4)", () => {
  beforeEach(() => {
    invokeMock.mockClear();
    trackMock.mockClear();
  });

  it.each(CASES)("%s records and invokes %s with the same { request }", async (_name, command, run) => {
    const request = { jobId: "job-1", marker: command };
    await run(request as never);
    expect(trackMock).toHaveBeenCalledWith(command, { request });
    expect(invokeMock).toHaveBeenCalledWith(command, { request });
  });

  it("builds the metadata request before recording it", async () => {
    await api.aiSuggestMetadata("C:/a.pdf", undefined, "job-2");
    const request = { path: "C:/a.pdf", password: null, jobId: "job-2", consent: true };
    expect(trackMock).toHaveBeenCalledWith("ai_suggest_metadata", { request });
    expect(invokeMock).toHaveBeenCalledWith("ai_suggest_metadata", { request });
  });
});

describe("every trackedRequest names its wrapper's command (3.4)", () => {
  const read = (file: string) => readFileSync(path.resolve(__dirname, "..", "..", file), "utf8");
  const generated = read("src/lib/generated/specta.ts");
  /** Each `trackedRequest(...)` call, read to its closing parenthesis. */
  const callsIn = (file: string) => {
    const text = read(file);
    const found: { file: string; command: string; wrapper: string }[] = [];
    for (let start = text.indexOf("trackedRequest("); start >= 0; start = text.indexOf("trackedRequest(", start + 1)) {
      let depth = 0;
      let end = start + "trackedRequest".length;
      for (; end < text.length; end += 1) {
        if (text[end] === "(") depth += 1;
        if (text[end] === ")" && --depth === 0) break;
      }
      const body = text.slice(start, end + 1);
      const command = /^trackedRequest\(\s*"([a-z0-9_]+)"/.exec(body)?.[1];
      const wrapper = /\(c, r\) =>\s*c\.([A-Za-z0-9]+)\(r\),?\s*\)$/.exec(body)?.[1];
      if (command && wrapper) found.push({ file, command, wrapper });
    }
    return found;
  };
  const calls = ["src/lib/api.ts", "src/screens/PdfStudio.tsx"].flatMap(callsIn);

  it("finds every tracked job", () => {
    expect(calls.length).toBe(39);
  });

  it.each(calls.map((call) => [call.command, call.wrapper, call.file]))(
    "%s goes through %s, which invokes that command",
    (command, wrapper) => {
      expect(generated).toMatch(
        new RegExp(`\\b${wrapper}: \\([^)]*\\) => [^\\n]*__TAURI_INVOKE(?:<[^>]*>)?\\("${command}"`),
      );
    },
  );
});
