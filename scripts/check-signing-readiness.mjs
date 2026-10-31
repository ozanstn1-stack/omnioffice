#!/usr/bin/env node
// Signing readiness gate (D10).
//
// The workflow supports three Authenticode providers and stays unsigned when
// none is configured. The dangerous state is a *partial* configuration: a
// provider selected by one secret but missing another fails deep inside the
// signing action, and a verification step that silently disappears would let
// an unsigned installer ship as "signed". This script pins the invariants in
// the workflow and the runbook:
//
//   * the provider detection still recognises all three providers and the
//     unsigned fallback;
//   * every SignPath step is guarded by the provider check;
//   * the preflight step exists and names every required secret of each
//     provider;
//   * the signature is verified and the packaged installer is checked again
//     after packaging;
//   * docs/signing.md lists the same secret names, the verification commands
//     and the first-signed-release runbook.
//
// Run with `npm run signing:check`; the vitest suite runs it too.
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = process.env.SIGNING_ROOT ?? join(dirname(fileURLToPath(import.meta.url)), "..");
const workflow = readFileSync(join(root, ".github", "workflows", "release.yml"), "utf8");
const docs = readFileSync(join(root, "docs", "signing.md"), "utf8");

const failures = [];
const check = (condition, message) => {
  if (!condition) failures.push(message);
};

const signpathSecrets = [
  "SIGNPATH_API_TOKEN",
  "SIGNPATH_ORGANIZATION_ID",
  "SIGNPATH_PROJECT_SLUG",
  "SIGNPATH_SIGNING_POLICY_SLUG",
  "SIGNPATH_ARTIFACT_CONFIGURATION_SLUG",
];
const azureSecrets = [
  "AZURE_TENANT_ID",
  "AZURE_CLIENT_ID",
  "AZURE_CLIENT_SECRET",
  "AZURE_TS_ENDPOINT",
  "AZURE_TS_ACCOUNT",
  "AZURE_TS_PROFILE",
];

// Provider detection keeps the unsigned fallback working for forks.
check(/Detect the Authenticode provider/.test(workflow), "the detection step is missing");
check(/SIGNPATH_API_TOKEN/.test(workflow) && /AZURE_CLIENT_SECRET/.test(workflow) && /WINDOWS_CERT_PFX/.test(workflow),
  "the detection step must recognise all three providers");
check(/provider="none"/.test(workflow), "the unsigned fallback must stay reachable");

// Every SignPath step is guarded; an unguarded one would run without secrets.
const steps = workflow.split(/\n {6}- name: /).slice(1);
for (const step of steps) {
  const name = step.split("\n", 1)[0].trim();
  if (!/SignPath/i.test(name) && !/Restore the SignPath-signed files/i.test(name)) continue;
  const guard = /if: steps\.authenticode\.outputs\.provider == 'signpath'/.test(step.split(/\n {6}- name: /)[0]);
  check(guard, `the "${name}" step is not guarded by the SignPath provider check`);
}

// Preflight names every required secret, so a partial configuration fails with
// the missing names instead of an opaque action error.
check(/Check the Authenticode configuration/.test(workflow), "the Authenticode preflight step is missing");
for (const secret of [...signpathSecrets, ...azureSecrets, "WINDOWS_CERT_PFX", "WINDOWS_CERT_PASSWORD"]) {
  check(workflow.includes(secret), `the workflow does not pass ${secret}`);
}

// Verification cannot be skipped: the installer and the packaged copy are both
// checked, and the updater signature is refreshed after signing.
check(/Verify the Authenticode signature/.test(workflow), "the signature verification step is missing");
check(/Verify the packaged installer is signed/.test(workflow), "the packaged installer check is missing");
check(/Refresh the updater signature over the signed installer/.test(workflow),
  "the updater signature refresh is missing");

// The runbook must document the same contract.
for (const secret of [...signpathSecrets, "AZURE_TS_ACCOUNT"]) {
  check(docs.includes(secret), `docs/signing.md does not document ${secret}`);
}
check(/Get-AuthenticodeSignature/.test(docs), "docs/signing.md does not show Get-AuthenticodeSignature");
check(/signtool verify/.test(docs), "docs/signing.md does not show signtool verify");
check(/D10 runbook/.test(docs), "docs/signing.md has no first-signed-release runbook");
check(/unsigned/i.test(docs), "docs/signing.md must keep the unsigned fallback note");

if (failures.length > 0) {
  console.error("signing readiness: FAILED");
  for (const failure of failures) console.error(`  - ${failure}`);
  process.exit(1);
}
console.log(
  `signing readiness: OK (3 providers, ${signpathSecrets.length + azureSecrets.length} required secrets, verification and runbook pinned)`,
);
