import { execFileSync } from "node:child_process";
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { afterEach, describe, expect, it } from "vitest";

import { ATTRIBUTES, PROPS, scanSource } from "../../scripts/lib/i18n-jsx.mjs";

// The JSX scanner and its ratchet (E4.5). The scanner is a pure function, so the
// counting rules are tested on inline snippets; the ratchet is tested through
// the audit script on a temporary tree (the I18N_AUDIT_ROOT hook), so the real
// `i18n-baseline.json` is never written by a test.
const root = path.resolve(__dirname, "..", "..");
const script = path.join(root, "scripts", "i18n-audit.mjs");
const cleanups: string[] = [];

afterEach(() => {
  for (const directory of cleanups.splice(0)) rmSync(directory, { recursive: true, force: true });
});

const scan = (body: string) => scanSource("Sample.tsx", body);

describe("i18n JSX scanner", () => {
  it("counts the three literal forms of a prop", () => {
    expect(scan('<Field label="Save" />').props.map((row) => row.text)).toEqual(["Save"]);
    expect(scan('<Field label={"Save"} />').props.map((row) => row.text)).toEqual(["Save"]);
    expect(scan("<Field label={`Save`} />").props.map((row) => row.text)).toEqual(["Save"]);
  });

  it("counts a braced string on an attribute too", () => {
    expect(scan('<input title={"Help"} />').attributes.map((row) => row.text)).toEqual(["Help"]);
    expect(scan("<img alt={`Chart`} />").attributes.map((row) => row.text)).toEqual(["Chart"]);
  });

  it("ignores translated calls, variables and dynamic templates", () => {
    expect(scan('<Field label={t("a.b")} />').props).toHaveLength(0);
    expect(scan("<Field label={value} />").props).toHaveLength(0);
    expect(scan("<Field label={`Total ${count}`} />").props).toHaveLength(0);
    // A conditional's branches are the documented gap of the first version.
    expect(scan('<Field label={flag ? "A" : "B"} />').props).toHaveLength(0);
  });

  it("skips separators and empty values", () => {
    expect(scan('<Field label="—" />').props).toHaveLength(0);
    expect(scan('<Field label="" />').props).toHaveLength(0);
    expect(scan('<Field label={"1"} />').props).toHaveLength(0);
  });

  it("keeps counting JSX text nodes, and never mistakes code for prose", () => {
    expect(scan("<p>Merhaba dünya</p>").texts).toHaveLength(1);
    const body = "const fn = <T,>(value: T) => value;\nconst bigger = a > b;\n<Map<K, V> />;\n";
    expect(scan(body).texts).toHaveLength(0);
  });

  it("records the line of every entry", () => {
    const found = scan('<div>\n  <Field label="Date" />\n</div>');
    expect(found.props[0].line).toBe(2);
  });

  it("counts the sibling props the components really declare", () => {
    // `hint` is a `Field`/`SectionTitle` prop and `subtitle` a `Screen` prop:
    // both reach the screen, and neither is a JSX attribute.
    for (const name of ["hint", "subtitle"]) {
      expect(scan(`<Field ${name}="Save" />`).props.map((row) => row.name)).toEqual([name]);
      expect(scan(`<Field ${name}={"Save"} />`).props.map((row) => row.name)).toEqual([name]);
      expect(scan(`<Field ${name}={\`Save\`} />`).props.map((row) => row.name)).toEqual([name]);
    }
  });

  it("leaves a translated or conditional sibling prop out of the count", () => {
    expect(scan('<Field hint={t("a.b")} />').props).toHaveLength(0);
    expect(scan("<Field hint={value} />").props).toHaveLength(0);
    expect(scan('<Screen subtitle={flag ? "A" : "B"} />').props).toHaveLength(0);
  });

  it("does not count a title twice when a component takes it as a prop", () => {
    // `title` stays an attribute only: counting it as a prop too would report
    // the same text twice.
    const found = scan('<Screen title="Settings" subtitle="Explain the screen" />');
    expect(found.attributes.map((row) => row.name)).toEqual(["title"]);
    expect(found.props.map((row) => row.name)).toEqual(["subtitle"]);
  });

  it("keeps the prop list explicit, so a new prop is a deliberate change", () => {
    expect(PROPS).toEqual(["label", "hint", "subtitle"]);
    expect(ATTRIBUTES).toEqual(["alt", "aria-label", "title", "placeholder"]);
  });
});

