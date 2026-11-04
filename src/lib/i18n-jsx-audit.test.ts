import { execFileSync } from "node:child_process";
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { afterEach, describe, expect, it } from "vitest";

import { ATTRIBUTES, CONTENT_PATHS, OBJECT_KEYS, PROPS, scanObjects, scanSource } from "../../scripts/lib/i18n-jsx.mjs";

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

  it("reads a template attribute's fixed words", () => {
    // `alt={`Page ${page}`}` used to be invisible: the scan only looked at
    // plain strings, so a fixed English word inside a template shipped unseen.
    expect(scan("<img alt={`Page ${page}`} />").attributes.map((row) => row.text)).toEqual(["Page ${…}"]);
    expect(scan("<button title={`${name} (Ctrl+K)`} />").attributes.map((row) => row.text)).toEqual(["${…} (Ctrl+K)"]);
    // A template whose words are only placeholders is not prose.
    expect(scan("<img alt={`${page}`} />").attributes).toHaveLength(0);
  });

  it("counts a braced string on an attribute too", () => {
    expect(scan('<input title={"Help"} />').attributes.map((row) => row.text)).toEqual(["Help"]);
    expect(scan("<img alt={`Chart`} />").attributes.map((row) => row.text)).toEqual(["Chart"]);
  });

  it("ignores translated calls, variables and dynamic templates", () => {
    expect(scan('<Field label={t("a.b")} />').props).toHaveLength(0);
    expect(scan("<Field label={value} />").props).toHaveLength(0);
    // A template's fixed words count now; only a bare placeholder does not.
    expect(scan("<Field label={`Total ${count}`} />").props.map((row) => row.text)).toEqual(["Total ${…}"]);
    expect(scan("<Field label={`${count}`} />").props).toHaveLength(0);
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
    // Translated and computed values stay out; a conditional is covered by the
    // expression tests below, which is where a real phrase is read.
    expect(scan('<Field hint={t("a.b")} />').props).toHaveLength(0);
    expect(scan("<Field hint={value} />").props).toHaveLength(0);
    expect(scan("<Screen subtitle={label} />").props).toHaveLength(0);
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
  const v5 = { files: {}, attributes: {}, props: {}, objects: {}, literal: 0 };

  it("fails when a file grows above its props baseline", () => {
    const tree = tempTree({ version: 5, ...v5, props: {} }, '<Field label="Save" />');
    const result = run(tree, "--check");
    expect(result.status).toBe(1);
    expect(result.output).toContain("prop string");
  });

  it("accepts the file once the props baseline records it", () => {
    const tree = tempTree({ version: 5, ...v5, props: { "screens/Sample.tsx": 1 } }, '<Field label="Save" />');
    expect(run(tree, "--check").status).toBe(0);
  });

  it("refuses to raise a props count without --allow-raise, and raises it with the flag", () => {
    const tree = tempTree({ version: 5, ...v5, props: { "screens/Sample.tsx": 0 } }, '<Field label="Save" />');
    const refused = run(tree, "--update");
    expect(refused.status).toBe(1);
    expect(refused.output).toContain("may only shrink");

    expect(run(tree, "--update", "--allow-raise").status).toBe(0);
    const written = JSON.parse(readFileSync(tree.baseline, "utf8")) as {
      version: number;
      props: Record<string, number>;
    };
    expect(written.version).toBe(5);
    expect(written.props["screens/Sample.tsx"]).toBe(1);
  });

  it("asks for a re-record instead of passing when the baseline has no props section", () => {
    const tree = tempTree({ version: 2, files: {}, attributes: {} }, '<Field label="Save" />');
    const result = run(tree, "--check");
    expect(result.status).toBe(1);
    expect(result.output).toContain("no `props` section");
  });

  it("fails when a sibling prop grows above its baseline", () => {
    const tree = tempTree({ version: 5, ...v5, props: {} }, '<Field hint="Tarih girin" />');
    const result = run(tree, "--check");
    expect(result.status).toBe(1);
    expect(result.output).toContain("prop string");
    expect(result.output).toContain("screens/Sample.tsx");
  });

  it("names the sibling prop in the backlog report", () => {
    const tree = tempTree({ version: 5, ...v5, props: {} }, '<Screen subtitle="Ekranı anlat" />');
    const result = run(tree, "--props-report");
    expect(result.status).toBe(0);
    expect(result.output).toContain("screens/Sample.tsx:1\tsubtitle\tEkranı anlat");
  });

  it("keeps the baseline schema at v5 when the widened props are recorded", () => {
    const tree = tempTree({ version: 5, ...v5, props: {} }, '<Field hint="Tarih girin" />');
    expect(run(tree, "--update", "--allow-raise").status).toBe(0);
    const written = JSON.parse(readFileSync(tree.baseline, "utf8")) as Record<string, unknown>;
    // Widening PROPS changes what is counted, not how the baseline is shaped.
    expect(written.version).toBe(5);
    expect(Object.keys(written).sort()).toEqual(["attributes", "files", "literal", "objects", "props", "version"]);
  });

  it("prints the translation backlog with --props-report", () => {
    const tree = tempTree({ version: 5, ...v5, props: {} }, '<Field label="Save" />');
    const result = run(tree, "--props-report");
    expect(result.status).toBe(0);
    expect(result.output).toContain("screens/Sample.tsx:1\tlabel\tSave");
  });
});

