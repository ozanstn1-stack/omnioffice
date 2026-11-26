import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.fn(async (command: string) => {
  switch (command) {
    case "load_operations":
      return [{ id: "1" }, { id: "2" }];
    case "clear_operations":
    case "jobs_clear_finished":
    case "clear_recent":
      return null;
    case "ai_clear_data":
      return { chatsDeleted: 1, indexDeleted: true };
    default:
      return null;
  }
});

vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invoke(...(args as [string])) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));

import { StorageCleanupCard } from "./storage-cleanup-card";
import { useJobs } from "../lib/jobs";
import { useRecent, useSettings, useToasts } from "../lib/store";
import { DEFAULT_SETTINGS } from "../lib/types";
import { WORKBENCH_PREFS_KEY } from "../lib/workbench-store";

describe("StorageCleanupCard (8.3.0)", () => {
  beforeEach(() => {
    invoke.mockClear();
    useSettings.setState({ settings: { ...DEFAULT_SETTINGS }, loaded: true });
    useRecent.setState({
      entries: [{ token: "tok-1", fileName: "a.pdf", tool: "reader", timestamp: 1732615200000 }],
    });
    useJobs.setState({
      jobs: [
        {
          id: "done-1",
          kind: "merge",
          title: "Merge",
          status: "succeeded",
          stage: "done",
          current: 1,
          total: 1,
          startedAt: 1,
        },
      ],
    });
    useToasts.setState({ toasts: [] });
    window.localStorage.setItem(WORKBENCH_PREFS_KEY, '{"splitRatio":0.4}');
  });

  it("summarizes storage counts and cleans finished jobs, logs, and optional data", async () => {
    const user = userEvent.setup();
    render(<StorageCleanupCard />);

    await screen.findByText(/1 finished jobs · 2 log entries · 1 recent files/i);

    await user.click(screen.getByRole("checkbox", { name: /Also clear recent files list/i }));
    await user.click(screen.getByRole("button", { name: /Clean unnecessary storage/i }));

    await waitFor(() => expect(invoke).toHaveBeenCalledWith("clear_operations"));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("clear_recent"));
    expect(window.localStorage.getItem(WORKBENCH_PREFS_KEY)).toBeNull();
    expect(useJobs.getState().jobs).toHaveLength(0);
    expect(useRecent.getState().entries).toHaveLength(0);
    expect(useToasts.getState().toasts.some((toast) => /4 categories cleared/i.test(toast.title))).toBe(true);
  });
});
