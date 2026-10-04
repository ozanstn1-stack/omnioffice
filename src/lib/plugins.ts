/**
 * Plugin runtime: manifests, permission host and the sandboxed Web Worker.
 *
 * Plugin code never runs in the page. It is loaded as text, wrapped in a thin
 * bootstrap and executed inside a Web Worker created from a Blob. The worker
 * has no DOM, no Tauri IPC and (through the app CSP) no network: every
 * capability it wants crosses the RPC bridge in this file, where the host
 * checks the manifest permissions before touching any data.
 *
 * Threat model: this protects documents and user data from buggy or malicious
 * plugins through the capability API. Worker isolation is same-process and
 * same-engine - it is not an escape hatch from the browser engine itself.
 */
import { invoke } from "@tauri-apps/api/core";
import { create } from "zustand";
import packageJson from "../../package.json";
import { registerCommand, unregisterCommand } from "./commands";
import { makeTranslate } from "./i18n";
import { useOfficeTabs, type OfficeModel } from "./office-store";
import {
  isDeletedRun,
  type Block,
  type CellValue,
  type Deck,
  type OfficeKind,
  type Run,
  type SlideObject,
  type TextDocument,
  type Workbook,
} from "./office-types";
import { useSettings, useToasts } from "./store";

export const PLUGIN_API_VERSION = 1;
export const PLUGIN_APP_VERSION = packageJson.version;
export const SAMPLE_PLUGIN_ID = "sample.word-counter";
export const MAX_PLUGIN_SOURCE_BYTES = 512 * 1024;
export const MAX_PLUGIN_FILE_BYTES = 512 * 1024;
export const MAX_PLUGIN_RESPONSE_BYTES = 1024 * 1024;
export const PLUGIN_COMMAND_TIMEOUT_MS = 30_000;
export const PLUGIN_RPC_TIMEOUT_MS = 15_000;

// ---------------------------------------------------------------------------
// Manifest
// ---------------------------------------------------------------------------

export const PLUGIN_PERMISSIONS = [
  "read_document",
  "modify_document",
  "read_files",
  "write_files",
  "clipboard",
  "network",
] as const;
export type PluginPermission = (typeof PLUGIN_PERMISSIONS)[number];

export const PLUGIN_CAPABILITIES = [
  "command",
  "document-read",
  "document-write",
  "files",
  "clipboard",
  "network",
  "ui",
  "log",
] as const;
export type PluginCapability = (typeof PLUGIN_CAPABILITIES)[number];

export interface PluginCommandSpec {
  id: string;
  title: string;
  description?: string;
}

export interface PluginManifest {
  id: string;
  name: string;
  version: string;
  apiVersion: number;
  compatibility: { app: string };
  permissions: PluginPermission[];
  capabilities: PluginCapability[];
  commands: PluginCommandSpec[];
}

export type ManifestResult = { ok: true; manifest: PluginManifest } | { ok: false; error: string };

const PLUGIN_ID_PATTERN = /^[a-z0-9](?:[a-z0-9._-]{0,62}[a-z0-9])?$/;
const SEMVER_PATTERN = /^(\d+)\.(\d+)\.(\d+)$/;

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

export function parseSemver(text: string): [number, number, number] | null {
  const match = SEMVER_PATTERN.exec(text.trim());
  if (!match) return null;
  return [Number(match[1]), Number(match[2]), Number(match[3])];
}

function compareSemver(a: [number, number, number], b: [number, number, number]): number {
  for (let index = 0; index < 3; index += 1) {
    if (a[index] !== b[index]) return a[index] < b[index] ? -1 : 1;
  }
  return 0;
}

/**
 * Supports the comparator forms plugins actually need: a bare version,
 * `>=`, `>`, `<=`, `<`, `=`, `^` and `~`, joined by commas (AND).
 */
export function satisfiesAppRange(version: string, range: string): boolean {
  const current = parseSemver(version);
  if (!current) return false;
  for (const raw of range.split(",")) {
    const token = raw.trim();
    if (!token || token === "*") continue;
    const match = /^(>=|<=|>|<|=|\^|~)?\s*(\d+\.\d+\.\d+)$/.exec(token);
    if (!match) return false;
    const target = parseSemver(match[2]);
    if (!target) return false;
    const operator = match[1] ?? "=";
    const comparison = compareSemver(current, target);
    if (operator === ">=" && comparison < 0) return false;
    if (operator === ">" && comparison <= 0) return false;
    if (operator === "<=" && comparison > 0) return false;
    if (operator === "<" && comparison >= 0) return false;
    if (operator === "=" && comparison !== 0) return false;
    if (operator === "^" && (comparison < 0 || current[0] !== target[0])) return false;
    if (operator === "~" && (comparison < 0 || current[0] !== target[0] || current[1] !== target[1])) return false;
  }
  return true;
}

function validateCommands(value: unknown): PluginCommandSpec[] | string {
  if (!Array.isArray(value)) return "commands must be an array.";
  const commands: PluginCommandSpec[] = [];
  const seen = new Set<string>();
  for (const entry of value) {
    if (!isRecord(entry)) return "each command must be an object.";
    const id = entry.id;
    const title = entry.title;
    if (typeof id !== "string" || !PLUGIN_ID_PATTERN.test(id)) return "a command id is invalid.";
    if (typeof title !== "string" || !title.trim() || title.length > 120) return "a command title is invalid.";
    if (entry.description !== undefined && (typeof entry.description !== "string" || entry.description.length > 400)) {
      return "a command description is invalid.";
    }
    if (seen.has(id)) return `duplicate command id: ${id}.`;
    seen.add(id);
    commands.push({ id, title, ...(entry.description !== undefined ? { description: entry.description } : {}) });
  }
  return commands;
}

