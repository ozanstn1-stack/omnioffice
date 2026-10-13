# Plugin API v2

OmniOffice plugins are small, sandboxed programs that extend the app through a
capability API. A plugin is a folder with two files:

```
my-plugin/
  manifest.json   # what the plugin is allowed to do
  main.js         # the code, run inside a Web Worker
```

Plugins never touch the page, the network or the filesystem directly. Every
capability request crosses an RPC bridge where the host re-checks the manifest,
so a plugin only ever gets what it asked for — and the host can refuse even
that.

## API versions

| Version | Introduced | Highlights |
| --- | --- | --- |
| 1 | 3.1.0 | Commands, document read/write, sandboxed files, clipboard, network, UI, log. |
| 2 | 5.0.0 | `pdf` capability (read-only PDF text/stats), `batch-step` capability (recipe steps). |

A manifest declares the API it was written for with `apiVersion`. This build
speaks **1–2**: existing v1 plugins keep loading unchanged, and new
capabilities require `apiVersion: 2`.

## Manifest

```json
{
  "id": "example.pdf-stats",
  "name": "PDF Stats",
  "version": "2.0.0",
  "apiVersion": 2,
  "compatibility": { "app": ">=5.0.0" },
  "permissions": ["read_pdf"],
  "capabilities": ["command", "pdf", "ui", "log"],
  "commands": [
    { "id": "stats", "title": "PDF Stats: pages and words" }
  ]
}
```

- `id` — lowercase letters, digits, `.`, `-`, `_`.
- `version` — semver (`x.y.z`).
- `apiVersion` — `1` or `2`.
- `compatibility.app` — a version range for the app (see below).
- `permissions` — the data the plugin may touch (below).
- `capabilities` — the API surface it may call.
- `commands` — palette commands (required when the `command` capability is used).
- `batchSteps` — recipe steps (v2; requires the `batch-step` capability).

Version ranges accept a bare version (`4.12.0`), a caret (`^4.12.0`) or a
comparator (`>=4.12.0`, `<5.0.0`).

## Permissions

| Permission | Grants |
| --- | --- |
| `read_document` | Read the active office document text/model. |
| `modify_document` | Apply text edits to the active document. |
| `read_files` | Read files inside the plugin's sandbox. |
| `write_files` | Write files inside the plugin's sandbox. |
| `clipboard` | Read/write the system clipboard. |
| `network` | HTTPS requests through the host proxy (public hosts only). |
| `read_pdf` | Read page count and text from a PDF in the sandbox (v2). |
| `batch_steps` | Expose batch steps to the recipe engine (v2). |

## Capabilities

| Capability | Host API |
| --- | --- |
| `command` | Registers one palette command per `commands[]` entry. |
| `document-read` | `host.doc.getText(scope)`, `host.doc.getModel(scope)` |
| `document-write` | `host.doc.applyEdits(edits)` |
| `files` | `host.files.read(name)`, `host.files.write(name, text)` |
| `clipboard` | `host.clipboard.read()`, `host.clipboard.write(text)` |
| `network` | `host.net.request({ url, method, headers, body })` |
| `ui` | `host.ui.notify(message)` |
| `log` | `host.log(message)` |
| `pdf` | `host.pdf.read(name)` (v2) |
| `batch-step` | The manifest's `batchSteps[]` become selectable recipe steps (v2). |

A capability without its matching permission is rejected at manifest load:
`pdf` requires `read_pdf`, `batch-step` requires `batch_steps`.

## The plugin entry point

The host calls `self.onPluginMessage` when the user runs a command:

```js
self.onPluginMessage = async function (call) {
  if (call.kind !== "command") return "Unknown call.";
  if (call.command !== "stats") return "Unknown command: " + call.command;
  var pdf = await self.host.pdf.read("report.pdf");
  await self.host.ui.notify(pdf.pageCount + " pages, " + pdf.text.length + " chars");
  return pdf.pageCount + " pages";
};
```

Returned values are shown to the user (truncated). Throwing rejects only that
call; the worker stays alive. A crash (uncaught worker error or timeout) is
isolated and shown in the Plugins screen.

### `host.pdf.read(name)` (v2)

Reads a PDF **inside the plugin's sandbox** and returns:

```ts
{ pageCount: number; hasTextLayer: boolean; text: string }
```

`text` is bounded (256 KB) so a huge PDF cannot exhaust the worker.

## Batch steps (v2)

A plugin can contribute a step to the batch/recipe engine. Declare the steps in
the manifest and handle them like commands:

```json
{
  "capabilities": ["command", "batch-step", "ui", "log"],
  "permissions": ["batch_steps"],
  "commands": [{ "id": "normalize", "title": "Normalize whitespace" }],
  "batchSteps": [
    { "id": "normalize", "title": "Normalize whitespace", "description": "Collapses runs of spaces and blank lines." }
  ]
}
```

When a recipe runs the step, the host calls the plugin with the step id as the
command and passes the input:

```js
self.onPluginMessage = async function (call) {
  if (call.command !== "normalize") return "Unknown command.";
  var input = call.params.input;          // { name, text }
  var out = input.text.replace(/[ \t]+/g, " ").replace(/\n{3,}/g, "\n\n");
  return { name: input.name, text: out };
};
```

The step id doubles as the command id, so one handler can serve both the palette
and the recipe engine.

## Security model

- **Worker isolation.** Plugin code runs in a Web Worker created from a Blob;
  `fetch`, `XMLHttpRequest`, `WebSocket`, `importScripts`, `indexedDB` and
  `caches` are shadowed to `undefined` inside it.
- **Manifest validation.** Unknown permissions, capabilities, commands or batch
  steps are rejected — never silently ignored.
- **Defense in depth.** File and PDF access is resolved inside the per-plugin
  sandbox directory in Rust as well; the frontend never learns a real path.
- **Network proxy.** Requests go through Rust, are HTTPS-only (plain HTTP only
  for loopback), and private/loopback/metadata destinations are refused (SSRF).
- **Bounded I/O.** File, source, response and PDF-text sizes are capped.

## Examples

- `plugins/sample` — v1 Word Frequency Counter (`read_document`).
- `plugins/examples/pdf-stats` — v2 `pdf` capability.
- `plugins/examples/text-normalizer` — v2 `batch-step` capability.
