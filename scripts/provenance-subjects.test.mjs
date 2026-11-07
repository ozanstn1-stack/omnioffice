// Tests for the release provenance subjects: the hashes a build job takes, the
// merged list the generator receives, and the two checks that keep a published
// release from drifting away from it. Every case builds its own directory, so
// nothing is committed as a fixture.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const SCRIPT = join(dirname(fileURLToPath(import.meta.url)), "provenance-subjects.mjs");

/** Runs the script and returns its exit code with the captured output. */
function run(args) {
  try {
    const stdout = execFileSync(process.execPath, [SCRIPT, ...args], { encoding: "utf8" });
    return { code: 0, stdout, stderr: "" };
  } catch (error) {
    return { code: error.status ?? 1, stdout: error.stdout ?? "", stderr: error.stderr ?? "" };
  }
}

function workspace() {
  return mkdtempSync(join(tmpdir(), "provenance-"));
}

function write(directory, name, body) {
  const path = join(directory, name);
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, body);
  return path;
}

function sha256(body) {
  return createHash("sha256").update(body).digest("hex");
}

function hashLines(directory, names) {
  const path = join(directory, `hashes-${Math.random().toString(16).slice(2)}.txt`);
  const lines = names.map((name) => `${sha256(readFileSync(join(directory, name)))}  ${name}`);
  writeFileSync(path, `${lines.join("\n")}\n`);
  return path;
}

/** An in-toto envelope naming the given name/digest pairs. */
function envelope(subjects, type = "https://in-toto.io/Statement/v1") {
  const statement = { _type: type, subject: subjects.map(([name, digest]) => ({ name, digest: { sha256: digest } })) };
  const payload = Buffer.from(JSON.stringify(statement), "utf8").toString("base64");
  return `${JSON.stringify({ payloadType: "application/vnd.in-toto+json", payload })}\n`;
}

test("hash writes sha256sum lines with the file's own name", () => {
  const directory = workspace();
  const path = write(directory, "a.bin", "abc");
  const result = run(["hash", "--paths-json", JSON.stringify([path])]);
  assert.equal(result.code, 0, result.stderr);
  assert.equal(result.stdout.trim(), `${sha256("abc")}  a.bin`);
});

test("hash --dir digests every file in a directory, sorted", () => {
  const directory = workspace();
  write(directory, "b.zip", "b");
  write(directory, "a.exe", "a");
  const result = run(["hash", "--dir", directory]);
  assert.equal(result.code, 0, result.stderr);
  assert.deepEqual(
    result.stdout
      .trim()
      .split("\n")
      .map((line) => line.slice(66)),
    ["a.exe", "b.zip"],
  );
  assert.equal(run(["hash", "--dir", join(directory, "missing")]).code, 2);
});

test("hash refuses an empty array, a non-array, a non-string entry, a missing file and a directory", () => {
  const directory = workspace();
  const file = write(directory, "a.bin", "abc");
  // A malformed argument is a usage error; a path that is not a usable file is
  // a failed check, so the build job says which file it is unhappy about.
  for (const argument of ["[]", "{}", JSON.stringify([42])]) {
    const result = run(["hash", "--paths-json", argument]);
    assert.equal(result.code, 2, `${argument} was accepted`);
    assert.match(result.stderr, /provenance-subjects:/);
  }
  for (const argument of [JSON.stringify([join(directory, "gone.bin")]), JSON.stringify([directory])]) {
    const result = run(["hash", "--paths-json", argument]);
    assert.equal(result.code, 1, `${argument} was accepted`);
    assert.match(result.stderr, /provenance-subjects:/);
  }
  assert.equal(run(["hash", "--paths-json", JSON.stringify([file])]).code, 0);
});

test("encode merges, sorts and de-duplicates", () => {
  const directory = workspace();
  write(directory, "b.zip", "b");
  write(directory, "a.exe", "a");
  const first = hashLines(directory, ["b.zip"]);
  const second = hashLines(directory, ["a.exe", "b.zip"]);
  const result = run(["encode", first, second]);
  assert.equal(result.code, 0, result.stderr);
  const text = Buffer.from(result.stdout.trim(), "base64").toString("utf8").trim();
  assert.deepEqual(
    text.split("\n").map((line) => line.slice(66)),
    ["a.exe", "b.zip"],
    "sorted by name, one line each",
  );
});

