/**
 * LAN sync (C14): typed wrappers around the same-network commands.
 *
 * The Rust side owns the identity, the sealed store and the pinned-TLS
 * transport; this module only carries the views across. Folder arguments are
 * capability tokens from `pickDirectoryToken` (a folder the user picked), never
 * paths the webview invents.
 */
import { call } from "./typed-call";
import type { LanApplyView, LanDeviceView, LanPlanView, LanStatusView } from "./generated/specta";

export type { LanApplyView, LanDeviceView, LanPlanView, LanStatusView };

/** The identity, whether sharing is on, and the peers on both sides. */
export const lanStatus = () => call((c) => c.lanStatus());

/** Renames this device on the network. */
export const lanRename = (name: string) => call((c) => c.lanRename(name));

/** Shares a picked folder and opens a six-digit pairing window. */
export const lanHostStart = (folder: string) => call((c) => c.lanHostStart(folder));

/** Opens a fresh pairing code on the running host. */
export const lanHostOpenPairing = () => call((c) => c.lanHostOpenPairing());

/** Stops sharing and closes the pairing window. */
export const lanHostStop = () => call((c) => c.lanHostStop());

/** Revokes a pairing on the host side. */
export const lanHostUnpair = (deviceId: string) => call((c) => c.lanHostUnpair(deviceId));

/** Broadcasts a discovery probe; returns the hosts that answered. */
export const lanDiscover = (timeoutMs = 1200) => call((c) => c.lanDiscover(timeoutMs));

/** Pairs with a host using its six-digit code. */
export const lanPair = (address: string, code: string) => call((c) => c.lanPair(address, code));

/** Forgets a pairing on this side. */
export const lanUnpair = (deviceId: string) => call((c) => c.lanUnpair(deviceId));

/** Compares the picked folder with a paired peer's folder. */
export const lanPlan = (deviceId: string, folder: string) => call((c) => c.lanPlan(deviceId, folder));

/** Applies one plan item; a conflict needs an explicit resolution. */
export const lanApply = (
  deviceId: string,
  folder: string,
  path: string,
  resolution?: "keep_local" | "keep_cloud" | "keep_both",
) => call((c) => c.lanApply(deviceId, folder, path, resolution ?? null));

/** Confirms one deletion; `side` is `local` or `remote`, `resolution` one of
 * `delete_remote`, `restore_local`, `delete_local`, `restore_remote`. */
export const lanResolveDeletion = (
  deviceId: string,
  folder: string,
  path: string,
  side: "local" | "remote",
  resolution: "delete_remote" | "restore_local" | "delete_local" | "restore_remote",
) => call((c) => c.lanResolveDeletion(deviceId, folder, path, side, resolution));

/** Applies every change that needs no question; conflicts stay for the user. */
export const lanSyncSafe = (deviceId: string, folder: string) => call((c) => c.lanSyncSafe(deviceId, folder));