/** Strict manifest validation: anything unknown is rejected, never ignored. */
export function validateManifest(value: unknown): ManifestResult {
  if (!isRecord(value)) return { ok: false, error: "The manifest must be a JSON object." };
  const { id, name, version, apiVersion, compatibility, permissions, capabilities, commands } = value;
  if (typeof id !== "string" || !PLUGIN_ID_PATTERN.test(id)) {
    return { ok: false, error: "The plugin id must be lowercase letters, digits, dots, dashes or underscores." };
  }
  if (typeof name !== "string" || !name.trim() || name.length > 80) {
    return { ok: false, error: "The plugin name is missing." };
  }
  if (typeof version !== "string" || !parseSemver(version)) {
    return { ok: false, error: "The plugin version must be semver (x.y.z)." };
  }
  if (apiVersion !== PLUGIN_API_VERSION) {
    return { ok: false, error: `Unsupported plugin API version (this app speaks ${PLUGIN_API_VERSION}).` };
  }
  if (!isRecord(compatibility) || typeof compatibility.app !== "string" || !compatibility.app.trim()) {
    return { ok: false, error: "compatibility.app must be a version range." };
  }
  if (!satisfiesAppRange(PLUGIN_APP_VERSION, compatibility.app)) {
    return { ok: false, error: `This plugin requires app ${compatibility.app}, this build is ${PLUGIN_APP_VERSION}.` };
  }
  if (!Array.isArray(permissions) || permissions.length === 0) {
    return { ok: false, error: "permissions must be a non-empty array." };
  }
  const granted: PluginPermission[] = [];
  const seenPermissions = new Set<string>();
  for (const permission of permissions) {
    if (typeof permission !== "string" || !(PLUGIN_PERMISSIONS as readonly string[]).includes(permission)) {
      return { ok: false, error: `Unknown permission: ${String(permission)}.` };
    }
    if (seenPermissions.has(permission)) return { ok: false, error: `Duplicate permission: ${permission}.` };
    seenPermissions.add(permission);
    granted.push(permission as PluginPermission);
  }
  if (!Array.isArray(capabilities)) return { ok: false, error: "capabilities must be an array." };
  const declared: PluginCapability[] = [];
  for (const capability of capabilities) {
    if (typeof capability !== "string" || !(PLUGIN_CAPABILITIES as readonly string[]).includes(capability)) {
      return { ok: false, error: `Unknown capability: ${String(capability)}.` };
    }
    if (!(declared as string[]).includes(capability)) declared.push(capability as PluginCapability);
  }
  const parsedCommands = validateCommands(commands);
  if (typeof parsedCommands === "string") return { ok: false, error: parsedCommands };
  if (declared.includes("command") && parsedCommands.length === 0) {
    return { ok: false, error: "A plugin that declares the command capability needs at least one command." };
  }
  return {
    ok: true,
    manifest: {
      id,
      name,
      version,
      apiVersion,
      compatibility: { app: compatibility.app },
      permissions: granted,
      capabilities: declared,
      commands: parsedCommands,
    },
  };
}

/** Rejects anything that must never reach a filesystem join. */
export function safePluginFileName(name: unknown): string | null {
  if (typeof name !== "string") return null;
  if (name.length === 0 || name.length > 128 || name.includes("..")) return null;
  if (!/^[A-Za-z0-9](?:[A-Za-z0-9._ -]{0,126}[A-Za-z0-9])?$/.test(name)) return null;
  return name;
}

// ---------------------------------------------------------------------------
// Transport (the only place Tauri is touched)
// ---------------------------------------------------------------------------

export interface PluginArchivedRecord {
  manifest: unknown;
  sourceBytes: number;
}

export interface PluginHttpRequest {
  url: string;
  method?: string;
  headers?: Record<string, string>;
  body?: string;
}

export interface PluginHttpResponse {
  status: number;
  body: string;
  truncated: boolean;
}

export interface PluginTransport {
  list: () => Promise<PluginArchivedRecord[]>;
  readSource: (id: string) => Promise<string>;
  install: (manifestJson: string, source: string) => Promise<PluginArchivedRecord>;
  /** Opens a native folder picker on the Rust side; resolves null on cancel. */
  installFromDialog: () => Promise<PluginArchivedRecord | null>;
  installSample: () => Promise<PluginArchivedRecord>;
  remove: (id: string) => Promise<void>;
  readFile: (pluginId: string, name: string) => Promise<string>;
  writeFile: (pluginId: string, name: string, text: string) => Promise<void>;
  httpRequest: (pluginId: string, request: PluginHttpRequest) => Promise<PluginHttpResponse>;
}

/**
 * The Rust commands never return filesystem paths; installs come from the
 * user's folder picker, and every file command stays inside the plugin's
 * sandbox directory.
 */
export const tauriPluginTransport: PluginTransport = {
  list: () => invoke<PluginArchivedRecord[]>("plugin_list"),
  readSource: (id) => invoke<string>("plugin_read_source", { id }),
  install: (manifestJson, source) => invoke<PluginArchivedRecord>("plugin_install", { manifestJson, source }),
  installFromDialog: () => invoke<PluginArchivedRecord | null>("plugin_install_from_dialog"),
  installSample: () => invoke<PluginArchivedRecord>("plugin_install_sample"),
  remove: (id) => invoke<void>("plugin_delete", { id }),
  readFile: (pluginId, name) => invoke<string>("plugin_file_read", { pluginId, name }),
  writeFile: (pluginId, name, text) => invoke<void>("plugin_file_write", { pluginId, name, text }),
  httpRequest: (pluginId, request) => invoke<PluginHttpResponse>("plugin_http_request", { pluginId, request }),
};