test("encode is deterministic whatever order the files come in", () => {
  const directory = workspace();
  write(directory, "a.exe", "a");
  write(directory, "b.zip", "b");
  const first = hashLines(directory, ["a.exe"]);
  const second = hashLines(directory, ["b.zip"]);
  const forward = run(["encode", first, second]).stdout.trim();
  const backward = run(["encode", second, first]).stdout.trim();
  assert.equal(forward, backward);
});

test("encode tolerates CRLF, the binary marker, upper case hex and a path prefix", () => {
  const directory = workspace();
  const digest = sha256("x").toUpperCase();
  const path = join(directory, "h.txt");
  writeFileSync(path, `${digest} *C:\\build\\release\\x.exe\r\n`);
  const result = run(["encode", path]);
  assert.equal(result.code, 0, result.stderr);
  const text = Buffer.from(result.stdout.trim(), "base64").toString("utf8").trim();
  assert.equal(text, `${digest.toLowerCase()}  x.exe`);
});

test("encode names the file and line of a malformed entry", () => {
  const directory = workspace();
  const path = join(directory, "h.txt");
  writeFileSync(path, `${sha256("a")}  a.exe\nnot a hash line\n`);
  const result = run(["encode", path]);
  assert.equal(result.code, 1);
  assert.match(result.stderr, /h\.txt:2/);
});

test("encode refuses one name with two digests", () => {
  const directory = workspace();
  write(directory, "a.exe", "a");
  const first = hashLines(directory, ["a.exe"]);
  const path = join(directory, "other.txt");
  writeFileSync(path, `${sha256("different")}  a.exe\n`);
  assert.equal(run(["encode", first, path]).code, 1);
});

test("encode skips latest.json and any envelope", () => {
  const directory = workspace();
  const path = join(directory, "h.txt");
  writeFileSync(
    path,
    `${sha256("l")}  latest.json\n${sha256("e")}  omnioffice-v1.intoto.jsonl\n${sha256("a")}  a.exe\n`,
  );
  const result = run(["encode", path]);
  assert.equal(result.code, 0, result.stderr);
  const text = Buffer.from(result.stdout.trim(), "base64").toString("utf8");
  assert.ok(text.includes("a.exe"));
  assert.ok(!text.includes("latest.json"));
  assert.ok(!text.includes(".intoto.jsonl"));
});

test("encode fails when only excluded files are listed", () => {
  const directory = workspace();
  const path = join(directory, "h.txt");
  writeFileSync(path, `${sha256("l")}  latest.json\n`);
  assert.equal(run(["encode", path]).code, 1);
});

test("encode round-trips: the decoded output is sha256sum text", () => {
  const directory = workspace();
  write(directory, "a.exe", "a");
  const path = hashLines(directory, ["a.exe"]);
  const encoded = run(["encode", path]).stdout.trim();
  const text = Buffer.from(encoded, "base64").toString("utf8");
  assert.match(text, /^[0-9a-f]{64} {2}a\.exe\n$/);
});

test("check-published passes when the digests are on the release", () => {
  const directory = workspace();
  write(directory, "a.exe", "installer");
  const subjects = run(["encode", hashLines(directory, ["a.exe"])]).stdout.trim();
  const result = run(["check-published", "--subjects-b64", subjects, "--release-dir", directory]);
  assert.equal(result.code, 0, result.stderr);
});

test("check-published names a tampered asset", () => {
  const directory = workspace();
  const path = write(directory, "a.exe", "installer");
  const subjects = run(["encode", hashLines(directory, ["a.exe"])]).stdout.trim();
  writeFileSync(path, "installer with one byte more");
  const result = run(["check-published", "--subjects-b64", subjects, "--release-dir", directory]);
  assert.equal(result.code, 1);
  assert.match(result.stderr, /a\.exe is not on the release/);
});

test("check-published ignores an asset the subjects do not mention", () => {
  const directory = workspace();
  write(directory, "a.exe", "installer");
  const subjects = run(["encode", hashLines(directory, ["a.exe"])]).stdout.trim();
  write(directory, "another-job.zip", "other");
  assert.equal(run(["check-published", "--subjects-b64", subjects, "--release-dir", directory]).code, 0);
});

test("check-published refuses empty or unusable input", () => {
  const directory = workspace();
  write(directory, "a.exe", "installer");
  assert.equal(run(["check-published", "--subjects-b64", "", "--release-dir", directory]).code, 1);
  assert.equal(run(["check-published", "--subjects-b64", "!!", "--release-dir", directory]).code, 1);
  assert.equal(run(["check-published", "--subjects-b64", "eA=="]).code, 2, "a missing --release-dir is a usage error");
});

