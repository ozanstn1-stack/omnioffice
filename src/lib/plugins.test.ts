/**
 * Plugin runtime tests.
 *
 * The host is tested against a fake transport and a fake worker, so nothing
 * here needs Tauri, a real Worker or a filesystem. The enforcement points
 * (manifest validation, permission gates, path traversal, crash handling and
 * command gating) are the same code the app runs.
 */
import { afterEach, describe, expect, it, vi } from "vitest";
import sampleManifest from "../../plugins/sample/manifest.json";
import sampleSource from "../../plugins/sample/main.js?raw";
import { getCommand, runCommand } from "./commands";
import { newTextDocument, type TextDocument } from "./office-types";
import { useOfficeTabs } from "./office-store";
import {
  MAX_PLUGIN_FILE_BYTES,
  PLUGIN_COMMAND_TIMEOUT_MS,
  PLUGIN_WORKER_BOOTSTRAP,
  PluginHost,
  PluginRuntime,
  applyDocumentEdits,
  buildWorkerSource,
  hasActiveOfficeDocument,
  officeModelText,
  parseSemver,
  pluginCommandId,
  pluginRuntimeStatus,
  registerPluginCommands,
  safePluginFileName,
  satisfiesAppRange,
  setPluginTransport,
  tauriPluginTransport,
  unregisterPluginCommands,
  usePluginStore,
  validateManifest,
  type PluginHostOptions,
  type PluginManifest,
  type PluginStatus,
  type PluginTransport,
  type PluginWorkerLike,
} from "./plugins";

function manifestJson(patch: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    id: "test.tool",
    name: "Test Tool",
    version: "1.0.0",
    apiVersion: 1,
    compatibility: { app: ">=3.1.0" },
    permissions: ["read_document"],
    capabilities: ["command", "document-read", "ui", "log"],
    commands: [{ id: "run", title: "Run Test Tool" }],
    ...patch,
  };
}

function parsedManifest(patch: Record<string, unknown> = {}): PluginManifest {
  const result = validateManifest(manifestJson(patch));
  if (!result.ok) throw new Error(result.error);
  return result.manifest;
}

function fakeTransport(): PluginTransport {
  return {
    list: vi.fn(async () => []),
    readSource: vi.fn(async () => "self.onPluginMessage = () => 'ok';"),
    install: vi.fn(async () => ({ manifest: manifestJson(), sourceBytes: 24 })),
    installFromDialog: vi.fn(async () => ({ manifest: manifestJson(), sourceBytes: 24 })),
    installSample: vi.fn(async () => ({ manifest: manifestJson(), sourceBytes: 24 })),
    remove: vi.fn(async () => undefined),
    readFile: vi.fn(async () => "file-contents"),
    writeFile: vi.fn(async () => undefined),
    httpRequest: vi.fn(async () => ({ status: 200, body: "hello", truncated: false })),
  };
}

function documentSnapshot(): { kind: "writer"; title: string; text: string; model: unknown } {
  return { kind: "writer", title: "Doc", text: "Hello world", model: { blocks: [] } };
}

function makeHost(options: Partial<PluginHostOptions> = {}, patch: Record<string, unknown> = {}): PluginHost {
  return new PluginHost(parsedManifest(patch), { transport: fakeTransport(), ...options });
}

afterEach(() => {
  useOfficeTabs.setState({ tabs: [], activeId: null });
  usePluginStore.setState({ plugins: [], loaded: false, busy: false });
  setPluginTransport(tauriPluginTransport);
});

// ---------------------------------------------------------------------------
// Manifest validation
// ---------------------------------------------------------------------------

