import { readFileSync, readdirSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";

// The end-to-end suite started as one 735-line file where the WebDriver plumbing,
// the flows and the dispatch were interleaved. This keeps the split honest: the
// entry point stays a thin dispatcher, and every flow the deep-flow job runs is
// still exported from its module.
const root = path.resolve(__dirname, "..", "..");
const e2e = path.join(root, "e2e");
const read = (name: string) => readFileSync(path.join(e2e, name), "utf8");

const EXPORTS: Record<string, string[]> = {
  "flows/home.mjs": ["homeAndSettings"],
  "flows/reader.mjs": ["reader"],
  "flows/office.mjs": ["officeRoundtrip", "activateWriter", "typeInWriter", "openReviewRibbon"],
  "flows/pdf.mjs": ["mergeFlow", "sanitizeFlow"],
  "flows/acl.mjs": ["aclGate"],
  "flows/personas.mjs": ["accountantPersona", "studentPersona", "publicPersona", "lawyerPersona"],
  "lib/driver.mjs": ["withSession", "withSessionRetry", "requireAppBinary", "waitForScript"],
  "lib/files.mjs": ["prepareWorkFile", "elementKey"],
};

describe("end-to-end scenarios are modules, not one file (E4.3)", () => {
  it("keeps the entry point to argument parsing and dispatch", () => {
    const smoke = read("smoke.mjs");
    expect(smoke.split("\n").length).toBeLessThan(150);
    expect(smoke).toContain('from "./lib/driver.mjs"');
    expect(smoke).toContain('from "./flows/personas.mjs"');
    // The WebDriver plumbing lives in lib/driver.mjs now.
    expect(smoke).not.toContain("new XMLHttpRequest");
    expect(smoke).not.toContain("function webdriver(");
  });

  it("exports every flow the CI jobs and the personas run", () => {
    for (const [file, names] of Object.entries(EXPORTS)) {
      const body = read(file);
      for (const name of names) {
        const declared =
          new RegExp(`export (async )?function ${name}\\(`).test(body) ||
          new RegExp(`export const ${name} = `).test(body);
        expect(declared, `${file} must export ${name}`).toBe(true);
      }
    }
  });

  it("imports every module in the flow directory", () => {
    const smoke = read("smoke.mjs");
    for (const file of readdirSync(path.join(e2e, "flows"))) {
      expect(smoke, file).toContain(`./flows/${file}`);
    }
  });

  it("keeps the binary guard out of module load", () => {
    // Importing a module must not exit: the guard moved behind requireAppBinary()
    // so tools (and this test) can load the plumbing without a built app.
    expect(read("lib/driver.mjs")).toContain("export function requireAppBinary()");
    expect(read("smoke.mjs")).toContain("requireAppBinary();");
  });
});
