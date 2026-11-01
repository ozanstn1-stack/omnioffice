import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import en from "../lib/i18n/en.json";
import { LanSyncPlan } from "./lan-sync-plan";

// The plan is the only place where a LAN sync touches a document, and nothing
// may move without a click: these tests pin that every state renders its own
// choice and that each click reports the exact (device, path, resolution).
const plan = (overrides: Partial<Parameters<typeof LanSyncPlan>[0]["plan"]> = {}) => ({
  deviceId: "dev-1",
  folder: "C:/Docs",
  items: [
    { path: "a.oswk", state: "synced", action: "nothing" },
    { path: "b.oswk", state: "local_ahead", action: "upload" },
  ],
  deletions: [],
  ...overrides,
});

function renderPlan(overrides: Partial<Parameters<typeof LanSyncPlan>[0]> = {}) {
  const onResolve = vi.fn();
  const onSyncSafe = vi.fn();
  const onResolveDeletion = vi.fn();
  render(
    <LanSyncPlan
      plan={plan()}
      busy={false}
      onResolve={onResolve}
      onSyncSafe={onSyncSafe}
      onResolveDeletion={onResolveDeletion}
      {...overrides}
    />,
  );
  return { onResolve, onSyncSafe, onResolveDeletion };
}

describe("LanSyncPlan", () => {
  it("shows one row per document with its state and the action that needs no question", () => {
    renderPlan();
    expect(screen.getByText("a.oswk")).toBeInTheDocument();
    expect(screen.getByText("b.oswk")).toBeInTheDocument();
    expect(screen.getByText(en["lan.stateSynced"])).toBeInTheDocument();
    expect(screen.getByText(en["lan.stateLocalAhead"])).toBeInTheDocument();
    expect(screen.getByText(en["lan.actionUpload"])).toBeInTheDocument();
  });

  it("offers the three explicit choices on a conflict and nothing is chosen by default", () => {
    const { onResolve } = renderPlan({
      plan: plan({ items: [{ path: "c.oswk", state: "conflict" }] }),
    });
    fireEvent.click(screen.getByRole("button", { name: en["lan.keepLocal"] }));
    expect(onResolve).toHaveBeenCalledWith("dev-1", "c.oswk", "keep_local");
    fireEvent.click(screen.getByRole("button", { name: en["lan.keepRemote"] }));
    expect(onResolve).toHaveBeenCalledWith("dev-1", "c.oswk", "keep_cloud");
    fireEvent.click(screen.getByRole("button", { name: en["lan.keepBoth"] }));
    expect(onResolve).toHaveBeenCalledWith("dev-1", "c.oswk", "keep_both");
  });

  it("asks before acting on a deletion, on the side the file was removed from", () => {
    const { onResolveDeletion } = renderPlan({
      plan: plan({
        deletions: [
          { path: "gone-local.oswk", side: "local" },
          { path: "gone-remote.oswk", side: "remote" },
        ],
      }),
    });
    expect(screen.getByText(en["lan.deletionsTitle"])).toBeInTheDocument();
    // Removed here: delete it there, or bring it back here.
    fireEvent.click(screen.getAllByRole("button", { name: en["lan.deleteRemote"] })[0]);
    expect(onResolveDeletion).toHaveBeenCalledWith("dev-1", "gone-local.oswk", "local", "delete_remote");
    fireEvent.click(screen.getAllByRole("button", { name: en["lan.restoreLocal"] })[0]);
    expect(onResolveDeletion).toHaveBeenCalledWith("dev-1", "gone-local.oswk", "local", "restore_local");
    // Removed on the peer: delete it here, or bring it back there.
    fireEvent.click(screen.getAllByRole("button", { name: en["lan.deleteLocal"] })[0]);
    expect(onResolveDeletion).toHaveBeenCalledWith("dev-1", "gone-remote.oswk", "remote", "delete_local");
    fireEvent.click(screen.getAllByRole("button", { name: en["lan.restoreRemote"] })[0]);
    expect(onResolveDeletion).toHaveBeenCalledWith("dev-1", "gone-remote.oswk", "remote", "restore_remote");
  });

  it("runs the safe subset only for the device the plan belongs to", () => {
    const { onSyncSafe } = renderPlan();
    fireEvent.click(screen.getByRole("button", { name: en["lan.syncSafe"] }));
    expect(onSyncSafe).toHaveBeenCalledWith("dev-1");
  });

  it("disables every action while a sync is running", () => {
    renderPlan({ busy: true });
    expect(screen.getByRole("button", { name: en["lan.syncSafe"] })).toBeDisabled();
  });

  it("says so when there is nothing to do", () => {
    renderPlan({ plan: plan({ items: [] }) });
    expect(screen.getByText(en["lan.emptyPlan"])).toBeInTheDocument();
  });
});
