import assert from "node:assert/strict";
import { existsSync, mkdirSync, mkdtempSync, rmSync, utimesSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, test } from "node:test";

import { cleanStorage, formatBytes } from "./cleanup-storage.mjs";

const sandboxes = [];
afterEach(() => {
  for (const dir of sandboxes.splice(0)) {
    rmSync(dir, { recursive: true, force: true });
  }
});

function createSandbox() {
  const root = mkdtempSync(join(tmpdir(), "cleanup-test-repo-"));
  const fakeTmp = mkdtempSync(join(tmpdir(), "cleanup-test-tmp-"));
  const fakeHome = mkdtempSync(join(tmpdir(), "cleanup-test-home-"));
  sandboxes.push(root, fakeTmp, fakeHome);
  return { root, fakeTmp, fakeHome };
}

test("formatBytes prints human-readable storage sizes", () => {
  assert.equal(formatBytes(0), "0 B");
  assert.equal(formatBytes(512), "512 B");
  assert.equal(formatBytes(2048), "2.0 KB");
  assert.equal(formatBytes(5 * 1024 * 1024), "5.0 MB");
});

test("dry-run reports cleanable bytes without deleting anything", () => {
  const { root, fakeTmp, fakeHome } = createSandbox();
  mkdirSync(join(root, "dist"), { recursive: true });
  mkdirSync(join(root, "src"), { recursive: true });
  writeFileSync(join(root, "dist", "bundle.js"), "x".repeat(4096));
  writeFileSync(join(root, "src", "main.ts"), "console.log('keep');");

  const report = cleanStorage({ root, tmpDir: fakeTmp, homeDir: fakeHome, dryRun: true });
  assert.equal(report.dryRun, true);
  assert.equal(report.removedCount, 1);
  assert.equal(report.freedBytes, 4096);
  assert.ok(existsSync(join(root, "dist", "bundle.js")));
  assert.ok(existsSync(join(root, "src", "main.ts")));
});

test("cleans build artifacts, honors --keep-target, and never deletes source files", () => {
  const { root, fakeTmp, fakeHome } = createSandbox();
  mkdirSync(join(root, "dist"), { recursive: true });
  mkdirSync(join(root, "coverage"), { recursive: true });
  mkdirSync(join(root, "target", "debug"), { recursive: true });
  mkdirSync(join(root, "src"), { recursive: true });
  writeFileSync(join(root, "dist", "a.js"), "123456");
  writeFileSync(join(root, "coverage", "cov.json"), "1234");
  writeFileSync(join(root, "target", "debug", "app.bin"), "1234567890");
  writeFileSync(join(root, "src", "App.tsx"), "export default function App() {}");

  const midWork = cleanStorage({ root, tmpDir: fakeTmp, homeDir: fakeHome, keepTarget: true });
  assert.equal(midWork.removedCount, 2);
  assert.equal(midWork.freedBytes, 10);
  assert.ok(!existsSync(join(root, "dist")));
  assert.ok(!existsSync(join(root, "coverage")));
  assert.ok(existsSync(join(root, "target", "debug", "app.bin")));
  assert.ok(existsSync(join(root, "src", "App.tsx")));

  const finalClean = cleanStorage({ root, tmpDir: fakeTmp, homeDir: fakeHome });
  assert.equal(finalClean.removedCount, 1);
  assert.equal(finalClean.freedBytes, 10);
  assert.ok(!existsSync(join(root, "target")));
  assert.ok(existsSync(join(root, "src", "App.tsx")));
});

test("cleans stale prefixed temp directories and deep caches safely", () => {
  const { root, fakeTmp, fakeHome } = createSandbox();
  const staleTmp = join(fakeTmp, "pdfsak-old-run");
  const freshTmp = join(fakeTmp, "omnioffice-active-run");
  const unrelatedTmp = join(fakeTmp, "other-app-file.txt");
  mkdirSync(staleTmp, { recursive: true });
  mkdirSync(freshTmp, { recursive: true });
  writeFileSync(join(staleTmp, "doc.pdf"), "old");
  writeFileSync(join(freshTmp, "doc.pdf"), "new");
  writeFileSync(unrelatedTmp, "keep");

  const oldDate = new Date(Date.now() - 15 * 60 * 1000);
  utimesSync(staleTmp, oldDate, oldDate);

  mkdirSync(join(fakeHome, ".npm", "_cacache"), { recursive: true });
  writeFileSync(join(fakeHome, ".npm", "_cacache", "blob"), "12345");

  const result = cleanStorage({ root, tmpDir: fakeTmp, homeDir: fakeHome, deep: true });
  assert.ok(!existsSync(staleTmp));
  assert.ok(existsSync(freshTmp));
  assert.ok(existsSync(unrelatedTmp));
  assert.ok(!existsSync(join(fakeHome, ".npm", "_cacache")));
  assert.equal(result.freedBytes, 8);
});
