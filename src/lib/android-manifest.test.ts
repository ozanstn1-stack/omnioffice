import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

/**
 * Guards the Android "Open with" / share intent filters on every PR (the
 * Gradle tests in ManifestIntentFiltersTest only run in the tag-triggered
 * Android workflow).
 *
 * Android evaluates a <data android:pathPattern> only when the same filter has
 * a scheme and a host, and a filter with neither a scheme nor a type matches
 * intents without data only. The file associations Tauri generated were bare
 * path patterns, so no file manager ever matched them.
 */
const read = (relative: string) => readFileSync(fileURLToPath(new URL(relative, import.meta.url)), "utf8");
const manifestXml = read("../../src-tauri/gen/android/app/src/main/AndroidManifest.xml");

interface Filter {
  actions: string[];
  schemes: string[];
  hosts: string[];
  mimeTypes: string[];
  paths: string[];
}

function mainActivityFilters(): Filter[] {
  const document = new DOMParser().parseFromString(manifestXml, "application/xml");
  expect(document.querySelector("parsererror")).toBeNull();
  const activity = Array.from(document.getElementsByTagName("activity")).find(
    (element) => element.getAttribute("android:name") === ".MainActivity",
  );
  expect(activity).toBeDefined();
  return Array.from(activity!.getElementsByTagName("intent-filter")).map((filter) => {
    const attributes = (tag: string, name: string) =>
      Array.from(filter.getElementsByTagName(tag))
        .map((element) => element.getAttribute(`android:${name}`))
        .filter((value): value is string => Boolean(value));
    return {
      actions: attributes("action", "name"),
      schemes: attributes("data", "scheme"),
      hosts: attributes("data", "host"),
      mimeTypes: attributes("data", "mimeType"),
      paths: [...attributes("data", "pathPattern"), ...attributes("data", "pathSuffix")],
    };
  });
}

const VIEW = "android.intent.action.VIEW";
const SEND = "android.intent.action.SEND";
const SEND_MULTIPLE = "android.intent.action.SEND_MULTIPLE";

const TYPES: Record<string, string[]> = {
  docx: ["application/vnd.openxmlformats-officedocument.wordprocessingml.document"],
  odt: ["application/vnd.oasis.opendocument.text"],
  rtf: ["application/rtf", "text/rtf"],
  xlsx: ["application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"],
  ods: ["application/vnd.oasis.opendocument.spreadsheet"],
  pptx: ["application/vnd.openxmlformats-officedocument.presentationml.presentation"],
  odp: ["application/vnd.oasis.opendocument.presentation"],
  pdf: ["application/pdf"],
  oswk: ["application/x-oswk"],
};

describe("Android intent filters", () => {
  const filters = mainActivityFilters();

  it("never declares a path without a scheme and a host", () => {
    const withPath = filters.filter((filter) => filter.paths.length > 0);
    expect(withPath.length).toBeGreaterThan(0);
    for (const filter of withPath) {
      expect(filter.schemes).toEqual(expect.arrayContaining(["content", "file"]));
      expect(filter.hosts).toContain("*");
    }
  });

  it("never pairs a specific MIME type with a path", () => {
    // Type and path would both have to match, and content:// paths usually carry no extension.
    for (const filter of filters.filter((entry) => entry.paths.length > 0)) {
      expect(filter.mimeTypes.filter((type) => type !== "*/*")).toEqual([]);
    }
  });

  it("opens and shares every supported type through one typed filter without a scheme", () => {
    const typed = filters.filter((filter) => filter.actions.includes(SEND));
    expect(typed).toHaveLength(1);
    const [filter] = typed;
    expect(filter.actions).toEqual(expect.arrayContaining([VIEW, SEND, SEND_MULTIPLE]));
    // A share intent has no data URI, so a scheme here would stop it from ever matching.
    expect(filter.schemes).toEqual([]);
    expect(filter.paths).toEqual([]);
    for (const [extension, types] of Object.entries(TYPES)) {
      for (const type of types) expect(filter.mimeTypes, `${extension}: ${type}`).toContain(type);
    }
  });

  it("matches by file name for generic and missing types", () => {
    const byName = filters.filter((filter) => filter.paths.length > 0);
    expect(byName.map((filter) => filter.mimeTypes.join(",")).sort()).toEqual(["", "*/*"]);
    for (const filter of byName) {
      for (const extension of Object.keys(TYPES)) {
        expect(filter.paths, extension).toContain(`.${extension}`);
        expect(filter.paths, extension).toContain(`.*\\\\.${extension}`);
      }
    }
  });

  it("leaves the manifest to us: tauri-build must not regenerate the associations", () => {
    expect(manifestXml).not.toContain("tauri-file-associations");
    const config = JSON.parse(read("../../src-tauri/tauri.android.conf.json"));
    expect(config.bundle.fileAssociations).toEqual([]);
    // The desktop config still declares them for the Windows installer.
    const desktop = JSON.parse(read("../../src-tauri/tauri.conf.json"));
    expect(desktop.bundle.fileAssociations.length).toBeGreaterThan(0);
  });
});
