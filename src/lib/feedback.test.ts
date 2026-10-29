import { describe, expect, it } from "vitest";

import { ISSUE_NEW_URL, buildIssueUrl, draftIssue, issueBody, issueTitle } from "./feedback";

describe("issueTitle", () => {
  it("appends the version", () => {
    expect(issueTitle("Problem: ", "6.5.1")).toBe("Problem: (v6.5.1)");
  });

  it("falls back to Problem for an empty title", () => {
    expect(issueTitle("   ", "6.5.1")).toBe("Problem (v6.5.1)");
  });

  it("leaves the version out when it is unknown", () => {
    expect(issueTitle("Problem", "")).toBe("Problem");
  });
});

describe("issueBody", () => {
  it("resolves the version placeholder", () => {
    expect(issueBody("Version: {version}\n", "6.5.1")).toBe("Version: 6.5.1\n");
  });

  it("shows ? for an unknown version", () => {
    expect(issueBody("Version: {version}", "")).toBe("Version: ?");
  });
});

describe("buildIssueUrl", () => {
  it("prefills the title, body and label", () => {
    const url = buildIssueUrl(draftIssue("Version: {version}", "Problem: ", "6.5.1"));
    expect(url.startsWith(`${ISSUE_NEW_URL}?`)).toBe(true);
    const params = new URL(url).searchParams;
    expect(params.get("title")).toBe("Problem: (v6.5.1)");
    expect(params.get("body")).toBe("Version: 6.5.1");
    expect(params.get("labels")).toBe("feedback");
  });

  it("survives newlines and Turkish characters in the body", () => {
    const body = "## Ne oldu\n\nİmza doğrulama ekranı çöküyor\n";
    const url = buildIssueUrl({ title: "Sorun: (v6.5.1)", body });
    const params = new URL(url).searchParams;
    expect(params.get("body")).toBe(body);
    expect(url).not.toMatch(/\s/);
  });
});
