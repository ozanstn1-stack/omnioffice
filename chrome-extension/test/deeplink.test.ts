// Unit tests for the context-menu deep link handling: the fragment written by
// background.js must only ever produce an http(s) document, and the derived
// file name must not contain path separators or control characters.

import { test } from "node:test";
import assert from "node:assert/strict";

import { fileNameForLink, parseRemoteLink } from "../src/lib/remotelink";

test("remote link fragments are parsed strictly", () => {
  const link = parseRemoteLink(`#url=${encodeURIComponent("https://example.com/docs/report.pdf")}`);
  assert.deepEqual(link, { url: "https://example.com/docs/report.pdf", host: "example.com" });

  const withPort = parseRemoteLink(`#url=${encodeURIComponent("http://localhost:8080/report.pdf")}`);
  assert.equal(withPort?.host, "localhost:8080");

  assert.equal(parseRemoteLink("#url=javascript:alert(1)"), null);
  assert.equal(parseRemoteLink("#url=data:text/html,<script>"), null);
  assert.equal(parseRemoteLink("#url=file:///etc/passwd"), null);
  assert.equal(parseRemoteLink("#url=vbscript:msgbox(1)"), null);
  assert.equal(parseRemoteLink("#other=1"), null);
  assert.equal(parseRemoteLink("#url=not a url"), null);
  assert.equal(parseRemoteLink(""), null);
});

test("remote link file names are sanitized", () => {
  assert.equal(fileNameForLink("https://example.com/a/report.pdf"), "report.pdf");
  assert.equal(fileNameForLink("https://example.com/a/report"), "report.pdf");
  assert.equal(fileNameForLink("https://example.com/"), "remote-document.pdf");
  assert.equal(fileNameForLink("https://example.com/a%20b.pdf"), "a b.pdf");
  // Percent-encoded traversal and Windows-reserved characters never survive.
  assert.equal(fileNameForLink("https://example.com/dir/..%2F..%2Fsecret.pdf"), ".._.._secret.pdf");
  assert.equal(fileNameForLink("https://example.com/C%3A%5Cbad%3F.pdf"), "C__bad_.pdf");
});