describe("i18n expressions (E4.7)", () => {
  it("counts both branches of a conditional attribute and prop", () => {
    expect(scan('<Field label={flag ? "Kaydet" : "İptal"} />').props.map((row) => row.text)).toEqual([
      "Kaydet",
      "İptal",
    ]);
    expect(scan('<img alt={f ? "Grafik" : "Tablo"} />').attributes).toHaveLength(2);
    expect(scan('<img alt={a ? "Birinci" : b ? "İkinci" : "Üçüncü"} />').attributes).toHaveLength(3);
  });

  it("counts the literal fallback of ??, || and &&", () => {
    expect(scan('<Field label={name ?? "Adsız"} />').props.map((row) => row.text)).toEqual(["Adsız"]);
    expect(scan('<Field label={name || "Anonim"} />').props.map((row) => row.text)).toEqual(["Anonim"]);
    expect(scan('<Field label={busy && "Yükleniyor"} />').props.map((row) => row.text)).toEqual(["Yükleniyor"]);
    // The condition is a comparison, not prose.
    expect(scan('<Field label={mode === "edit" ? t("a.b") : t("c.d")} />').props).toHaveLength(0);
  });

  it("counts a string, template or conditional written as a JSX child", () => {
    expect(scan('<p>{"Merhaba"}</p>').texts.map((row) => row.text)).toEqual(["Merhaba"]);
    expect(scan("<p>{`${n} sayfa`}</p>").texts.map((row) => row.text)).toEqual(["${…} sayfa"]);
    expect(scan('<p>{ok ? "Tamam" : "Hata"}</p>').texts.map((row) => row.text)).toEqual(["Tamam", "Hata"]);
  });

  it("never reads a condition, a call, a class name or a separator as prose", () => {
    expect(scan('<p>{t(f ? "a.b" : "c.d")}</p>').texts).toHaveLength(0);
    expect(scan('<div className={x ? "btn active" : "btn"} />').texts).toHaveLength(0);
    expect(scan('<p>{items.join(", ")}</p>').texts).toHaveLength(0);
    // `+` builds a string instead of choosing one.
    expect(scan('<p>{"Ad" + soyad}</p>').texts).toHaveLength(0);
    // A JSX child inside a branch is counted once, as a text node.
    expect(scan("<p>{x ? <b>Kalın</b> : null}</p>").texts).toHaveLength(1);
  });

  it("reads the literals inside a template's interpolations", () => {
    // The shell is placeholders only, but the branch words are prose.
    expect(scan('<img alt={`${n} ${n === 1 ? "page" : "pages"}`} />').attributes.map((row) => row.text)).toEqual([
      "page",
      "pages",
    ]);
    expect(scan('<img alt={`${n} ${n === 1 ? "Sayfa" : "Sayfalar"}`} />').attributes.map((row) => row.text)).toEqual([
      "Sayfa",
      "Sayfalar",
    ]);
    expect(scan('<img alt={`Sayfa ${f ? "ön" : "arka"}`} />').attributes.map((row) => row.text)).toEqual([
      "Sayfa ${…}",
      "ön",
      "arka",
    ]);
  });

  it("does not count an attribute expression as a child too", () => {
    const found = scan('<p title={x ? "Yardım" : "Bilgi"} />');
    expect(found.attributes).toHaveLength(2);
    expect(found.texts).toHaveLength(0);
  });
});

