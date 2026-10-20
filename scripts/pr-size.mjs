/**
 * Advisory PR size gate (v6.0.0 audit, B10).
 *
 * The audit asks for roughly 400 lines of non-generated code per PR. A hard
 * failure would block legitimate work (a split PR that moves a file, a data
 * table), so this measures the churn a reviewer actually reads - lockfiles,
 * generated code, archives, the i18n tables and the file-size baseline are
 * excluded - and flags the PR with a `size:large` label and one comment. The
 * comment is idempotent (`<!-- pr-size -->`) and is removed when the PR
 * shrinks below the limit.
 *
 * The job passes either way: the signal is the label/comment, not a red check
 * that would have to be bypassed on purpose.
 */
const token = process.env.GITHUB_TOKEN;
const [owner, repo] = (process.env.REPOSITORY ?? "").split("/");
const number = process.env.PR_NUMBER;
const LIMIT = 400;

const EXCLUDED = [
  /(^|\/)package-lock\.json$/,
  /(^|\/)Cargo\.lock$/,
  /^docs\/archive\//,
  /^src-tauri\/gen\//,
  /^src\/lib\/generated\//,
  /^scripts\/file-size-baseline\.json$/,
  /^src\/lib\/i18n\/(en|tr)\.json$/,
  /(^|\/)licenses\//,
];

async function api(path, init = {}) {
  const response = await fetch(`https://api.github.com${path}`, {
    ...init,
    headers: {
      authorization: `Bearer ${token}`,
      accept: "application/vnd.github+json",
      "content-type": "application/json",
      ...(init.headers ?? {}),
    },
  });
  if (!response.ok) throw new Error(`${response.status} ${init.method ?? "GET"} ${path}`);
  return response.status === 204 ? null : response.json();
}

async function files() {
  const all = [];
  for (let page = 1; ; page += 1) {
    const batch = await api(`/repos/${owner}/${repo}/pulls/${number}/files?per_page=100&page=${page}`);
    all.push(...batch);
    if (batch.length < 100) return all;
  }
}

async function markerComment() {
  const comments = await api(`/repos/${owner}/${repo}/issues/${number}/comments?per_page=100`);
  return comments.find((comment) => comment.body?.includes("<!-- pr-size -->"));
}

async function main() {
  const changed = await files();
  const counted = changed.filter((file) => !EXCLUDED.some((pattern) => pattern.test(file.filename)));
  const lines = counted.reduce((sum, file) => sum + file.additions + file.deletions, 0);
  const largest = counted
    .map((file) => ({ name: file.filename, lines: file.additions + file.deletions }))
    .sort((a, b) => b.lines - a.lines)
    .slice(0, 5);
  console.log(`pr-size: ${lines} non-generated lines across ${counted.length} files (limit ${LIMIT})`);

  const existing = await markerComment();
  if (lines <= LIMIT) {
    if (existing) await api(`/repos/${owner}/${repo}/issues/comments/${existing.id}`, { method: "DELETE" });
    await api(`/repos/${owner}/${repo}/issues/${number}/labels/size:large`, { method: "DELETE" }).catch(
      () => undefined,
    );
    return;
  }

  const body = [
    "<!-- pr-size -->",
    `**Size: ${lines} non-generated lines** (the review guideline is ~${LIMIT}).`,
    "",
    "Reviewers skim long diffs; if this cannot be split, say why in the PR description and keep the checklist honest. Largest files:",
    "",
    ...largest.map((file) => `- \`${file.name}\`: ${file.lines}`),
    "",
    "_Advisory only (v6.0.0 audit B10); lockfiles, generated code, archives, the i18n tables and the size baseline are excluded._",
  ].join("\n");
  if (existing) {
    await api(`/repos/${owner}/${repo}/issues/comments/${existing.id}`, {
      method: "PATCH",
      body: JSON.stringify({ body }),
    });
  } else {
    await api(`/repos/${owner}/${repo}/issues/${number}/comments`, { method: "POST", body: JSON.stringify({ body }) });
  }
  await api(`/repos/${owner}/${repo}/issues/${number}/labels`, {
    method: "POST",
    body: JSON.stringify({ labels: ["size:large"] }),
  });
}

main().catch((error) => {
  // An API hiccup must not turn into a red required check on the PR.
  console.warn(`pr-size: skipped (${error.message})`);
});
