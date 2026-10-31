#!/usr/bin/env node
/**
 * The command ACL manifest (E1/4b).
 *
 * Tauri only enforces an application command ACL once the app declares an ACL
 * manifest: from that moment every command the webview invokes must be allowed
 * by a capability, and anything else is refused before it runs. The manifest is
 * generated here from `src-tauri/src/ipc.rs` - the single list of registered
 * commands - so the ACL cannot drift from the surface it guards:
 *
 *   node scripts/command-acl.mjs           # write src-tauri/permissions/*.toml
 *   node scripts/command-acl.mjs --check   # fail when they are stale
 *
 * One permission per IPC domain (`allow-app-commands`, `allow-pdf-commands`,
 * ...): the domain split is the reviewable unit, and a new command only has to
 * land in its domain list in `ipc.rs`.
 *
 * `--check` also proves the capabilities actually grant the generated
 * permissions to the main window: a permission nobody references would deny the
 * whole domain at runtime.
 */
import { readFileSync, readdirSync, writeFileSync, existsSync, mkdirSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const sourcePath = join(root, "src-tauri", "src", "ipc.rs");
const permissionsDir = join(root, "src-tauri", "permissions");
const capabilitiesDir = join(root, "src-tauri", "capabilities");

const HEADER = `# Generated from src-tauri/src/ipc.rs by \`npm run acl:sync\` - do not edit by hand.
# One permission per IPC domain: Tauri refuses any command a capability does not
# allow once an application ACL manifest exists, so this file is the whole
# permission for the commands below.`;

/** The IPC domains and their commands, in source order. */
export function readDomains(source = readFileSync(sourcePath, "utf8")) {
  const domains = [];
  const lines = source.split("\n");
  let depth = 0;
  let current = null;
  let pendingDoc = [];
  for (const line of lines) {
    if (depth === 0) {
      if (/^domain!\s*\{\s*$/.test(line)) {
        depth = 1;
        current = null;
        pendingDoc = [];
      }
      continue;
    }
    // Inside a `domain! { ... }` block.
    const withoutComment = line.replace(/\/\/.*$/, "");
    const opens = (withoutComment.match(/\{/g) ?? []).length;
    const closes = (withoutComment.match(/\}/g) ?? []).length;
    if (!current) {
      const doc = /^\s*\/\/\/\s?(.*)$/.exec(line);
      if (doc) {
        pendingDoc.push(doc[1].trim());
      }
      const name = /^\s*([a-z][a-z0-9_]*)\s*\{\s*$/.exec(withoutComment);
      if (name) {
        current = { name: name[1], description: pendingDoc.join(" ").trim(), commands: [] };
        domains.push(current);
      }
    } else {
      const entry = /^\s*(?:#\[[^\]]*\]\s*)?([a-z][a-z0-9_]*)::([a-z][a-z0-9_]*)\s*,/.exec(line);
      if (entry) {
        current.commands.push(entry[2]);
      }
    }
    depth += opens - closes;
    if (depth <= 0) {
      depth = 0;
      current = null;
    }
  }
  return domains;
}

/** `app_commands` -> `allow-app-commands`. */
function permissionIdentifier(domain) {
  return `allow-${domain.replace(/_/g, "-")}`;
}

/** The TOML body for one domain, byte-stable so `--check` compares text. */
function permissionFile(domain) {
  const identifier = permissionIdentifier(domain.name);
  const description = domain.description || `The ${domain.name.replace(/_/g, " ")} commands.`;
  const commands = domain.commands.map((command) => `  "${command}",`).join("\n");
  return `${HEADER}\n\n[[permission]]\nidentifier = "${identifier}"\ndescription = "${description.replace(/"/g, '\\"')}"\ncommands.allow = [\n${commands}\n]\n`;
}

/** Every app permission identifier a capability grants to the main window. */
function grantedIdentifiers() {
  const granted = new Set();
  if (!existsSync(capabilitiesDir)) return granted;
  for (const entry of readdirSync(capabilitiesDir)) {
    if (!entry.endsWith(".json")) continue;
    let capability;
    try {
      capability = JSON.parse(readFileSync(join(capabilitiesDir, entry), "utf8"));
    } catch (error) {
      throw new Error(`capabilities/${entry} is not valid JSON: ${error.message}`);
    }
    // A capability with a `platforms` filter only applies to those targets; the
    // ACL must hold on every platform, so only unconditional ones count here.
    if (capability.platforms) continue;
    if (!Array.isArray(capability.permissions)) continue;
    for (const permission of capability.permissions) {
      if (typeof permission === "string") granted.add(permission);
      else if (permission && typeof permission.identifier === "string") granted.add(permission.identifier);
    }
  }
  return granted;
}

function build() {
  const domains = readDomains();
  if (domains.length === 0) throw new Error("no IPC domains found in src-tauri/src/ipc.rs");
  const files = new Map();
  for (const domain of domains) {
    if (domain.commands.length === 0) throw new Error(`domain ${domain.name} has no commands`);
    files.set(`${domain.name.replace(/_/g, "-")}.toml`, permissionFile(domain));
  }
  return files;
}

function main() {
  const check = process.argv.includes("--check");
  const files = build();
  const granted = grantedIdentifiers();

  if (!check) {
    mkdirSync(permissionsDir, { recursive: true });
    for (const [name, content] of files) {
      const path = join(permissionsDir, name);
      if (existsSync(path) && readFileSync(path, "utf8") === content) continue;
      writeFileSync(path, content);
      console.log(`acl:sync: wrote ${relative(root, path)}`);
    }
    const stale = existsSync(permissionsDir)
      ? readdirSync(permissionsDir).filter((name) => name.endsWith(".toml") && !files.has(name))
      : [];
    for (const name of stale) {
      console.error(`acl:sync: ${name} has no IPC domain any more; delete src-tauri/permissions/${name}`);
    }
    if (stale.length > 0) process.exit(1);
  }

  const problems = [];
  for (const [name, content] of files) {
    const path = join(permissionsDir, name);
    if (!existsSync(path)) problems.push(`src-tauri/permissions/${name} is missing (run npm run acl:sync)`);
    else if (readFileSync(path, "utf8") !== content) {
      problems.push(`src-tauri/permissions/${name} is stale (run npm run acl:sync)`);
    }
    const identifier = permissionIdentifier(name.replace(/\.toml$/, "").replace(/-/g, "_"));
    if (!granted.has(identifier)) {
      problems.push(`${identifier} is not granted by any unconditional capability (capabilities/*.json)`);
    }
  }
  if (existsSync(permissionsDir)) {
    for (const name of readdirSync(permissionsDir)) {
      if (name.endsWith(".toml") && !files.has(name)) problems.push(`src-tauri/permissions/${name} has no IPC domain`);
    }
  }
  if (problems.length > 0) {
    for (const problem of problems) console.error(`command ACL: ${problem}`);
    process.exit(1);
  }
  const commands = [...files.values()].reduce((sum, content) => sum + (content.match(/^ {2}"/gm) ?? []).length, 0);
  console.log(
    `Command ACL OK: ${files.size} domain permission(s), ${commands} command(s), all granted by a capability.`,
  );
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();