describe("i18n deliberate literals (E4.7)", () => {
  it("excludes an attribute marked by a comment on the line above", () => {
    const source = ["// i18n-literal: formula syntax example, not UI copy", '<Field label="Column" />'].join("\n");
    const found = scan(source);
    expect(found.props).toHaveLength(0);
    expect(found.ignored).toHaveLength(1);
    expect(found.ignored[0]).toMatchObject({
      kind: "prop",
      name: "label",
      text: "Column",
      reason: "formula syntax example, not UI copy",
    });
  });

  it("excludes a JSX child marked by a comment above it", () => {
    const source = ["<p>", "  {/* i18n-literal: shortcut hint, rendered as typed */}", '  {"Ctrl+S"}', "</p>"].join(
      "\n",
    );
    const found = scan(source);
    expect(found.texts).toHaveLength(0);
    expect(found.ignored[0].kind).toBe("text");
  });

  it("excludes a value marked by a trailing comment on the same line", () => {
    const found = scan('<Field label="SUM" /> // i18n-literal: Calc function name, never translated');
    expect(found.props).toHaveLength(0);
    expect(found.ignored[0].line).toBe(1);
  });

  it("keeps the marker's reason in the report so a reviewer can judge it", () => {
    const source = ["// i18n-literal: a unit, the table header already translates it", '<img alt="pt" />'].join("\n");
    expect(scan(source).ignored[0].reason).toBe("a unit, the table header already translates it");
  });

  it("rejects a marker without a reason and keeps counting the text", () => {
    const source = ["// i18n-literal:", '<Field label="Column" />'].join("\n");
    const found = scan(source);
    expect(found.props).toHaveLength(1);
    expect(found.invalidMarkers).toEqual([{ line: 1, problem: "missing-reason" }]);
  });

  it("rejects a placeholder reason such as todo or a two-word note", () => {
    for (const reason of ["todo", "FIXME", "ok"]) {
      const source = [`// i18n-literal: ${reason}`, '<Field label="Column" />'].join("\n");
      const found = scan(source);
      expect(found.props, reason).toHaveLength(1);
      expect(found.invalidMarkers[0].problem, reason).toBe("weak-reason");
    }
  });

  it("reports a marker that binds to no finding", () => {
    // The marker stayed behind when the attribute moved: say so instead of
    // letting the number quietly stay higher than it needs to be.
    const source = ["// i18n-literal: this attribute moved away years ago", "", "const x = 1;"].join("\n");
    expect(scan(source).invalidMarkers).toEqual([{ line: 1, problem: "orphan" }]);
  });

  it("reports a marker whose line holds two different attributes", () => {
    const source = ["// i18n-literal: which one did I mean here?", '<Field label="Amount" hint="Quantity" />'].join(
      "\n",
    );
    expect(scan(source).invalidMarkers).toEqual([{ line: 1, problem: "ambiguous" }]);
  });

  it("binds a template and its expression as one finding", () => {
    // The structured-reference placeholder of TablesUi: one attribute, one
    // decision, even though the audit sees a template and a fallback in it.
    const source = [
      "// i18n-literal: structured reference, formula syntax",
      '<Field label={`=${name}[${col ?? "Column"}]`} />',
    ].join("\n");
    const found = scan(source);
    expect(found.props).toHaveLength(0);
    expect(found.invalidMarkers).toHaveLength(0);
    expect(found.ignored).toHaveLength(1);
  });

  it("does not let a marker reach past the next code line", () => {
    const source = [
      "// i18n-literal: only the first line is marked, not this one",
      '<Field label="Column" />',
      '<Field label="Amount" />',
    ].join("\n");
    const found = scan(source);
    expect(found.props.map((row) => row.text)).toEqual(["Amount"]);
  });

  it("treats a // line inside JSX children as text, not as a marker", () => {
    const source = ["<p>", "  // i18n-literal: not a comment out here", "</p>"].join("\n");
    const found = scan(source);
    expect(found.invalidMarkers).toEqual([{ line: 2, problem: "in-jsx-text" }]);
  });

  it("keeps the two pragmas in their own scans", () => {
    // `i18n-content:` speaks about data, `i18n-literal:` about JSX syntax;
    // neither silences the other.
    const objectSource = ["// i18n-literal: JSX only", 'export const table = { label: "Çalışan" };'].join("\n");
    const objectScan = scanObjects("src/table.ts", objectSource);
    expect(objectScan.counted).toHaveLength(1);
  });
});