describe("manifest validation", () => {
  it("accepts a valid manifest and keeps the declared permissions", () => {
    const result = validateManifest(manifestJson());
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.manifest.id).toBe("test.tool");
    expect(result.manifest.permissions).toEqual(["read_document"]);
    expect(result.manifest.commands[0]).toEqual({ id: "run", title: "Run Test Tool" });
  });

  it("rejects unknown permissions", () => {
    const result = validateManifest(manifestJson({ permissions: ["read_document", "admin"] }));
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.error).toContain("Unknown permission");
  });

  it("rejects duplicate permissions", () => {
    const result = validateManifest(manifestJson({ permissions: ["read_document", "read_document"] }));
    expect(result.ok).toBe(false);
  });

  it("rejects an app version the manifest does not support", () => {
    const result = validateManifest(manifestJson({ compatibility: { app: ">=99.0.0" } }));
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.error).toContain("requires app");
  });

  it("rejects path traversal and uppercase ids", () => {
    expect(validateManifest(manifestJson({ id: "../evil" })).ok).toBe(false);
    expect(validateManifest(manifestJson({ id: "Test.Tool" })).ok).toBe(false);
    expect(validateManifest(manifestJson({ id: "" })).ok).toBe(false);
  });

  it("rejects a wrong plugin API version", () => {
    expect(validateManifest(manifestJson({ apiVersion: 2 })).ok).toBe(false);
    expect(validateManifest(manifestJson({ apiVersion: "1" })).ok).toBe(false);
  });

  it("rejects a command capability without commands", () => {
    const result = validateManifest(manifestJson({ commands: [], capabilities: ["command"] }));
    expect(result.ok).toBe(false);
  });

  it("rejects unknown capabilities and bad command ids", () => {
    expect(validateManifest(manifestJson({ capabilities: ["root"] })).ok).toBe(false);
    expect(validateManifest(manifestJson({ commands: [{ id: "a/b", title: "Bad" }] })).ok).toBe(false);
  });

  it("parses and compares semver ranges", () => {
    expect(parseSemver("3.1.0")).toEqual([3, 1, 0]);
    expect(parseSemver("3.1")).toBeNull();
    expect(satisfiesAppRange("3.1.0", ">=3.1.0")).toBe(true);
    expect(satisfiesAppRange("3.1.0", ">=3.2.0")).toBe(false);
    expect(satisfiesAppRange("3.2.0", "^3.1.0")).toBe(true);
    expect(satisfiesAppRange("4.0.0", "^3.1.0")).toBe(false);
    expect(satisfiesAppRange("3.1.9", "~3.1.0")).toBe(true);
    expect(satisfiesAppRange("3.2.0", "~3.1.0")).toBe(false);
    expect(satisfiesAppRange("3.1.0", ">=3.0.0, <4.0.0")).toBe(true);
  });

  it("validates the bundled sample plugin and keeps it read-only", () => {
    const result = validateManifest(sampleManifest);
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.manifest.id).toBe("sample.word-counter");
    expect(result.manifest.permissions).toEqual(["read_document"]);
    expect(sampleSource).toContain("self.onPluginMessage");
    expect(sampleSource).toContain('self.host.doc.getText("document")');
    expect(sampleSource).toContain("self.host.ui.notify");
    expect(sampleSource).not.toContain("applyEdits");
    expect(sampleSource).not.toContain("files.write");
  });
});

// ---------------------------------------------------------------------------
// Host permission enforcement
// ---------------------------------------------------------------------------

