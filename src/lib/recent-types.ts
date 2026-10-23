/** What a caller stores; `path` may be a token or a real path. */
export interface RecentAdd {
  path: string;
  fileName: string;
  tool: string;
  timestamp: number;
}

/** What the Rust store returns: an opaque read token, never a path. */
export interface RecentEntry {
  token: string;
  fileName: string;
  tool: string;
  timestamp: number;
}
