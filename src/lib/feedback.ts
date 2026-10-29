/**
 * The GitHub issue a "report a problem" run opens (C15).
 *
 * Nothing is uploaded by the app: the diagnostics report is copied to the
 * clipboard and the issue form is opened with a body template the user fills
 * in and pastes the report into. Kept free of React and Tauri so the URL
 * rules are unit-testable.
 */

/** Where the feedback issue is opened (the project's own tracker). */
export const ISSUE_NEW_URL = "https://github.com/ozanstn1-stack/omnioffice/issues/new";

/** Label the new issue gets; `feedback` is the one the maintainer watches. */
export const FEEDBACK_LABEL = "feedback";

export interface IssueDraft {
  title: string;
  body: string;
}

/** `${base} (v1.2.3)`, or just the base when the version is unknown. */
export function issueTitle(base: string, version: string): string {
  const cleaned = base.trim();
  const name = cleaned === "" ? "Problem" : cleaned;
  return version ? `${name} (v${version})` : name;
}

/** The body template with `{version}` resolved; unknown versions read as "?". */
export function issueBody(template: string, version: string): string {
  return template.replaceAll("{version}", version || "?");
}

/** The issue title and body for a version, ready to be put in the URL. */
export function draftIssue(template: string, baseTitle: string, version: string): IssueDraft {
  return { title: issueTitle(baseTitle, version), body: issueBody(template, version) };
}

/**
 * The `issues/new` URL with the title, body and label pre-filled.
 * `URLSearchParams` does the encoding, so newlines and Turkish characters in
 * the body cannot break the query and the allow-list sees no raw whitespace.
 */
export function buildIssueUrl(draft: IssueDraft): string {
  const params = new URLSearchParams({
    title: draft.title,
    body: draft.body,
    labels: FEEDBACK_LABEL,
  });
  return `${ISSUE_NEW_URL}?${params.toString()}`;
}
