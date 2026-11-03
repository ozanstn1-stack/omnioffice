import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { afterEach, describe, expect, it } from "vitest";

// F-Droid and IzzyOnDroid read `fastlane/metadata/android/<locale>/changelogs/
// <versionCode>.txt`. The English body is generated from CHANGELOG.md, the other
// locales are hand-maintained, so the check is what keeps a translated listing
// from quietly showing the notes of the version before it.
const root = path.resolve(__dirname, "..", "..");
const script = path.join(root, "scripts", "fastlane-changelogs.mjs");
const cleanups: string[] = [];

afterEach(() => {
  for (const directory of cleanups.splice(0)) rmSync(directory, { recursive: true, force: true });
});

const CHANGELOG = `# Changelog

## [7.5.0] - Newest release

### Added

- **A newest thing:** one sentence about it.

## [7.4.0] - Older release

### Added

- **An older thing:** another sentence.
`;

/** A fixture tree: CHANGELOG.md plus one directory per locale. */
function fixture(locales: string[]): string {
  const directory = mkdtempSync(path.join(tmpdir(), "fastlane-"));
  cleanups.push(directory);
  writeFileSync(path.join(directory, "CHANGELOG.md"), CHANGELOG);
  for (const locale of locales) {
    mkdirSync(path.join(directory, "fastlane", "metadata", "android", locale, "changelogs"), {
      recursive: true,
    });
  }
  return directory;
}

function translated(directory: string, locale: string, code: number, body = "elle yazıldı\n"): void {
  writeFileSync(path.join(directory, "fastlane", "metadata", "android", locale, "changelogs", `${code}.txt`), body);
}

function run(directory: string, ...flags: string[]): { status: number; output: string } {
  try {
    const output = execFileSync(process.execPath, [script, ...flags], {
      encoding: "utf8",
      env: { ...process.env, FASTLANE_ROOT: directory },
    });
    return { status: 0, output };
  } catch (caught) {
    const error = caught as { status?: number; stdout?: string; stderr?: string };
    return { status: error.status ?? 1, output: `${error.stdout ?? ""}${error.stderr ?? ""}` };
  }
}

describe("fastlane changelogs", () => {
  it("writes the English body from the changelog section", () => {
    const directory = fixture(["en-US", "tr-TR"]);
    expect(run(directory).status).toBe(0);
    const body = readFileSync(
      path.join(directory, "fastlane", "metadata", "android", "en-US", "changelogs", "7005000.txt"),
      "utf8",
    );
    expect(body).toContain("7.5.0: A newest thing");
    expect(body).not.toContain("An older thing");
  });

  it("demands the newest version, not the oldest, from a translated locale", () => {
    // The regression this pins: the checker used the last CHANGELOG entry as
    // "the current version", which is the oldest one, so a translated locale
    // could miss the new release and still pass.
    const directory = fixture(["en-US", "tr-TR"]);
    run(directory);
    translated(directory, "tr-TR", 7004000);
    const checked = run(directory, "--check");
    expect(checked.status).toBe(1);
    expect(checked.output).toContain("tr-TR/changelogs/7005000.txt is missing");
  });

  it("passes once the translated locale carries the newest version", () => {
    const directory = fixture(["en-US", "tr-TR"]);
    run(directory);
    translated(directory, "tr-TR", 7004000);
    translated(directory, "tr-TR", 7005000);
    expect(run(directory, "--check").status).toBe(0);
  });

  it("never overwrites a hand-written translated changelog", () => {
    const directory = fixture(["en-US", "tr-TR"]);
    translated(directory, "tr-TR", 7005000, "elle yazılmış sürüm notu\n");
    run(directory);
    expect(
      readFileSync(
        path.join(directory, "fastlane", "metadata", "android", "tr-TR", "changelogs", "7005000.txt"),
        "utf8",
      ),
    ).toBe("elle yazılmış sürüm notu\n");
  });
});
