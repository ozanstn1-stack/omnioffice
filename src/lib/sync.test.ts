import { describe, expect, it } from "vitest";
import { isOswkPath, needsResolution, shortHash, syncStateTone, SYNC_STATE_ORDER } from "./sync";
import type { SyncStateId } from "./api";

describe("sync presentation helpers", () => {
  it("maps every state to a badge tone and covers the enum once", () => {
    const tones: Record<SyncStateId, string> = {
      synced: syncStateTone("synced"),
      local_only: syncStateTone("local_only"),
      local_ahead: syncStateTone("local_ahead"),
      cloud_ahead: syncStateTone("cloud_ahead"),
      conflict: syncStateTone("conflict"),
    };
    expect(tones.conflict).toBe("danger");
    expect(tones.synced).toBe("ok");
    expect(new Set(SYNC_STATE_ORDER).size).toBe(5);
    for (const state of SYNC_STATE_ORDER) {
      expect(Object.keys(tones)).toContain(state);
    }
  });

  it("offers resolution only for divergent states", () => {
    expect(needsResolution("conflict")).toBe(true);
    expect(needsResolution("local_ahead")).toBe(true);
    expect(needsResolution("cloud_ahead")).toBe(true);
    expect(needsResolution("synced")).toBe(false);
    expect(needsResolution("local_only")).toBe(false);
    expect(needsResolution(null)).toBe(false);
  });

  it("tracks native documents only, case-insensitively", () => {
    expect(isOswkPath("C:\\docs\\Report.OSWK")).toBe(true);
    expect(isOswkPath("/home/me/report.oswk")).toBe(true);
    expect(isOswkPath("report.pdf")).toBe(false);
    expect(isOswkPath("report.oswk.bak")).toBe(false);
  });

  it("shortens hashes without hiding the fact that there is one", () => {
    expect(shortHash(null)).toBe("—");
    expect(shortHash("abcdef")).toBe("abcdef");
    expect(shortHash("1234567890abcdef")).toBe("12345678…");
  });
});
