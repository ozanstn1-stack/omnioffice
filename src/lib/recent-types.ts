import type * as specta from "./generated/specta";

/** What a caller stores; `path` may be a token or a real path. */
export type RecentAdd = specta.RecentEntry;

/** What the Rust store returns: an opaque read token, never a path. */
export type RecentEntry = specta.RecentRef;