describe("i18n object strings (E4.6)", () => {
  const scanObject = (body: string, name = "lib/Sample.ts") => scanObjects(name, body);

  it("counts the interface text an object literal holds, in a .ts module too", () => {
    expect(scanObject('const o = { value: "employee", label: "Çalışan" };').counted.map((r) => r.text)).toEqual([
      "Çalışan",
    ]);
    expect(scanObject('const o = { label: "Çalışan" };', "screens/Sample.tsx").counted).toHaveLength(1);
    expect(scanObject('const o = { "label": "Çalışan" };').counted).toHaveLength(1);
  });

  it("leaves product names and identifiers to the info list", () => {
    const found = scanObject('const o = { label: "DeepSeek", title: "Letter" };');
    expect(found.counted).toHaveLength(0);
    expect(found.unmarked.map((r) => r.text)).toEqual(["DeepSeek", "Letter"]);
    // The narrow rule is deliberate: ASCII-only Turkish slips through, visibly.
    expect(scanObject('const o = { title: "Belge" };').counted).toHaveLength(0);
  });

  it("reads a template as one string with its placeholders", () => {
    const found = scanObject("const o = { description: `Belgede ${n} adet geçerli TCKN tespit edildi.` };");
    expect(found.counted.map((r) => r.text)).toEqual(["Belgede ${…} adet geçerli TCKN tespit edildi."]);
  });

  it("peels as, satisfies and parentheses, and splits a conditional", () => {
    expect(scanObject('const o = { label: "Çalışan" as const };').counted).toHaveLength(1);
    expect(scanObject("const o = { label: ('Ağ') };").counted).toHaveLength(1);
    expect(scanObject('const o = { label: flag ? "Geçerli" : "Geçersiz" };').counted).toHaveLength(2);
  });

  it("ignores translation calls, concatenation, computed keys and unlisted keys", () => {
    expect(scanObject('const o = { label: t("a.b"), title: "Ad" + suffix };').counted).toHaveLength(0);
    expect(scanObject('const o = { [key]: "Çalışan" };').counted).toHaveLength(0);
    expect(scanObject('const o = { name: "Çalışan", value: "çalışan", message: "Çalışan yok" };').counted).toHaveLength(
      0,
    );
  });

  it("honours an i18n-content pragma, and refuses one without a reason", () => {
    const excluded = scanObject('const o = {\n  // i18n-content: fixture text\n  title: "Değerlendirme",\n};');
    expect(excluded.counted).toHaveLength(0);
    expect(excluded.excluded[0].reason).toBe("fixture text");
    // An empty reason hides nothing, so the string stays counted.
    expect(scanObject('const o = {\n  // i18n-content:\n  title: "Değerlendirme",\n};').counted).toHaveLength(1);
  });

  it("lets one pragma cover the declaration that holds the entries", () => {
    const body = '// i18n-content: language autonyms\nconst LANGS = [\n  { code: "tr", label: "Türkçe" },\n];';
    const found = scanObjects("lib/languages.ts", body);
    expect(found.counted).toHaveLength(0);
    expect(found.excluded.map((r) => r.text)).toEqual(["Türkçe"]);
  });

  it("skips a whole file that writes the document instead of the interface", () => {
    expect(CONTENT_PATHS.length).toBeGreaterThan(0);
    for (const entry of CONTENT_PATHS) {
      const found = scanObjects(entry.path, 'const o = { title: "Gereği:" };');
      expect(found.contentFile?.reason).toBe(entry.reason);
      expect(found.counted).toHaveLength(0);
    }
  });

  it("keeps the measured keys explicit", () => {
    expect(OBJECT_KEYS).toEqual([
      "label",
      "title",
      "description",
      "hint",
      "subtitle",
      "placeholder",
      "tooltip",
      "detail",
    ]);
    for (const name of ["name", "value", "message"]) expect(OBJECT_KEYS).not.toContain(name);
  });

  it("asks for a re-record instead of passing when the baseline has no objects section", () => {
    const tree = tempTree({ version: 3, files: {}, attributes: {}, props: {} }, 'const o = { label: "Çalışan" };');
    const result = run(tree, "--check");
    expect(result.status).toBe(1);
    expect(result.output).toContain("no `objects` section");
  });

  it("fails when an object string grows above its baseline", () => {
    const tree = tempTree(
      { version: 5, files: {}, attributes: {}, props: {}, objects: {}, literal: 0 },
      'const o = { label: "Çalışan" };',
    );
    const result = run(tree, "--check");
    expect(result.status).toBe(1);
    expect(result.output).toContain("object string");
  });

  it("prints the object backlog in three sections", () => {
    const tree = tempTree(
      { version: 5, files: {}, attributes: {}, props: {}, objects: {}, literal: 0 },
      'const o = { label: "Çalışan", title: "DeepSeek" };',
    );
    const result = run(tree, "--objects-report");
    expect(result.status).toBe(0);
    expect(result.output).toContain("counted: 1 object string(s)");
    expect(result.output).toContain("screens/Sample.tsx:1\tlabel\tÇalışan");
    expect(result.output).toContain("unmarked (info only): 1");
  });
});

