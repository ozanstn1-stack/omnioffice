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