let activeTransport: PluginTransport = tauriPluginTransport;

/** Swaps the backend (tests). */
export function setPluginTransport(transport: PluginTransport): void {
  activeTransport = transport;
}

export function pluginTransport(): PluginTransport {
  return activeTransport;
}

// ---------------------------------------------------------------------------
// Document text / edits (host side, through the normal office store)
// ---------------------------------------------------------------------------

export interface PluginDocumentSnapshot {
  kind: OfficeKind;
  title: string;
  text: string;
  model: unknown;
}

export interface PluginTextEdit {
  find: string;
  replace: string;
}

export function hasActiveOfficeDocument(): boolean {
  const state = useOfficeTabs.getState();
  return state.tabs.some((tab) => tab.id === state.activeId);
}

export function readActiveDocument(): PluginDocumentSnapshot | null {
  const state = useOfficeTabs.getState();
  const tab = state.tabs.find((candidate) => candidate.id === state.activeId);
  if (!tab) return null;
  return { kind: tab.kind, title: tab.title, text: officeModelText(tab.model), model: tab.model };
}

export function officeModelText(model: OfficeModel): string {
  if ("blocks" in model) return writerText(model);
  if ("sheets" in model) return workbookText(model);
  return deckText(model);
}

function pushBlockText(lines: string[], block: Block): void {
  if (block.type === "paragraph") {
    const text = block.runs
      .filter((run) => !isDeletedRun(run))
      .map((run) => run.text)
      .join("");
    if (text) lines.push(text);
    return;
  }
  if (block.type === "table") {
    for (const row of block.table.rows) {
      const cells = row.cells.map((cell) => {
        const parts: string[] = [];
        for (const nested of cell.blocks) pushBlockText(parts, nested);
        return parts.join(" ");
      });
      lines.push(cells.join("\t"));
    }
    return;
  }
  if (block.type === "image" && block.caption) lines.push(block.caption);
  if (block.type === "toc") {
    for (const entry of block.entries) lines.push(entry.text);
  }
}

function writerText(document: TextDocument): string {
  const lines: string[] = [];
  for (const block of document.blocks) pushBlockText(lines, block);
  return lines.join("\n");
}

function cellValueText(value: CellValue): string {
  if (value.kind === "text") return value.value;
  if (value.kind === "number") return String(value.value);
  if (value.kind === "bool") return value.value ? "TRUE" : "FALSE";
  if (value.kind === "error") return value.value;
  return "";
}

function compareCellKeys(a: string, b: string): number {
  const parse = (key: string): [string, number] => {
    const match = /^([A-Z]+)(\d+)$/.exec(key);
    return match ? [match[1], Number(match[2])] : [key, 0];
  };
  const [columnA, rowA] = parse(a);
  const [columnB, rowB] = parse(b);
  if (columnA === columnB) return rowA - rowB;
  return columnA < columnB ? -1 : 1;
}

function workbookText(workbook: Workbook): string {
  const lines: string[] = [];
  for (const sheet of workbook.sheets) {
    lines.push(`# ${sheet.name}`);
    for (const key of Object.keys(sheet.cells).sort(compareCellKeys)) {
      const text = cellValueText(sheet.cells[key].value);
      if (text) lines.push(`${key}\t${text}`);
    }
  }
  return lines.join("\n");
}

function deckText(deck: Deck): string {
  const lines: string[] = [];
  deck.slides.forEach((slide, index) => {
    lines.push(`# Slide ${index + 1}`);
    const visit = (objects: SlideObject[]): void => {
      for (const object of objects) {
        if (object.text) {
          for (const paragraph of object.text.paragraphs) {
            if (paragraph.text) lines.push(paragraph.text);
          }
        }
        if (object.children?.length) visit(object.children);
      }
    };
    visit(slide.objects);
    if (slide.notes) lines.push(slide.notes);
  });
  return lines.join("\n");
}

/**
 * Applies edits through the normal store path (`useOfficeTabs.edit`), which
 * marks the tab dirty exactly like a manual edit. Edits match inside a single
 * run (Writer, Impress) or a single cell (Calc): a `find` that spans several
 * runs does not match. Importing `office-store` here keeps the store as the
 * single writer - plugins can never mutate a document bypassing it.
 */
export function applyDocumentEdits(edits: PluginTextEdit[]): number {
  const state = useOfficeTabs.getState();
  const tab = state.tabs.find((candidate) => candidate.id === state.activeId);
  if (!tab) throw new Error("No document is open.");
  const model = JSON.parse(JSON.stringify(tab.model)) as OfficeModel;
  let applied = 0;
  if ("blocks" in model) applied = applyWriterEdits(model, edits);
  else if ("sheets" in model) applied = applyWorkbookEdits(model, edits);
  else applied = applyDeckEdits(model, edits);
  if (applied > 0) useOfficeTabs.getState().edit(tab.id, () => model);
  return applied;
}

function applyRunEdits(runs: Run[], edits: PluginTextEdit[]): number {
  let count = 0;
  for (const run of runs) {
    if (isDeletedRun(run)) continue;
    for (const edit of edits) {
      const index = run.text.indexOf(edit.find);
      if (index < 0) continue;
      run.text = run.text.slice(0, index) + edit.replace + run.text.slice(index + edit.find.length);
      count += 1;
    }
  }
  return count;
}