test("coverage passes for both statement versions", () => {
  const directory = workspace();
  write(directory, "a.exe", "installer");
  write(directory, "b.zip", "portable");
  const pairs = [
    ["a.exe", sha256("installer")],
    ["b.zip", sha256("portable")],
  ];
  const elsewhere = workspace();
  for (const type of ["https://in-toto.io/Statement/v1", "https://in-toto.io/Statement/v0.1"]) {
    const path = join(elsewhere, `env-${type.endsWith("v1") ? "v1" : "v01"}.jsonl`);
    writeFileSync(path, envelope(pairs, type));
    const result = run(["coverage", "--release-dir", directory, path]);
    assert.equal(result.code, 0, `${type}: ${result.stderr}`);
  }
});

test("coverage reads a Sigstore bundle and a bare statement too", () => {
  const directory = workspace();
  write(directory, "a.exe", "installer");
  const statement = {
    _type: "https://in-toto.io/Statement/v1",
    subject: [{ name: "a.exe", digest: { sha256: sha256("installer") } }],
  };
  const payload = Buffer.from(JSON.stringify(statement), "utf8").toString("base64");
  const elsewhere = workspace();
  const bundle = join(elsewhere, "bundle.jsonl");
  writeFileSync(
    bundle,
    `${JSON.stringify({ mediaType: "application/vnd.dev.sigstore.bundle.v0.3+json", dsseEnvelope: { payloadType: "application/vnd.in-toto+json", payload } })}\n`,
  );
  assert.equal(run(["coverage", "--release-dir", directory, bundle]).code, 0, run(["coverage", "--release-dir", directory, bundle]).stderr);
  const raw = join(elsewhere, "statement.json");
  writeFileSync(raw, `${JSON.stringify(statement)}\n`);
  assert.equal(run(["coverage", "--release-dir", directory, raw]).code, 0);
});

test("coverage names an asset no envelope covers", () => {
  const directory = workspace();
  write(directory, "a.exe", "installer");
  write(directory, "b.zip", "portable");
  const path = join(directory, "env.jsonl");
  writeFileSync(path, envelope([["a.exe", sha256("installer")]]));
  const result = run(["coverage", "--release-dir", directory, path]);
  assert.equal(result.code, 1);
  assert.match(result.stderr, /b\.zip is published without provenance/);
});

test("coverage ignores latest.json and the envelopes themselves", () => {
  const directory = workspace();
  write(directory, "a.exe", "installer");
  write(directory, "latest.json", "{}");
  // A downloaded envelope that lands in the release directory is not an asset.
  const path = write(directory, "omnioffice-v1.intoto.jsonl", envelope([["a.exe", sha256("installer")]]));
  assert.equal(run(["coverage", "--release-dir", directory, path]).code, 0);
});

test("coverage refuses a stale digest, a broken envelope and an unknown statement", () => {
  const directory = workspace();
  write(directory, "a.exe", "installer");
  const stale = envelope([["a.exe", sha256("an older build")]]);
  writeFileSync(join(directory, "stale.jsonl"), stale);
  assert.match(
    run(["coverage", "--release-dir", directory, join(directory, "stale.jsonl")]).stderr,
    /without provenance/,
  );

  const broken = join(directory, "broken.jsonl");
  writeFileSync(broken, "{not json\n");
  assert.equal(run(["coverage", "--release-dir", directory, broken]).code, 1);

  const wrongType = join(directory, "type.jsonl");
  writeFileSync(wrongType, `${JSON.stringify({ payloadType: "text/plain", payload: "eA==" })}\n`);
  assert.equal(run(["coverage", "--release-dir", directory, wrongType]).code, 1);

  const badPayload = join(directory, "payload.jsonl");
  writeFileSync(badPayload, `${JSON.stringify({ payloadType: "application/vnd.in-toto+json", payload: "!!" })}\n`);
  assert.equal(run(["coverage", "--release-dir", directory, badPayload]).code, 1);

  const noDigest = join(directory, "digest.jsonl");
  const statement = { _type: "https://in-toto.io/Statement/v1", subject: [{ name: "a.exe", digest: {} }] };
  const payload = Buffer.from(JSON.stringify(statement), "utf8").toString("base64");
  writeFileSync(noDigest, `${JSON.stringify({ payloadType: "application/vnd.in-toto+json", payload })}\n`);
  assert.equal(run(["coverage", "--release-dir", directory, noDigest]).code, 1);
});

test("an unknown subcommand is a usage error", () => {
  const result = run(["nonsense"]);
  assert.equal(result.code, 2);
  assert.match(result.stderr, /usage: provenance-subjects\.mjs/);
});
