import { readdirSync, readFileSync, statSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";

/**
 * The IPC surface contract (v6.0.1 audit, B2).
 *
 * A screen that calls `invoke("name")` for a command that does not exist only
 * fails at runtime, in the packaged app, with a tauri error nobody sees in
 * review. This test reads the Rust command attributes and the frontend
 * `invoke`/`invokeTracked` calls and fails when a name has no Rust
 * counterpart, or when two commands share a name (Tauri registers by name, so
 * the second one would shadow the first).
 */

const root = path.resolve(__dirname, "..", "..");

function walk(directory: string, accept: (file: string) => boolean): string[] {
  const files: string[] = [];
  for (const entry of readdirSync(directory)) {
    if (entry === "node_modules" || entry === "target" || entry === "dist" || entry.startsWith(".")) continue;
    const absolute = path.join(directory, entry);
    if (statSync(absolute).isDirectory()) {
      files.push(...walk(absolute, accept));
    } else if (accept(absolute)) {
      files.push(absolute);
    }
  }
  return files;
}

function rustCommands(): { name: string; cfg: string }[] {
  const files = walk(path.join(root, "src-tauri", "src"), (file) => file.endsWith(".rs"));
  const commands: { name: string; cfg: string }[] = [];
  // The cfg attribute directly above a command is part of its identity:
  // `android_take_pending_open` has an Android implementation and a desktop
  // stub, which is not a duplicate.
  const pattern =
    /(?:#\[cfg\(([^)]*)\)\]\s*)?#\[tauri::command(?:\([^)]*\))?\]\s*(?:#\[[^\]]*\]\s*)*pub\s+(?:async\s+)?fn\s+([A-Za-z0-9_]+)/g;
  for (const file of files) {
    const text = readFileSync(file, "utf8");
    for (const match of text.matchAll(pattern)) {
      commands.push({ name: match[2], cfg: (match[1] ?? "").replace(/\s+/g, "") });
    }
  }
  return commands;
}

function tsInvocations(): Map<string, string> {
  const files = walk(path.join(root, "src"), (file) => /\.(ts|tsx)$/.test(file) && !/\.test\.(ts|tsx)$/.test(file));
  const calls = new Map<string, string>();
  for (const file of files) {
    const text = readFileSync(file, "utf8");
    for (const match of text.matchAll(/\binvoke(?:Tracked)?\b/g)) {
      let cursor = (match.index ?? 0) + match[0].length;
      while (cursor < text.length && /\s/.test(text[cursor])) cursor += 1;
      // Optional generic argument, with balanced angle brackets.
      if (text[cursor] === "<") {
        let depth = 0;
        while (cursor < text.length) {
          if (text[cursor] === "<") depth += 1;
          if (text[cursor] === ">") {
            depth -= 1;
            if (depth === 0) {
              cursor += 1;
              break;
            }
          }
          cursor += 1;
        }
        while (cursor < text.length && /\s/.test(text[cursor])) cursor += 1;
      }
      if (text[cursor] !== "(") continue;
      cursor += 1;
      while (cursor < text.length && /\s/.test(text[cursor])) cursor += 1;
      const quote = text[cursor];
      if (quote !== '"' && quote !== "'") continue;
      cursor += 1;
      const start = cursor;
      while (cursor < text.length && text[cursor] !== quote) cursor += 1;
      const name = text.slice(start, cursor);
      if (/^[a-z0-9_]+$/.test(name) && !calls.has(name)) {
        calls.set(name, path.relative(root, file));
      }
    }
  }
  return calls;
}

describe("IPC surface", () => {
  const registered = rustCommands();
  const invoked = tsInvocations();

  it("registers a realistic number of commands", () => {
    expect(registered.length).toBeGreaterThan(150);
    expect(invoked.size).toBeGreaterThan(100);
  });

  it("never invokes a command that does not exist", () => {
    const names = new Set(registered.map((entry) => entry.name));
    const missing = [...invoked.entries()].filter(([name]) => !names.has(name));
    expect(missing, `unknown commands: ${JSON.stringify(missing)}`).toEqual([]);
  });

  it("never registers the same command name twice for the same platform", () => {
    const seen = new Set<string>();
    const duplicates = new Set<string>();
    for (const entry of registered) {
      const key = `${entry.name}#${entry.cfg}`;
      if (seen.has(key)) duplicates.add(`${entry.name} (${entry.cfg || "all platforms"})`);
      seen.add(key);
    }
    expect([...duplicates], "duplicate #[tauri::command] names").toEqual([]);
  });
});