describe("host permission enforcement", () => {
  it("refuses capabilities the manifest does not declare", async () => {
    const transport = fakeTransport();
    const host = new PluginHost(parsedManifest(), {
      transport,
      readDocument: documentSnapshot,
      applyTextEdits: () => 1,
      clipboard: { read: async () => "x", write: async () => undefined },
    });
    await expect(host.handle("files.read", { name: "notes.txt" })).rejects.toThrow(/Permission denied/);
    await expect(host.handle("files.write", { name: "notes.txt", text: "x" })).rejects.toThrow(/Permission denied/);
    await expect(host.handle("net.request", { request: { url: "https://example.com" } })).rejects.toThrow(/Permission denied/);
    await expect(host.handle("clipboard.read", {})).rejects.toThrow(/Permission denied/);
    await expect(host.handle("clipboard.write", { text: "x" })).rejects.toThrow(/Permission denied/);
    await expect(host.handle("doc.applyEdits", { edits: [{ find: "a", replace: "b" }] })).rejects.toThrow(/Permission denied/);
    expect(transport.readFile).not.toHaveBeenCalled();
    expect(transport.writeFile).not.toHaveBeenCalled();
    expect(transport.httpRequest).not.toHaveBeenCalled();
  });

  it("serves document data only with read_document and an open document", async () => {
    const withoutDocument = makeHost({ readDocument: () => null });
    await expect(withoutDocument.handle("doc.getText", { scope: "document" })).rejects.toThrow(/No document/);

    const host = makeHost({ readDocument: documentSnapshot });
    await expect(host.handle("doc.getText", { scope: "document" })).resolves.toBe("Hello world");
    await expect(host.handle("doc.getModel", { scope: "document" })).resolves.toEqual({ blocks: [] });
    await expect(host.handle("doc.getText", { scope: "everything" })).rejects.toThrow(/Unsupported scope/);
  });

  it("allows edits only with modify_document and reports the applied count", async () => {
    const applyTextEdits = vi.fn(() => 2);
    const host = makeHost({ readDocument: documentSnapshot, applyTextEdits }, { permissions: ["modify_document"] });
    await expect(host.handle("doc.applyEdits", { edits: [{ find: "a", replace: "b" }] })).resolves.toEqual({ applied: 2 });
    expect(applyTextEdits).toHaveBeenCalledWith([{ find: "a", replace: "b" }]);
    await expect(host.handle("doc.applyEdits", { edits: [] })).rejects.toThrow(/1\.\.200/);
  });

  it("gates clipboard access on the clipboard permission", async () => {
    const write = vi.fn(async () => undefined);
    const host = makeHost({ clipboard: { read: async () => "copied", write } }, { permissions: ["clipboard"] });
    await expect(host.handle("clipboard.read", {})).resolves.toBe("copied");
    await expect(host.handle("clipboard.write", { text: "hello" })).resolves.toBeNull();
    expect(write).toHaveBeenCalledWith("hello");
  });

  it("always allows ui.notify and log, and rejects unknown methods", async () => {
    const notify = vi.fn();
    const log = vi.fn();
    const host = makeHost({ notify, log }, { permissions: ["network"] });
    await expect(host.handle("ui.notify", { message: "hi" })).resolves.toBeNull();
    await expect(host.handle("log", { message: "trace" })).resolves.toBeNull();
    expect(notify).toHaveBeenCalledWith("hi");
    expect(log).toHaveBeenCalledWith("trace");
    await expect(host.handle("tauri.invoke", {})).rejects.toThrow(/Unknown host method/);
  });
});

// ---------------------------------------------------------------------------
// File sandbox and network proxy
// ---------------------------------------------------------------------------

describe("file sandbox", () => {
  it("accepts plain names and rejects traversal, absolute paths and separators", () => {
    expect(safePluginFileName("notes.txt")).toBe("notes.txt");
    expect(safePluginFileName("A-1_2.csv")).toBe("A-1_2.csv");
    expect(safePluginFileName("two words.md")).toBe("two words.md");
    for (const bad of ["..", "../x", "a/../b", "sub/dir", "sub\\dir", "/etc/passwd", "C:\\x", ".hidden", "name.", "name ", "a..b", "", "x".repeat(129)]) {
      expect(safePluginFileName(bad), String(bad)).toBeNull();
    }
    expect(safePluginFileName(42)).toBeNull();
  });

  it("never forwards a traversal attempt to the transport", async () => {
    const transport = fakeTransport();
    const host = new PluginHost(parsedManifest({ permissions: ["read_files", "write_files"] }), { transport });
    await expect(host.handle("files.read", { name: "../secret.json" })).rejects.toThrow(/Invalid file name/);
    await expect(host.handle("files.read", { name: "C:\\Users\\me\\secret.txt" })).rejects.toThrow(/Invalid file name/);
    await expect(host.handle("files.write", { name: "../../escape.txt", text: "x" })).rejects.toThrow(/Invalid file name/);
    expect(transport.readFile).not.toHaveBeenCalled();
    expect(transport.writeFile).not.toHaveBeenCalled();

    await expect(host.handle("files.read", { name: "notes.txt" })).resolves.toBe("file-contents");
    expect(transport.readFile).toHaveBeenCalledWith("test.tool", "notes.txt");
  });

  it("bounds file writes", async () => {
    const transport = fakeTransport();
    const host = new PluginHost(parsedManifest({ permissions: ["write_files"] }), { transport });
    await expect(host.handle("files.write", { name: "big.txt", text: "x".repeat(MAX_PLUGIN_FILE_BYTES + 1) })).rejects.toThrow(/too large/);
    await expect(host.handle("files.write", { name: "ok.txt", text: "small" })).resolves.toBeNull();
    expect(transport.writeFile).toHaveBeenCalledWith("test.tool", "ok.txt", "small");
  });
});