function applyBlockEdits(block: Block, edits: PluginTextEdit[]): number {
  if (block.type === "paragraph") return applyRunEdits(block.runs, edits);
  if (block.type !== "table") return 0;
  let count = 0;
  for (const row of block.table.rows) {
    for (const cell of row.cells) {
      for (const nested of cell.blocks) count += applyBlockEdits(nested, edits);
    }
  }
  return count;
}

function applyWriterEdits(document: TextDocument, edits: PluginTextEdit[]): number {
  let count = 0;
  for (const block of document.blocks) count += applyBlockEdits(block, edits);
  return count;
}

function applyWorkbookEdits(workbook: Workbook, edits: PluginTextEdit[]): number {
  let count = 0;
  for (const sheet of workbook.sheets) {
    for (const cell of Object.values(sheet.cells)) {
      if (cell.value.kind !== "text") continue;
      for (const edit of edits) {
        const index = cell.value.value.indexOf(edit.find);
        if (index < 0) continue;
        cell.value.value =
          cell.value.value.slice(0, index) + edit.replace + cell.value.value.slice(index + edit.find.length);
        count += 1;
      }
    }
  }
  return count;
}

function applyDeckEdits(deck: Deck, edits: PluginTextEdit[]): number {
  let count = 0;
  const visit = (objects: SlideObject[]): void => {
    for (const object of objects) {
      if (object.text) {
        for (const paragraph of object.text.paragraphs) {
          for (const edit of edits) {
            const index = paragraph.text.indexOf(edit.find);
            if (index < 0) continue;
            paragraph.text =
              paragraph.text.slice(0, index) + edit.replace + paragraph.text.slice(index + edit.find.length);
            count += 1;
          }
        }
      }
      if (object.children?.length) visit(object.children);
    }
  };
  for (const slide of deck.slides) visit(slide.objects);
  return count;
}

// ---------------------------------------------------------------------------
// Host: the RPC surface exposed to the worker, permission-checked here
// ---------------------------------------------------------------------------

export const PLUGIN_HOST_METHODS: Readonly<Record<string, PluginPermission | null>> = Object.freeze({
  "doc.getText": "read_document",
  "doc.getModel": "read_document",
  "doc.applyEdits": "modify_document",
  "files.read": "read_files",
  "files.write": "write_files",
  "clipboard.read": "clipboard",
  "clipboard.write": "clipboard",
  "net.request": "network",
  "ui.notify": null,
  log: null,
});

export interface PluginClipboard {
  read: () => Promise<string>;
  write: (text: string) => Promise<void>;
}

export interface PluginHostOptions {
  transport: PluginTransport;
  readDocument?: () => PluginDocumentSnapshot | null;
  applyTextEdits?: (edits: PluginTextEdit[]) => number;
  clipboard?: PluginClipboard;
  notify?: (message: string) => void;
  log?: (message: string) => void;
}

