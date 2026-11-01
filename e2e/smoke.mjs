/**
 * End-to-end entry point. Each flag selects a flow from `./flows/`; the shared
 * WebDriver plumbing lives in `./lib/driver.mjs`, the working copies in
 * `./lib/files.mjs`. CI runs the plain smoke, the Reader flow and the ACL gate;
 * the office/merge/sanitize flows and the personas run on demand and in the
 * deep-flow job.
 *
 * Environment:
 *   TAURI_DRIVER_PORT     first WebDriver port (default 4444; scenarios take the next port)
 *   TAURI_NATIVE_DRIVER   path to msedgedriver (Windows; must match WebView2)
 *
 * The app is launched by tauri-driver with the dev launch context
 * (PDFSAK_START_SCREEN / PDFSAK_DEV_FILES / PDFSAK_DEV_RUN) so no file dialog
 * is involved. The Reader step needs the native engines next to the binary;
 * the office/merge/sanitize flows are engine-free (officecore + lopdf) and run
 * in the Linux CI job. Working copies live under e2e/artifacts/work/ so the
 * checked-in samples are never modified.
 */
import { mkdirSync } from "node:fs";
import { resolve } from "node:path";

import { aclGate } from "./flows/acl.mjs";
import { homeAndSettings } from "./flows/home.mjs";
import { officeRoundtrip } from "./flows/office.mjs";
import { mergeFlow, sanitizeFlow } from "./flows/pdf.mjs";
import { accountantPersona, lawyerPersona, publicPersona, studentPersona } from "./flows/personas.mjs";
import { reader } from "./flows/reader.mjs";
import { artifacts, argv, pdf, requireAppBinary, valueOf, withSession, withSessionRetry } from "./lib/driver.mjs";
import { prepareWorkFile } from "./lib/files.mjs";

async function main() {
  requireAppBinary();
  mkdirSync(artifacts, { recursive: true });
  const scenarios = [];
  if (argv.includes("--office") || argv.includes("--all")) scenarios.push("office");
  if (argv.includes("--merge") || argv.includes("--all")) scenarios.push("merge");
  if (argv.includes("--sanitize") || argv.includes("--all")) scenarios.push("sanitize");
  if (argv.includes("--acl")) scenarios.push("acl");

  if (pdf) {
    await withSession("reader", { PDFSAK_START_SCREEN: "reader", PDFSAK_DEV_FILES: resolve(pdf) }, reader);
    return;
  }
  const persona = valueOf("--persona");
  if (persona) {
    const personas = {
      accountant: { screen: "invoices", file: "sample-invoice.xml", run: accountantPersona },
      student: { screen: "office", file: "test-document.docx", run: studentPersona },
      public: { screen: "office", file: "test-document.docx", run: publicPersona },
      lawyer: { screen: "redact", file: "redaction-sample.pdf", run: lawyerPersona, autoRun: true },
    };
    const names =
      persona === "all"
        ? Object.keys(personas)
        : persona
            .split(",")
            .map((name) => name.trim())
            .filter(Boolean);
    for (const name of names) {
      const spec = personas[name];
      if (!spec) throw new Error(`unknown persona: ${name} (known: ${Object.keys(personas).join(", ")})`);
      const file = prepareWorkFile(spec.file);
      await withSession(
        `persona-${name}`,
        {
          PDFSAK_START_SCREEN: spec.screen,
          PDFSAK_DEV_FILES: file,
          ...(spec.autoRun ? { PDFSAK_DEV_RUN: "1" } : {}),
        },
        spec.run,
      );
    }
    return;
  }
  if (scenarios.length === 0) {
    await withSession("smoke", {}, homeAndSettings);
    return;
  }
  for (const scenario of scenarios) {
    if (scenario === "office") {
      const doc = prepareWorkFile("test-document.docx");
      await withSessionRetry("office", { PDFSAK_START_SCREEN: "office", PDFSAK_DEV_FILES: doc }, officeRoundtrip);
    } else if (scenario === "merge") {
      const first = prepareWorkFile("sample-1.pdf");
      const second = prepareWorkFile("sample-2.pdf");
      await withSessionRetry(
        "merge",
        { PDFSAK_START_SCREEN: "merge", PDFSAK_DEV_FILES: `${first};${second}`, PDFSAK_DEV_RUN: "1" },
        mergeFlow,
      );
    } else if (scenario === "acl") {
      await withSessionRetry("acl", {}, aclGate);
    } else if (scenario === "sanitize") {
      const input = prepareWorkFile("sample-1.pdf");
      await withSessionRetry("sanitize", { PDFSAK_START_SCREEN: "pdfStudio", PDFSAK_DEV_FILES: input }, sanitizeFlow);
    }
  }
}

// Exit explicitly: helper processes started under tauri-driver (e.g. WebKit's
// network process on Linux) can inherit its stdout/stderr pipes and outlive
// it, which kept Node's event loop - and the CI step - alive long after every
// scenario had passed.
main().then(
  () => process.exit(0),
  (error) => {
    console.error(`E2E FAILED: ${error.message}`);
    process.exit(1);
  },
);
