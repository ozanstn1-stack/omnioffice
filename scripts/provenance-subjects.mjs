#!/usr/bin/env node
/**
 * The subject list for a release's build provenance, and the checks that keep
 * it honest.
 *
 * SLSA's generic generator takes one base64 blob of `sha256sum`-format lines
 * ("subjects") and writes a provenance envelope naming exactly those files. The
 * steps that produce it are easy to get subtly wrong - a file listed under the
 * wrong name, a digest taken before the last write, a stale envelope that no
 * longer matches what was published - and none of that is visible at release
 * time unless something checks it. This script is that something:
 *
 *   hash            digests the files a build job is about to upload
 *   encode          merges the per-job hash files into one subject list
 *   check-published compares the subjects against the assets on the release
 *   coverage        checks that an envelope covers every published asset
 *
 * Usage:
 *   node scripts/provenance-subjects.mjs hash (--paths-json '<json array>' | --dir <dir>)
 *   node scripts/provenance-subjects.mjs encode <hashfile>...
 *   node scripts/provenance-subjects.mjs check-published --subjects-b64 <b64> --release-dir <dir>
 *   node scripts/provenance-subjects.mjs coverage --release-dir <dir> <envelope.jsonl>...
 *
 * Exit codes: 0 ok, 1 a check failed, 2 bad arguments or an unreadable file.
 */

import { createHash } from "node:crypto";
import { createReadStream, readFileSync, readdirSync, statSync } from "node:fs";
import { basename, join } from "node:path";

/** `latest.json` is rewritten by every matrix job and is covered by the updater
 * signatures inside it, so it is not a provenance subject. */
const EXCLUDED = new Set(["latest.json"]);

/** Provenance envelopes are outputs of the generator, never its inputs. */
const ENVELOPE_SUFFIX = ".intoto.jsonl";

/** `sha256sum` output: the digest, two spaces or " *" for binary, then the name. */
const LINE = /^([0-9a-fA-F]{64}) [ *](.+)$/;

/** The digest of one file, streamed so a large installer does not sit in RAM. */
function digestOf(path) {
  return new Promise((resolve, reject) => {
    const hash = createHash("sha256");
    const stream = createReadStream(path);
    stream.on("error", reject);
    stream.on("data", (chunk) => hash.update(chunk));
    stream.on("end", () => resolve(hash.digest("hex")));
  });
}

/** The file name of a path from either platform: a build job on Windows writes
 * `C:\build\release\x.exe`, and `basename` only knows the forward slash. */
function nameOf(path) {
  return basename(path.replace(/\\/g, "/"));
}

function isExcluded(name) {
  return EXCLUDED.has(name) || name.endsWith(ENVELOPE_SUFFIX);
}

function fail(message, code = 1) {
  console.error(`provenance-subjects: ${message}`);
  return code;
}

/** `hash`: the `sha256sum` lines for the files a build job produced. */
async function commandHash(argv) {
  const at = argv.indexOf("--paths-json");
  const directoryAt = argv.indexOf("--dir");
  if (at < 0 && directoryAt < 0) {
    return fail("usage: hash (--paths-json '<json array>' | --dir <directory>)", 2);
  }
  let paths;
  if (directoryAt >= 0) {
    const directory = argv[directoryAt + 1];
    if (!directory) {
      return fail("usage: hash --dir <directory>", 2);
    }
    try {
      paths = readdirSync(directory)
        .sort()
        .map((name) => join(directory, name))
        .filter((path) => statSync(path).isFile());
    } catch {
      return fail(`the directory is unreadable: ${directory}`, 2);
    }
    if (paths.length === 0) {
      return fail(`the directory holds no files: ${directory}`);
    }
  } else {
    try {
      paths = JSON.parse(argv[at + 1]);
    } catch {
      return fail("--paths-json is not valid JSON", 2);
    }
    if (!Array.isArray(paths) || paths.length === 0) {
      return fail("--paths-json must be a non-empty array of paths", 2);
    }
  }
  const lines = [];
  for (const path of paths) {
    if (typeof path !== "string" || path.trim() === "") {
      return fail(`--paths-json holds a non-string entry: ${JSON.stringify(path)}`, 2);
    }
    let stats;
    try {
      stats = statSync(path);
    } catch {
      return fail(`the file does not exist: ${path}`);
    }
    if (!stats.isFile()) {
      return fail(`not a regular file: ${path}`);
    }
    lines.push(`${await digestOf(path)}  ${basename(path)}`);
  }
  console.log(lines.join("\n"));
  return 0;
}

/** `encode`: one merged, sorted, de-duplicated subject list as base64. */
function commandEncode(argv) {
  const files = argv.filter((argument) => !argument.startsWith("--"));
  if (files.length === 0) {
    return fail("usage: encode <hashfile>...", 2);
  }
  const byName = new Map();
  for (const file of files) {
    let text;
    try {
      text = readFileSync(file, "utf8");
    } catch {
      return fail(`the hash file is unreadable: ${file}`);
    }
    const lines = text.split("\n");
    for (const [index, raw] of lines.entries()) {
      const line = raw.replace(/\r$/, "");
      if (line.trim() === "") {
        continue;
      }
      const match = LINE.exec(line);
      if (!match) {
        return fail(`${file}:${index + 1}: not a sha256sum line: ${line}`);
      }
      const digest = match[1].toLowerCase();
      const name = nameOf(match[2].trim());
      if (isExcluded(name)) {
        console.error(`provenance-subjects: skipped ${name} (not a subject)`);
        continue;
      }
      const known = byName.get(name);
      if (known && known !== digest) {
        return fail(`${name} is listed with two different digests`);
      }
      byName.set(name, digest);
    }
  }
  if (byName.size === 0) {
    return fail("no subjects remain after the exclusions");
  }
  const text = [...byName.entries()]
    .sort(([left], [right]) => (left < right ? -1 : left > right ? 1 : 0))
    .map(([name, digest]) => `${digest}  ${name}`)
    .join("\n");
  console.log(Buffer.from(`${text}\n`, "utf8").toString("base64"));
  return 0;
}