/** A throwaway repository with the dictionaries, one TSX file and a baseline. */
function tempTree(baseline: unknown, source: string): { root: string; baseline: string } {
  const directory = mkdtempSync(path.join(tmpdir(), "i18n-audit-"));
  cleanups.push(directory);
  mkdirSync(path.join(directory, "src/lib/i18n"), { recursive: true });
  for (const language of ["en", "tr"]) {
    cpSync(
      path.join(root, "src/lib/i18n", `${language}.json`),
      path.join(directory, "src/lib/i18n", `${language}.json`),
    );
  }
  mkdirSync(path.join(directory, "src/screens"), { recursive: true });
  writeFileSync(path.join(directory, "src/screens/Sample.tsx"), source);
  const baselinePath = path.join(directory, "baseline.json");
  writeFileSync(baselinePath, `${JSON.stringify(baseline, null, 2)}\n`);
  return { root: directory, baseline: baselinePath };
}

function run(tree: { root: string; baseline: string }, ...flags: string[]): { status: number; output: string } {
  try {
    const output = execFileSync(process.execPath, [script, ...flags], {
      encoding: "utf8",
      env: { ...process.env, I18N_AUDIT_ROOT: tree.root, I18N_BASELINE: tree.baseline },
    });
    return { status: 0, output };
  } catch (caught) {
    const error = caught as { status?: number; stdout?: string; stderr?: string };
    return { status: error.status ?? 1, output: `${error.stdout ?? ""}${error.stderr ?? ""}` };
  }
}

describe("i18n props ratchet", () => {
  const emptySections = { files: {}, attributes: {} };

  it("fails when a file grows above its props baseline", () => {
    const tree = tempTree({ version: 3, ...emptySections, props: {} }, '<Field label="Save" />');
    const result = run(tree, "--check");
    expect(result.status).toBe(1);
    expect(result.output).toContain("prop string");
  });

  it("accepts the file once the props baseline records it", () => {
    const tree = tempTree(
      { version: 3, ...emptySections, props: { "screens/Sample.tsx": 1 } },
      '<Field label="Save" />',
    );
    expect(run(tree, "--check").status).toBe(0);
  });

  it("refuses to raise a props count without --allow-raise, and raises it with the flag", () => {
    const tree = tempTree(
      { version: 3, ...emptySections, props: { "screens/Sample.tsx": 0 } },
      '<Field label="Save" />',
    );
    const refused = run(tree, "--update");
    expect(refused.status).toBe(1);
    expect(refused.output).toContain("may only shrink");

    expect(run(tree, "--update", "--allow-raise").status).toBe(0);
    const written = JSON.parse(readFileSync(tree.baseline, "utf8")) as {
      version: number;
      props: Record<string, number>;
    };
    expect(written.version).toBe(3);
    expect(written.props["screens/Sample.tsx"]).toBe(1);
  });

  it("asks for a re-record instead of passing when the baseline has no props section", () => {
    const tree = tempTree({ version: 2, files: {}, attributes: {} }, '<Field label="Save" />');
    const result = run(tree, "--check");
    expect(result.status).toBe(1);
    expect(result.output).toContain("no `props` section");
  });

  it("fails when a sibling prop grows above its baseline", () => {
    const tree = tempTree({ version: 3, ...emptySections, props: {} }, '<Field hint="Tarih girin" />');
    const result = run(tree, "--check");
    expect(result.status).toBe(1);
    expect(result.output).toContain("prop string");
    expect(result.output).toContain("screens/Sample.tsx");
  });

  it("names the sibling prop in the backlog report", () => {
    const tree = tempTree({ version: 3, ...emptySections, props: {} }, '<Screen subtitle="Ekranı anlat" />');
    const result = run(tree, "--props-report");
    expect(result.status).toBe(0);
    expect(result.output).toContain("screens/Sample.tsx:1\tsubtitle\tEkranı anlat");
  });

  it("keeps the baseline schema at v3 when the widened props are recorded", () => {
    const tree = tempTree({ version: 3, ...emptySections, props: {} }, '<Field hint="Tarih girin" />');
    expect(run(tree, "--update", "--allow-raise").status).toBe(0);
    const written = JSON.parse(readFileSync(tree.baseline, "utf8")) as Record<string, unknown>;
    // Widening PROPS changes what is counted, not how the baseline is shaped.
    expect(written.version).toBe(3);
    expect(Object.keys(written).sort()).toEqual(["attributes", "files", "props", "version"]);
  });

  it("prints the translation backlog with --props-report", () => {
    const tree = tempTree({ version: 3, ...emptySections, props: {} }, '<Field label="Save" />');
    const result = run(tree, "--props-report");
    expect(result.status).toBe(0);
    expect(result.output).toContain("screens/Sample.tsx:1\tlabel\tSave");
  });
});