function errorText(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/**
 * Main-thread implementation of every capability. Notes:
 * - each method is gated by the manifest permission (a plugin without the
 *   permission gets an error, never data);
 * - a command that throws rejects only that call, the worker stays alive;
 * - file access goes through the transport, which enforces the per-plugin
 *   sandbox directory in Rust as well (defense in depth).
 */
export class PluginHost {
  readonly manifest: PluginManifest;
  private readonly options: PluginHostOptions;

  constructor(manifest: PluginManifest, options: PluginHostOptions) {
    this.manifest = manifest;
    this.options = options;
  }

  hasPermission(permission: PluginPermission): boolean {
    return this.manifest.permissions.includes(permission);
  }

  private requirePermission(permission: PluginPermission): void {
    if (!this.hasPermission(permission)) {
      throw new Error(`Permission denied: the plugin does not declare "${permission}".`);
    }
  }

  private requireDocument(): PluginDocumentSnapshot {
    const snapshot = this.options.readDocument?.() ?? null;
    if (!snapshot) throw new Error("No document is open.");
    return snapshot;
  }

  async handle(method: string, params: Record<string, unknown>): Promise<unknown> {
    const permission = PLUGIN_HOST_METHODS[method];
    if (permission === undefined) throw new Error(`Unknown host method: ${method}.`);
    if (permission) this.requirePermission(permission);
    switch (method) {
      case "doc.getText": {
        const scope = params.scope;
        if (scope !== "document") throw new Error(`Unsupported scope: ${String(scope)}.`);
        return this.requireDocument().text;
      }
      case "doc.getModel": {
        const scope = params.scope;
        if (scope !== "document") throw new Error(`Unsupported scope: ${String(scope)}.`);
        return this.requireDocument().model;
      }
      case "doc.applyEdits": {
        const edits = this.parseEdits(params.edits);
        this.requireDocument();
        const apply = this.options.applyTextEdits;
        if (!apply) throw new Error("Editing is not available.");
        const applied = apply(edits);
        return { applied };
      }
      case "files.read": {
        const name = safePluginFileName(params.name);
        if (!name) throw new Error("Invalid file name.");
        return this.options.transport.readFile(this.manifest.id, name);
      }
      case "files.write": {
        const name = safePluginFileName(params.name);
        if (!name) throw new Error("Invalid file name.");
        const text = params.text;
        if (typeof text !== "string") throw new Error("The file content must be text.");
        if (text.length > MAX_PLUGIN_FILE_BYTES) throw new Error("The file content is too large (512 KB limit).");
        await this.options.transport.writeFile(this.manifest.id, name, text);
        return null;
      }
      case "clipboard.read": {
        const clipboard = this.options.clipboard;
        if (!clipboard) throw new Error("The clipboard is not available.");
        return clipboard.read();
      }
      case "clipboard.write": {
        const clipboard = this.options.clipboard;
        if (!clipboard) throw new Error("The clipboard is not available.");
        const text = params.text;
        if (typeof text !== "string") throw new Error("Clipboard content must be text.");
        await clipboard.write(text);
        return null;
      }
      case "net.request": {
        const request = this.parseHttpRequest(params.request);
        const response = await this.options.transport.httpRequest(this.manifest.id, request);
        if (typeof response?.status !== "number" || typeof response?.body !== "string") {
          throw new Error("The network proxy returned an invalid response.");
        }
        return {
          status: response.status,
          body: response.body.slice(0, MAX_PLUGIN_RESPONSE_BYTES),
          truncated: Boolean(response.truncated),
        };
      }
      case "ui.notify": {
        const message = params.message;
        if (typeof message !== "string" || !message.trim()) throw new Error("The notification is empty.");
        if (message.length > 4000) throw new Error("The notification is too long.");
        this.options.notify?.(message);
        return null;
      }
      case "log": {
        const message = params.message;
        if (typeof message !== "string") throw new Error("The log message must be text.");
        this.options.log?.(message.slice(0, 4000));
        return null;
      }
      default:
        throw new Error(`Unknown host method: ${method}.`);
    }
  }

  private parseEdits(value: unknown): PluginTextEdit[] {
    if (!Array.isArray(value) || value.length === 0 || value.length > 200) {
      throw new Error("applyEdits expects 1..200 edits.");
    }
    const edits: PluginTextEdit[] = [];
    for (const entry of value) {
      if (!isRecord(entry) || typeof entry.find !== "string" || typeof entry.replace !== "string") {
        throw new Error("Each edit needs find and replace strings.");
      }
      if (entry.find.length === 0 || entry.find.length > 1000 || entry.replace.length > 10_000) {
        throw new Error("An edit is out of bounds.");
      }
      edits.push({ find: entry.find, replace: entry.replace });
    }
    return edits;
  }

  private parseHttpRequest(value: unknown): PluginHttpRequest {
    if (!isRecord(value) || typeof value.url !== "string") throw new Error("net.request needs a url.");
    const method = value.method === undefined ? "GET" : String(value.method).toUpperCase();
    if (method !== "GET" && method !== "POST") throw new Error("Only GET and POST are allowed.");
    let url: URL;
    try {
      url = new URL(value.url);
    } catch {
      throw new Error("The URL is not valid.");
    }
    if (url.protocol !== "https:") {
      const host = url.hostname;
      const privateHost =
        host === "localhost" ||
        /^127\./.test(host) ||
        /^10\./.test(host) ||
        /^192\.168\./.test(host) ||
        /^172\.(1[6-9]|2\d|3[01])\./.test(host) ||
        host === "[::1]" ||
        host === "::1";
      if (url.protocol !== "http:" || !privateHost) {
        throw new Error("Only https URLs are allowed (http only for localhost/private addresses).");
      }
    }
    const headers: Record<string, string> = {};
    if (value.headers !== undefined) {
      if (!isRecord(value.headers)) throw new Error("Headers must be an object.");
      const names = Object.keys(value.headers);
      if (names.length > 32) throw new Error("Too many headers.");
      for (const name of names) {
        const header = value.headers[name];
        if (typeof header !== "string" || header.length > 2048) throw new Error("An HTTP header is invalid.");
        headers[name] = header;
      }
    }
    const body = value.body;
    if (body !== undefined && (typeof body !== "string" || body.length > MAX_PLUGIN_FILE_BYTES)) {
      throw new Error("The request body is invalid.");
    }
    return {
      url: url.toString(),
      method,
      ...(Object.keys(headers).length ? { headers } : {}),
      ...(typeof body === "string" ? { body } : {}),
    };
  }
}

// ---------------------------------------------------------------------------
// Worker bootstrap and runtime
// ---------------------------------------------------------------------------

export type PluginStatus = "disabled" | "idle" | "running" | "crashed";

export interface PluginWorkerLike {
  postMessage: (message: unknown) => void;
  terminate: () => void;
  onmessage: ((event: { data: unknown }) => void) | null;
  onerror: ((event: { message?: string }) => void) | null;
  onmessageerror: (() => void) | null;
}

export type PluginWorkerFactory = (source: string) => PluginWorkerLike;

/**
 * The bootstrap is deliberately thin: it only wires the RPC shapes
 * (`{ id, kind: "call", method, params }` out, `{ id, kind: "result"|"error" }`
 * in) and exposes `self.host.*`. All enforcement lives on this side.
 */
export const PLUGIN_WORKER_BOOTSTRAP = `"use strict";
(() => {
  var pending = new Map();
  var nextId = 1;
  function callHost(method, params) {
    return new Promise((resolve, reject) => {
      var id = nextId++;
      pending.set(id, { resolve: resolve, reject: reject });
      self.postMessage({ id: id, kind: "call", method: method, params: params || {} });
    });
  }
  self.host = {
    doc: {
      getText: (scope) => callHost("doc.getText", { scope: scope }),
      getModel: (scope) => callHost("doc.getModel", { scope: scope }),
      applyEdits: (edits) => callHost("doc.applyEdits", { edits: edits })
    },
    files: {
      read: (name) => callHost("files.read", { name: name }),
      write: (name, text) => callHost("files.write", { name: name, text: text })
    },
    clipboard: {
      read: () => callHost("clipboard.read", {}),
      write: (text) => callHost("clipboard.write", { text: text })
    },
    net: { request: (request) => callHost("net.request", { request: request }) },
    ui: { notify: (message) => callHost("ui.notify", { message: message }) },
    log: (message) => callHost("log", { message: message })
  };
  self.onPluginMessage = null;
  self.onmessage = (event) => {
    var message = event.data;
    if (!message || typeof message !== "object") return;
    if (message.kind === "result" || message.kind === "error") {
      var entry = pending.get(message.id);
      if (!entry) return;
      pending.delete(message.id);
      if (message.kind === "result") entry.resolve(message.value);
      else entry.reject(new Error(typeof message.error === "string" ? message.error : "Plugin call failed"));
      return;
    }
    if (message.kind === "call" && message.method === "command.run") {
      var handler = self.onPluginMessage;
      Promise.resolve()
        .then(() => (typeof handler === "function"
          ? handler({ kind: "command", command: message.params && message.params.command, params: (message.params && message.params.params) || {} })
          : undefined))
        .then((value) => self.postMessage({ id: message.id, kind: "result", value: value }))
        .catch((error) => self.postMessage({ id: message.id, kind: "error", error: error && error.message ? String(error.message) : String(error) }));
    }
  };
  // The sandbox is explicit: plugins talk to the host, never to the network or
  // the page directly. CSP already blocks these inside the worker; shadowing
  // turns a would-be silent CSP violation into a clear TypeError.
  ["fetch", "XMLHttpRequest", "WebSocket", "EventSource", "importScripts", "indexedDB", "caches"].forEach((name) => {
    try {
      Object.defineProperty(self, name, { value: undefined, writable: false, configurable: false });
    } catch (_) {}
  });
})();
`;

export function buildWorkerSource(code: string): string {
  return `${PLUGIN_WORKER_BOOTSTRAP}\n// --- plugin source ---\n${code}\n`;
}

/** Creates a same-origin Blob worker; the CSP allows `worker-src blob:`. */
export function browserWorkerFactory(source: string): PluginWorkerLike {
  const blob = new Blob([source], { type: "text/javascript" });
  const url = URL.createObjectURL(blob);
  const worker = new Worker(url);
  const adapter: PluginWorkerLike = {
    postMessage: (message) => worker.postMessage(message),
    terminate: () => {
      worker.terminate();
      URL.revokeObjectURL(url);
    },
    onmessage: null,
    onerror: null,
    onmessageerror: null,
  };
  worker.onmessage = (event) => adapter.onmessage?.({ data: event.data });
  worker.onerror = (event) => adapter.onerror?.({ message: event.message || "Worker error" });
  worker.onmessageerror = () => adapter.onmessageerror?.();
  return adapter;
}

interface PendingCall {
  resolve: (value: unknown) => void;
  reject: (error: Error) => void;
  timer: ReturnType<typeof setTimeout>;
}

export interface PluginRuntimeOptions {
  manifest: PluginManifest;
  source: string;
  host: PluginHost;
  workerFactory?: PluginWorkerFactory;
  onStatus?: (status: PluginStatus, error: string | null) => void;
}

/**
 * Owns one worker at a time. A crash (worker error, message error or a timed
 * out RPC) rejects every pending call, terminates the worker and marks the
 * plugin crashed; the rest of the app is never affected. Command-level
 * exceptions come back as `kind: "error"` replies and reject only that call.
 */
export class PluginRuntime {
  readonly manifest: PluginManifest;
  private readonly source: string;
  private readonly host: PluginHost;
  private readonly workerFactory: PluginWorkerFactory;
  private readonly onStatus?: (status: PluginStatus, error: string | null) => void;
  private worker: PluginWorkerLike | null = null;
  private pending = new Map<number, PendingCall>();
  private sequence = 1;
  private state: PluginStatus = "idle";
  private lastError: string | null = null;

  constructor(options: PluginRuntimeOptions) {
    this.manifest = options.manifest;
    this.source = options.source;
    this.host = options.host;
    this.workerFactory = options.workerFactory ?? browserWorkerFactory;
    this.onStatus = options.onStatus;
  }

  get status(): PluginStatus {
    return this.state;
  }

  get error(): string | null {
    return this.lastError;
  }

  start(): void {
    if (this.worker) return;
    try {
      const worker = this.workerFactory(buildWorkerSource(this.source));
      worker.onmessage = (event) => this.handleMessage(worker, event.data);
      worker.onerror = (event) => this.crash(event.message || "The plugin worker raised an error.");
      worker.onmessageerror = () => this.crash("The plugin worker sent an unreadable message.");
      this.worker = worker;
      this.state = "running";
      this.lastError = null;
      this.onStatus?.("running", null);
    } catch (error) {
      this.state = "crashed";
      this.lastError = errorText(error);
      this.onStatus?.("crashed", this.lastError);
    }
  }

  async run(method: string, params: unknown): Promise<unknown> {
    if (this.state === "crashed") throw new Error(this.lastError ?? "The plugin has crashed.");
    if (this.state === "disabled") throw new Error("The plugin is disabled.");
    this.start();
    const worker = this.worker;
    if (!worker) throw new Error(this.lastError ?? "The plugin could not start.");
    const id = this.sequence++;
    const timeout = method === "command.run" ? PLUGIN_COMMAND_TIMEOUT_MS : PLUGIN_RPC_TIMEOUT_MS;
    const promise = new Promise<unknown>((resolve, reject) => {
      const timer = setTimeout(() => {
        const entry = this.pending.get(id);
        this.pending.delete(id);
        entry?.reject(new Error("The plugin call timed out."));
        this.crash("The plugin call timed out.");
      }, timeout);
      this.pending.set(id, { resolve, reject, timer });
    });
    worker.postMessage({ id, kind: "call", method, params: params ?? {} });
    return promise;
  }

  restart(): void {
    this.teardown();
    this.state = "idle";
    this.lastError = null;
    this.start();
  }

  dispose(): void {
    this.teardown();
    this.state = "disabled";
    this.lastError = null;
    this.onStatus?.("disabled", null);
  }

  private teardown(): void {
    const worker = this.worker;
    this.worker = null;
    if (worker) worker.terminate();
    for (const [, entry] of this.pending) {
      clearTimeout(entry.timer);
      entry.reject(new Error("The plugin was stopped."));
    }
    this.pending.clear();
  }

  private crash(reason: string): void {
    if (this.state === "crashed") return;
    const message = reason.trim() || "The plugin crashed.";
    const worker = this.worker;
    this.worker = null;
    if (worker) worker.terminate();
    for (const [, entry] of this.pending) {
      clearTimeout(entry.timer);
      entry.reject(new Error(message));
    }
    this.pending.clear();
    this.state = "crashed";
    this.lastError = message;
    this.onStatus?.("crashed", message);
  }

  private handleMessage(worker: PluginWorkerLike, data: unknown): void {
    if (this.worker !== worker || !isRecord(data)) return;
    if (data.kind === "result" || data.kind === "error") {
      const id = typeof data.id === "number" ? data.id : -1;
      const entry = this.pending.get(id);
      if (!entry) return;
      this.pending.delete(id);
      clearTimeout(entry.timer);
      if (data.kind === "result") entry.resolve(data.value);
      else entry.reject(new Error(typeof data.error === "string" ? data.error : "The plugin call failed."));
      return;
    }
    if (data.kind === "call") {
      const id = typeof data.id === "number" ? data.id : null;
      const method = typeof data.method === "string" ? data.method : "";
      const params = isRecord(data.params) ? data.params : {};
      void this.host.handle(method, params).then(
        (value) => {
          if (id !== null && this.worker === worker) worker.postMessage({ id, kind: "result", value });
        },
        (error) => {
          if (id !== null && this.worker === worker) worker.postMessage({ id, kind: "error", error: errorText(error) });
        },
      );
    }
  }
}

// ---------------------------------------------------------------------------
// Command registration
// ---------------------------------------------------------------------------

export function pluginCommandId(pluginId: string, commandId: string): string {
  return `plugin.${pluginId}.${commandId}`;
}

export function manifestNeedsDocument(manifest: PluginManifest): boolean {
  return manifest.permissions.includes("read_document") || manifest.permissions.includes("modify_document");
}

/**
 * Registers one palette command per manifest command. `isReady` reports the
 * runtime state (running vs crashed/disabled); document capabilities are
 * additionally gated on an open office document.
 */
export function registerPluginCommands(
  manifest: PluginManifest,
  run: (commandId: string) => Promise<unknown>,
  isReady: () => boolean = () => true,
): void {
  for (const command of manifest.commands) {
    registerCommand({
      id: pluginCommandId(manifest.id, command.id),
      titleKey: command.title,
      category: "plugins",
      enabled: () => isReady() && (!manifestNeedsDocument(manifest) || hasActiveOfficeDocument()),
      execute: async () => {
        await run(command.id);
      },
    });
  }
}

export function unregisterPluginCommands(manifest: PluginManifest): void {
  for (const command of manifest.commands) unregisterCommand(pluginCommandId(manifest.id, command.id));
}

// ---------------------------------------------------------------------------
// Store: installed plugins, session runtimes and the default host
// ---------------------------------------------------------------------------

export interface PluginInfo {
  manifest: PluginManifest;
  status: PluginStatus;
  /** Crash reason; command failures go to `lastError` instead. */
  error: string | null;
  lastError: string | null;
  logs: string[];
  lastResult: string | null;
}

function summarizeResult(value: unknown): string {
  if (value === undefined || value === null) return "Done.";
  if (typeof value === "string") return value.slice(0, 500);
  try {
    return JSON.stringify(value).slice(0, 500);
  } catch {
    return String(value).slice(0, 500);
  }
}

/** Localized store messages; the runtime itself never shows raw English. */
function tr(key: string, params?: Record<string, string | number>): string {
  return makeTranslate(useSettings.getState().settings.language)(key, params);
}

function appendLog(id: string, message: string): void {
  usePluginStore.setState((state) => ({
    plugins: state.plugins.map((plugin) =>
      plugin.manifest.id === id ? { ...plugin, logs: [...plugin.logs, message].slice(-100) } : plugin,
    ),
  }));
}

const runtimes = new Map<string, PluginRuntime>();

export function createDefaultPluginHost(
  manifest: PluginManifest,
  transport: PluginTransport = activeTransport,
): PluginHost {
  const clipboard =
    typeof navigator !== "undefined" && navigator.clipboard
      ? {
          read: () => navigator.clipboard.readText(),
          write: (text: string) => navigator.clipboard.writeText(text),
        }
      : undefined;
  return new PluginHost(manifest, {
    transport,
    readDocument: readActiveDocument,
    applyTextEdits: applyDocumentEdits,
    clipboard,
    notify: (message) => {
      useToasts.getState().push({ kind: "info", title: manifest.name, detail: message });
    },
    log: (message) => appendLog(manifest.id, message),
  });
}

interface PluginStoreState {
  plugins: PluginInfo[];
  loaded: boolean;
  busy: boolean;
  load: () => Promise<void>;
  install: () => Promise<void>;
  reloadSample: () => Promise<void>;
  enable: (id: string) => Promise<void>;
  disable: (id: string) => void;
  restart: (id: string) => Promise<void>;
  remove: (id: string) => Promise<void>;
  run: (id: string, commandId: string) => Promise<void>;
}

export const usePluginStore = create<PluginStoreState>((set, get) => ({
  plugins: [],
  loaded: false,
  busy: false,

  load: async () => {
    set({ busy: true });
    try {
      const records = await activeTransport.list();
      const plugins: PluginInfo[] = [];
      for (const record of records) {
        const result = validateManifest(record.manifest);
        if (!result.ok) continue;
        const existing = get().plugins.find((plugin) => plugin.manifest.id === result.manifest.id);
        plugins.push(
          existing ?? {
            manifest: result.manifest,
            status: "disabled",
            error: null,
            lastError: null,
            logs: [],
            lastResult: null,
          },
        );
      }
      for (const id of [...runtimes.keys()]) {
        if (!plugins.some((plugin) => plugin.manifest.id === id)) {
          runtimes.get(id)?.dispose();
          runtimes.delete(id);
        }
      }
      set({ plugins, loaded: true, busy: false });
    } catch (error) {
      set({ loaded: true, busy: false });
      useToasts.getState().push({ kind: "error", title: "Plugins", detail: errorText(error) });
    }
  },

  install: async () => {
    set({ busy: true });
    try {
      // The folder is chosen by the Rust-side native dialog; the webview never
      // supplies a filesystem path.
      await activeTransport.installFromDialog();
      await get().load();
    } catch (error) {
      useToasts.getState().push({ kind: "error", title: "Plugins", detail: errorText(error) });
    } finally {
      set({ busy: false });
    }
  },

  reloadSample: async () => {
    set({ busy: true });
    try {
      await activeTransport.installSample();
      await get().load();
      await get().enable(SAMPLE_PLUGIN_ID);
      useToasts.getState().push({ kind: "success", title: "Plugins", detail: tr("plugins.sampleInstalled") });
    } catch (error) {
      useToasts.getState().push({ kind: "error", title: "Plugins", detail: errorText(error) });
    } finally {
      set({ busy: false });
    }
  },

  enable: async (id) => {
    const info = get().plugins.find((plugin) => plugin.manifest.id === id);
    if (!info || runtimes.has(id)) return;
    let source: string;
    try {
      source = await activeTransport.readSource(id);
    } catch (error) {
      set({
        plugins: get().plugins.map((plugin) =>
          plugin.manifest.id === id ? { ...plugin, error: errorText(error) } : plugin,
        ),
      });
      useToasts.getState().push({ kind: "error", title: info.manifest.name, detail: errorText(error) });
      return;
    }
    const runtime = new PluginRuntime({
      manifest: info.manifest,
      source,
      host: createDefaultPluginHost(info.manifest),
      onStatus: (status, error) => {
        set({
          plugins: get().plugins.map((plugin) => (plugin.manifest.id === id ? { ...plugin, status, error } : plugin)),
        });
        if (status === "crashed") {
          useToasts
            .getState()
            .push({ kind: "error", title: info.manifest.name, detail: error ?? "The plugin crashed." });
        }
      },
    });
    runtimes.set(id, runtime);
    registerPluginCommands(
      info.manifest,
      (commandId) => get().run(id, commandId),
      () => runtimes.get(id)?.status === "running",
    );
    runtime.start();
  },

  disable: (id) => {
    const runtime = runtimes.get(id);
    if (runtime) {
      runtime.dispose();
      runtimes.delete(id);
    }
    const info = get().plugins.find((plugin) => plugin.manifest.id === id);
    if (info) unregisterPluginCommands(info.manifest);
    set({
      plugins: get().plugins.map((plugin) =>
        plugin.manifest.id === id ? { ...plugin, status: "disabled", error: null } : plugin,
      ),
    });
  },

  restart: async (id) => {
    get().disable(id);
    await get().enable(id);
  },

  remove: async (id) => {
    get().disable(id);
    try {
      await activeTransport.remove(id);
      await get().load();
    } catch (error) {
      useToasts.getState().push({ kind: "error", title: "Plugins", detail: errorText(error) });
    }
  },

  run: async (id, commandId) => {
    const info = get().plugins.find((plugin) => plugin.manifest.id === id);
    const runtime = runtimes.get(id);
    if (!info || !runtime) {
      useToasts.getState().push({ kind: "error", title: "Plugins", detail: tr("plugins.notEnabled") });
      return;
    }
    try {
      const value = await runtime.run("command.run", { command: commandId });
      const summary = summarizeResult(value);
      set({
        plugins: get().plugins.map((plugin) =>
          plugin.manifest.id === id ? { ...plugin, lastResult: summary, error: null, lastError: null } : plugin,
        ),
      });
      useToasts.getState().push({ kind: "info", title: info.manifest.name, detail: summary });
    } catch (error) {
      const message = errorText(error);
      set({
        plugins: get().plugins.map((plugin) =>
          plugin.manifest.id === id ? { ...plugin, lastResult: null, lastError: message } : plugin,
        ),
      });
      useToasts.getState().push({ kind: "error", title: info.manifest.name, detail: message });
    }
  },
}));

/** Runtime handle for a plugin (used by the Plugins screen for status). */
export function pluginRuntimeStatus(id: string): PluginStatus {
  return runtimes.get(id)?.status ?? "disabled";
}