/** Every file in a directory, with the digest of each. */
async function digestsIn(directory) {
  const entries = readdirSync(directory).sort();
  const digests = new Map();
  for (const name of entries) {
    const path = join(directory, name);
    if (!statSync(path).isFile()) {
      continue;
    }
    digests.set(name, await digestOf(path));
  }
  return digests;
}

/** `check-published`: the subjects must be the bytes that were published. */
async function commandCheckPublished(argv) {
  const subjectsAt = argv.indexOf("--subjects-b64");
  const dirAt = argv.indexOf("--release-dir");
  if (subjectsAt < 0 || dirAt < 0 || subjectsAt + 1 >= argv.length || dirAt + 1 >= argv.length) {
    return fail("usage: check-published --subjects-b64 <base64> --release-dir <dir>", 2);
  }
  const directory = argv[dirAt + 1];
  let text;
  try {
    text = Buffer.from(argv[subjectsAt + 1], "base64").toString("utf8");
  } catch {
    return fail("--subjects-b64 is not valid base64", 2);
  }
  const subjects = text
    .split("\n")
    .map((line) => LINE.exec(line.trim()))
    .filter(Boolean)
    .map((match) => ({ digest: match[1].toLowerCase(), name: nameOf(match[2].trim()) }));
  if (subjects.length === 0) {
    return fail("--subjects-b64 names no subject");
  }
  const published = await digestsIn(directory);
  const byDigest = new Map([...published.entries()].map(([name, digest]) => [digest, name]));
  let failed = false;
  for (const subject of subjects) {
    const name = byDigest.get(subject.digest);
    if (name === undefined) {
      console.error(`provenance-subjects: ${subject.name} is not on the release (digest ${subject.digest})`);
      failed = true;
    } else if (name !== subject.name) {
      console.error(`provenance-subjects: warning: ${subject.name} was published as ${name}`);
    }
  }
  if (failed) {
    return 1;
  }
  console.log(`provenance-subjects: ${subjects.length} subject(s) match the published assets`);
  return 0;
}

/** `coverage`: every published asset is named by one of the envelopes. */
async function commandCoverage(argv) {
  const dirAt = argv.indexOf("--release-dir");
  if (dirAt < 0 || dirAt + 1 >= argv.length) {
    return fail("usage: coverage --release-dir <dir> <envelope.jsonl>...", 2);
  }
  const directory = argv[dirAt + 1];
  const envelopes = argv.filter((argument, index) => !argument.startsWith("--") && index !== dirAt + 1);
  if (envelopes.length === 0) {
    return fail("coverage needs at least one envelope", 2);
  }
  const covered = new Set();
  for (const file of envelopes) {
    let text;
    try {
      text = readFileSync(file, "utf8");
    } catch {
      return fail(`the envelope is unreadable: ${file}`);
    }
    for (const [index, raw] of text.split("\n").entries()) {
      const line = raw.trim();
      if (line === "") {
        continue;
      }
      let bundle;
      try {
        bundle = JSON.parse(line);
      } catch {
        return fail(`${file}:${index + 1}: not a JSON line`);
      }
      if (bundle.payloadType !== "application/vnd.in-toto+json") {
        return fail(`${file}:${index + 1}: not an in-toto envelope`);
      }
      let statement;
      try {
        statement = JSON.parse(Buffer.from(bundle.payload, "base64").toString("utf8"));
      } catch {
        return fail(`${file}:${index + 1}: the payload is not base64 JSON`);
      }
      const type = statement._type;
      if (type !== "https://in-toto.io/Statement/v0.1" && type !== "https://in-toto.io/Statement/v1") {
        return fail(`${file}:${index + 1}: unknown statement type ${type}`);
      }
      for (const subject of statement.subject ?? []) {
        const digest = subject?.digest?.sha256;
        if (typeof digest !== "string" || !/^[0-9a-f]{64}$/.test(digest)) {
          return fail(`${file}:${index + 1}: a subject has no sha256 digest`);
        }
        covered.add(digest);
      }
    }
  }
  const published = await digestsIn(directory);
  let failed = false;
  for (const [name, digest] of published) {
    if (isExcluded(name)) {
      continue;
    }
    if (!covered.has(digest)) {
      console.error(`provenance-subjects: ${name} is published without provenance`);
      failed = true;
    }
  }
  if (failed) {
    return 1;
  }
  console.log(`provenance-subjects: every published asset is covered by ${envelopes.length} envelope(s)`);
  return 0;
}

function main(argv) {
  const [command, ...rest] = argv;
  switch (command) {
    case "hash":
      return commandHash(rest);
    case "encode":
      return commandEncode(rest);
    case "check-published":
      return commandCheckPublished(rest);
    case "coverage":
      return commandCoverage(rest);
    default:
      console.error("usage: provenance-subjects.mjs <hash|encode|check-published|coverage> ...");
      return 2;
  }
}

process.exit(await main(process.argv.slice(2)));