describe("toast details in the object scan", () => {
  it("counts the second line of a toast, and not its title", () => {
    const found = scanObjects("lib/toast.ts", 'push({ title: t("a.b"), detail: "Dosya açılamadı" });');
    expect(found.counted.map((row) => row.key)).toEqual(["detail"]);
    expect(found.counted[0].text).toBe("Dosya açılamadı");
  });

  it("keeps a template's placeholders", () => {
    const found = scanObjects("lib/toast.ts", "push({ detail: `${path} açılamadı` });");
    expect(found.counted.map((row) => row.text)).toEqual(["${…} açılamadı"]);
  });

  it("ignores a detail that is computed rather than written", () => {
    for (const body of [
      "push({ detail: String(err) });",
      "push({ detail: err.message });",
      "push({ detail: path });",
    ]) {
      expect(scanObjects("lib/toast.ts", body).counted).toHaveLength(0);
    }
  });

  it("leaves an ASCII path to the info list", () => {
    const found = scanObjects("lib/toast.ts", 'push({ detail: "C:/out/report.pdf" });');
    expect(found.counted).toHaveLength(0);
    expect(found.unmarked.map((row) => row.key)).toEqual(["detail"]);
  });

  it("still honours the content pragma on a detail", () => {
    const found = scanObjects(
      "lib/toast.ts",
      'push({\n  // i18n-content: log line, not interface text\n  detail: "Günlük yazıldı",\n});',
    );
    expect(found.counted).toHaveLength(0);
    expect(found.excluded[0].reason).toBe("log line, not interface text");
  });

  it("leaves message out, even next to a counted detail", () => {
    const found = scanObjects("lib/toast.ts", 'push({ detail: "Ağ hatası", message: "Ağ hatası" });');
    expect(found.counted.map((row) => row.key)).toEqual(["detail"]);
  });
});