describe("network proxy", () => {
  it("only forwards https (or private http) GET/POST requests", async () => {
    const transport = fakeTransport();
    const host = new PluginHost(parsedManifest({ permissions: ["network"] }), { transport });
    await expect(host.handle("net.request", { request: { url: "http://example.com" } })).rejects.toThrow(/https/);
    await expect(host.handle("net.request", { request: { url: "ftp://example.com" } })).rejects.toThrow(/https/);
    await expect(host.handle("net.request", { request: { url: "https://example.com", method: "DELETE" } })).rejects.toThrow(/GET and POST/);

    const response = await host.handle("net.request", { request: { url: "https://example.com/api" } });
    expect(response).toEqual({ status: 200, body: "hello", truncated: false });
    expect(transport.httpRequest).toHaveBeenCalledWith("test.tool", expect.objectContaining({ method: "GET" }));

    await expect(host.handle("net.request", { request: { url: "http://127.0.0.1:8080/health" } })).resolves.toEqual({
      status: 200,
      body: "hello",
      truncated: false,
    });
  });

  it("caps the response body it returns", async () => {
    const transport = fakeTransport();
    transport.httpRequest = vi.fn(async () => ({ status: 200, body: "y".repeat(2_000_000), truncated: true }));
    const host = new PluginHost(parsedManifest({ permissions: ["network"] }), { transport });
    const response = (await host.handle("net.request", { request: { url: "https://example.com" } })) as { body: string };
    expect(response.body.length).toBe(1024 * 1024);
  });
});

// ---------------------------------------------------------------------------
// Worker runtime and crash isolation
// ---------------------------------------------------------------------------

class FakeWorker implements PluginWorkerLike {
  onmessage: ((event: { data: unknown }) => void) | null = null;
  onerror: ((event: { message?: string }) => void) | null = null;
  onmessageerror: (() => void) | null = null;
  sent: Array<Record<string, unknown>> = [];
  terminated = false;

  postMessage(message: unknown): void {
    this.sent.push(message as Record<string, unknown>);
  }

  terminate(): void {
    this.terminated = true;
  }
}

function makeRuntime(
  worker: FakeWorker,
  options: { manifestPatch?: Record<string, unknown>; onStatus?: (status: PluginStatus, error: string | null) => void; log?: (message: string) => void } = {},
) {
  const manifest = parsedManifest(options.manifestPatch ?? {});
  const host = new PluginHost(manifest, { transport: fakeTransport(), log: options.log, notify: vi.fn() });
  return new PluginRuntime({
    manifest,
    source: "self.onPluginMessage = () => 'done';",
    host,
    workerFactory: () => worker,
    onStatus: options.onStatus,
  });
}

describe("worker runtime", () => {
  it("wraps plugin code in the RPC bootstrap", () => {
    const source = buildWorkerSource("self.onPluginMessage = () => 1;");
    expect(source).toContain("self.host");
    expect(source).toContain('kind: "call"');
    expect(source).toContain("command.run");
    expect(source.endsWith("self.onPluginMessage = () => 1;\n")).toBe(true);
    expect(PLUGIN_WORKER_BOOTSTRAP).toContain("self.postMessage");
  });

  it("round-trips a command through the worker", async () => {
    const worker = new FakeWorker();
    const runtime = makeRuntime(worker);
    runtime.start();
    expect(runtime.status).toBe("running");
    const promise = runtime.run("command.run", { command: "run" });
    expect(worker.sent[0]).toMatchObject({ kind: "call", method: "command.run", params: { command: "run" } });
    worker.onmessage?.({ data: { id: worker.sent[0].id, kind: "result", value: "ok" } });
    await expect(promise).resolves.toBe("ok");
  });

  it("answers host calls from the worker and reports denials as errors", async () => {
    const worker = new FakeWorker();
    const log = vi.fn();
    const runtime = makeRuntime(worker, { log });
    runtime.start();
    worker.onmessage?.({ data: { id: 7, kind: "call", method: "log", params: { message: "hello" } } });
    await vi.waitFor(() => expect(worker.sent.some((message) => message.id === 7 && message.kind === "result")).toBe(true));
    expect(log).toHaveBeenCalledWith("hello");

    worker.onmessage?.({ data: { id: 8, kind: "call", method: "files.read", params: { name: "notes.txt" } } });
    await vi.waitFor(() => expect(worker.sent.some((message) => message.id === 8 && message.kind === "error")).toBe(true));
    const denial = worker.sent.find((message) => message.id === 8);
    expect(String(denial?.error)).toContain("Permission denied");
  });

  it("rejects only the failed command when the plugin throws", async () => {
    const worker = new FakeWorker();
    const runtime = makeRuntime(worker);
    runtime.start();
    const failed = runtime.run("command.run", { command: "run" });
    worker.onmessage?.({ data: { id: worker.sent[0].id, kind: "error", error: "boom inside the command" } });
    await expect(failed).rejects.toThrow("boom inside the command");
    expect(runtime.status).toBe("running");

    const next = runtime.run("command.run", { command: "run" });
    worker.onmessage?.({ data: { id: worker.sent[1].id, kind: "result", value: "fine" } });
    await expect(next).resolves.toBe("fine");
  });

  it("marks the plugin crashed on worker errors and rejects pending calls", async () => {
    const worker = new FakeWorker();
    const statuses: string[] = [];
    const runtime = makeRuntime(worker, { onStatus: (status) => statuses.push(status) });
    runtime.start();
    const pending = runtime.run("command.run", { command: "run" });
    worker.onerror?.({ message: "worker exploded" });
    await expect(pending).rejects.toThrow("worker exploded");
    expect(runtime.status).toBe("crashed");
    expect(worker.terminated).toBe(true);
    expect(statuses).toContain("crashed");
    await expect(runtime.run("command.run", { command: "run" })).rejects.toThrow("worker exploded");
  });

  it("treats unreadable worker messages as a crash", () => {
    const worker = new FakeWorker();
    const runtime = makeRuntime(worker);
    runtime.start();
    worker.onmessageerror?.();
    expect(runtime.status).toBe("crashed");
  });

  it("marks the plugin crashed and settles the call when an RPC times out", async () => {
    vi.useFakeTimers();
    try {
      const worker = new FakeWorker();
      const runtime = makeRuntime(worker);
      runtime.start();
      const pending = runtime.run("command.run", { command: "run" });
      const assertion = expect(pending).rejects.toThrow(/timed out/);
      await vi.advanceTimersByTimeAsync(PLUGIN_COMMAND_TIMEOUT_MS + 1);
      await assertion;
      expect(runtime.status).toBe("crashed");
    } finally {
      vi.useRealTimers();
    }
  });

  it("restarts a crashed plugin on a fresh worker", () => {
    const workers = [new FakeWorker(), new FakeWorker()];
    let index = 0;
    const manifest = parsedManifest();
    const host = new PluginHost(manifest, { transport: fakeTransport() });
    const runtime = new PluginRuntime({ manifest, source: "x", host, workerFactory: () => workers[index++] });
    runtime.start();
    workers[0].onerror?.({ message: "boom" });
    expect(runtime.status).toBe("crashed");
    runtime.restart();
    expect(runtime.status).toBe("running");
    expect(workers[1].terminated).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// Command registration gating
// ---------------------------------------------------------------------------

describe("plugin command registration", () => {
  it("hides document commands until a document is open, then runs them", async () => {
    const manifest = parsedManifest();
    const run = vi.fn(async () => "ok");
    registerPluginCommands(manifest, run, () => true);
    const id = pluginCommandId(manifest.id, "run");
    expect(hasActiveOfficeDocument()).toBe(false);
    expect(getCommand(id)?.enabled()).toBe(false);
    expect(runCommand(id)).toBe(false);

    useOfficeTabs.getState().create("writer", "Doc");
    expect(hasActiveOfficeDocument()).toBe(true);
    expect(getCommand(id)?.enabled()).toBe(true);
    expect(runCommand(id)).toBe(true);
    await vi.waitFor(() => expect(run).toHaveBeenCalledWith("run"));

    unregisterPluginCommands(manifest);
    expect(getCommand(id)).toBeUndefined();
  });

  it("does not require a document for plugins without document permissions", () => {
    const manifest = parsedManifest({ permissions: ["clipboard"] });
    registerPluginCommands(manifest, async () => undefined, () => true);
    expect(getCommand(pluginCommandId(manifest.id, "run"))?.enabled()).toBe(true);
    unregisterPluginCommands(manifest);
  });

  it("is disabled while the runtime is not running", () => {
    const manifest = parsedManifest();
    registerPluginCommands(manifest, async () => undefined, () => false);
    expect(getCommand(pluginCommandId(manifest.id, "run"))?.enabled()).toBe(false);
    unregisterPluginCommands(manifest);
  });
});

// ---------------------------------------------------------------------------
// Document integration (normal store path)
// ---------------------------------------------------------------------------

describe("document integration", () => {
  it("reads the active document text", () => {
    const model = newTextDocument();
    const block = model.blocks[0];
    if (block.type === "paragraph") block.runs[0].text = "Hello world";
    useOfficeTabs.getState().create("writer", "Doc", model);
    expect(officeModelText(model)).toBe("Hello world");
  });

  it("applies edits through the office store and marks the tab dirty", () => {
    useOfficeTabs.setState({ tabs: [], activeId: null });
    const model = newTextDocument();
    const block = model.blocks[0];
    if (block.type === "paragraph") block.runs[0].text = "Hello world";
    const id = useOfficeTabs.getState().create("writer", "Doc", model);
    expect(applyDocumentEdits([{ find: "world", replace: "planet" }])).toBe(1);
    const tab = useOfficeTabs.getState().tabs.find((candidate) => candidate.id === id);
    expect(officeModelText(tab?.model as TextDocument)).toContain("Hello planet");
    expect(tab?.dirty).toBe(true);
  });

  it("does not match an edit that spans two runs", () => {
    useOfficeTabs.setState({ tabs: [], activeId: null });
    const model = newTextDocument();
    const block = model.blocks[0];
    if (block.type === "paragraph") {
      block.runs = [
        { ...block.runs[0], text: "Hel" },
        { ...block.runs[0], text: "lo" },
      ];
    }
    useOfficeTabs.getState().create("writer", "Doc", model);
    expect(applyDocumentEdits([{ find: "Hello", replace: "Hi" }])).toBe(0);
  });

  it("refuses edits when no document is open", () => {
    useOfficeTabs.setState({ tabs: [], activeId: null });
    expect(() => applyDocumentEdits([{ find: "a", replace: "b" }])).toThrow(/No document/);
  });
});

// ---------------------------------------------------------------------------
// Store: install/load wiring
// ---------------------------------------------------------------------------

describe("plugin store", () => {
  it("loads backend manifests, skipping the invalid ones", async () => {
    const transport = fakeTransport();
    transport.list = vi.fn(async () => [
      { manifest: manifestJson(), sourceBytes: 24 },
      { manifest: { id: "broken" }, sourceBytes: 1 },
    ]);
    setPluginTransport(transport);
    await usePluginStore.getState().load();
    const state = usePluginStore.getState();
    expect(state.loaded).toBe(true);
    expect(state.plugins.map((plugin) => plugin.manifest.id)).toEqual(["test.tool"]);
    expect(state.plugins[0].status).toBe("disabled");
  });

  it("reports a runtime as disabled until it is enabled", () => {
    expect(pluginRuntimeStatus("test.tool")).toBe("disabled");
  });
});
