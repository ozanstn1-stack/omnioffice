# Windows Authenticode signing — decision and preparation (#501)

The Windows installer is Authenticode-signed only when a signing provider is
configured. With no provider secrets the release workflow behaves exactly as
before: the exe is unsigned, SmartScreen shows "unknown publisher" on first
download, and nothing else changes. This file is the decision guide: what each
provider costs, what identity validation it needs, how it is wired into
`.github/workflows/release.yml`, what to verify afterwards, and why the updater
signature has to be refreshed after signing.

The current state is expressed in `docs/distribution.md` and the README: they
say "unsigned" until the first release that actually carries a signature.

## What gets signed, and what does not

| Artifact | Signed by Authenticode? | Notes |
|---|---|---|
| `OmniOffice-Setup-X.Y.Z.exe` (NSIS installer) | yes, when a provider is set | the file users download and run |
| `OmniOffice_<version>_x64-setup.exe` | same bytes as the installer | package.ps1 copies it after signing |
| `OmniOffice.exe` inside `OmniOffice-Portable-X.Y.Z.zip` | yes, when a provider is set | signed before the ZIP is built, so the ZIP contains the signed exe |
| `latest.json` | no — Tauri's updater uses a separate minisign key | the Authenticode signature is refreshed into `.exe.sig`, see below |
| `SHA256SUMS*.txt`, `build-info.json`, SBOMs | no | covered by the build-provenance attestation instead |
| APK/AAB, `.deb`, `.AppImage` | different mechanisms | Android: release keystore + signature-continuity check in `android.yml`; Linux: no Authenticode equivalent |

The updater manifest (`latest.json`) remains valid: Tauri signs the updater
artifact during the build, and Authenticode changes the installer bytes
afterwards, so the workflow re-runs `tauri signer sign` over the signed
installer before `make-latest-json.mjs` looks for the `.sig`. Skipping that step
would ship an update whose signature never verifies; the workflow does it only
when Authenticode actually ran.

## Provider comparison

Prices are indicative (mid-2026, USD, before tax) and change; confirm on the
provider's page before deciding.

| | Azure Trusted Signing | SignPath.io | Own OV/EV certificate |
|---|---|---|---|
| Cost | ~$9.99/month (Basic) + Azure account | free for open-source projects (application is reviewed) | OV ~$200–400/year; EV ~$300–700/year |
| Identity validation | Microsoft identity validation, days to weeks, individual allowed only in supported countries | project/organisation review by SignPath, OSS projects usually approved quickly | CA identity validation (legal entity or individual), days to weeks |
| Key custody | Microsoft-managed short-lived certificates; private key never leaves the service | SignPath-held key / signing service | CA/B Forum requires the key in hardware (HSM or token) since 2023 |
| CI integration | `signtool` + Trusted Signing dlib (this repo) or `azure/trusted-signing-action` | `SignPath/github-action-submit-signing-request` (this repo, pinned) | `signtool` with a PFX secret (legacy/test only, see below) |
| Artifacts leave GitHub? | only the file to sign goes to Azure | the whole artifact is uploaded to SignPath and returned signed | no |
| SmartScreen / reputation | signature trusted immediately; reputation builds per certificate | same | EV gives near-immediate SmartScreen reputation; OV builds reputation |
| Effort in this repo | secrets + Azure app registration + identity validation | secrets + SignPath project/policy/artifact configuration | secrets + hardware-token strategy for renewals |
| Fits a solo OSS project? | yes, if the identity validation is acceptable | best fit (free, OSS-oriented) | only when instant reputation or full key control is required |

Who signs what in the workflow: the detection step picks **SignPath** when
`SIGNPATH_API_TOKEN` exists, otherwise **Azure** when `AZURE_CLIENT_SECRET`
exists, otherwise **PFX** when `WINDOWS_CERT_PFX` exists. One provider at a
time; remove the other secrets to switch.

### Recommendation

1. **SignPath.io** — free for OSS, purpose-built for GitHub release pipelines,
   and the request/approval flow doubles as a small code review. Start here.
2. **Azure Trusted Signing** — the fallback when the SignPath application is
   declined or the project prefers Microsoft's identity validation. The
   workflow already supports it through the Trusted Signing dlib.
3. **Own EV certificate** — only if instant SmartScreen reputation is a hard
   requirement (for example a paid product page) or the key must stay in-house.
   The CA/B hardware-key rule makes the CI setup harder and the renewal a
   manual job; keep it as the long-term option, not the first step.
4. **PFX-in-a-secret** — supported by the script for completeness and tests.
   It does not satisfy the CA/B hardware requirement for a real code-signing
   key; do not use it for the public release.

## Required secrets

Add them under Settings → Secrets and variables → Actions. The detection step
only looks at one of these per provider:

| Provider | Secrets |
|---|---|
| SignPath | `SIGNPATH_API_TOKEN`, `SIGNPATH_ORGANIZATION_ID`, `SIGNPATH_PROJECT_SLUG`, `SIGNPATH_SIGNING_POLICY_SLUG`, `SIGNPATH_ARTIFACT_CONFIGURATION_SLUG` |
| Azure Trusted Signing | `AZURE_TENANT_ID`, `AZURE_CLIENT_ID`, `AZURE_CLIENT_SECRET`, `AZURE_TS_ENDPOINT` (e.g. `https://eus.codesigning.azure.net`), `AZURE_TS_ACCOUNT`, `AZURE_TS_PROFILE` |
| PFX (legacy/test) | `WINDOWS_CERT_PFX` (base64 of the .pfx), `WINDOWS_CERT_PASSWORD` |

Existing and unrelated: `TAURI_SIGNING_PRIVATE_KEY` /
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD` sign the updater manifest, not the exe.
Losing them is recoverable (re-publish an updater key); losing the Authenticode
key is not (installed apps cannot be updated by a new signer until it is
re-signable), which is why the provider owns the key.

## What the workflow does

In `.github/workflows/release.yml`, Windows job, after `tauri build` and before
`npm run package`:

1. **Detect the provider** from the secrets; with none it annotates the run
   "installer stays unsigned" and every signing step is skipped.
2. **Sign** the app exe and the NSIS installer:
   - PFX / Azure: `.github/scripts/windows-signing.ps1 -Action sign -Provider …`.
   - SignPath: stage the two files, upload them as a workflow artifact,
     `SignPath/github-action-submit-signing-request` signs them, and the
     restore step copies the signed files back over the originals.
3. **Verify** with `signtool verify /pa /v` and `Get-AuthenticodeSignature`
   (status must be `Valid`).
4. **Refresh the updater signature** over the signed installer
   (`.github/scripts/refresh-updater-signature.ps1`).
5. `npm run package` builds the portable ZIP (with the signed exe) and copies
   the installer under both release names.
6. The packaged `OmniOffice-Setup-X.Y.Z.exe` is verified once more, then
   `latest.json`, the checksums, the attestation and the upload run on the
   final bytes.

The workflow supports `workflow_dispatch`: dispatch it on a branch to dry-run
the whole job (release upload steps are tag-gated, so nothing is published)
before relying on it for a real tag.

## Verifying a release by hand

```powershell
# Signature and timestamp of the installer
Get-AuthenticodeSignature .\OmniOffice-Setup-6.12.0.exe | Format-List Status, SignerCertificate, TimeStamperCertificate

# Full chain check with the Windows SDK
signtool verify /pa /v .\OmniOffice-Setup-6.12.0.exe

# The portable exe inside the ZIP must be signed too
Expand-Archive .\OmniOffice-Portable-6.12.0.zip -DestinationPath .\portable
Get-AuthenticodeSignature .\portable\OmniOffice.exe | Format-List Status, SignerCertificate
```

- `Status` must be `Valid` and a timestamp authority must be present (the
  signature stays valid after the certificate expires only with a timestamp).
- The file hashes in `SHA256SUMS.txt` are over the **signed** bytes; if a
  checksum fails, something modified the artifact after signing.
- SmartScreen reputation is not instant for a new OV certificate; the first
  downloads may still show a prompt, which improves as the certificate is used.

## Rollout checklist

1. Choose the provider (SignPath recommended) and create the account/project.
2. Complete identity validation (Microsoft) or the OSS project review
   (SignPath).
3. Add the secrets above; keep the provider's signing key under the provider's
   custody, not in the repository.
4. Dry-run: `gh workflow run release.yml --ref <branch>` and check the signing
   and verification steps in the log; the job must still pass with the
   secrets removed (it then skips signing).
5. Cut the next release; confirm `Get-AuthenticodeSignature` is `Valid` for the
   installer and the portable exe, and that `latest.json` still verifies in the
   app.
6. Update `docs/distribution.md`, the README limitation note and
   `RELEASE_READINESS.md`: remove "unsigned" only after a signed release
   exists.

## Troubleshooting

- **`signtool` not found** — the Windows SDK is missing; the release runner has
  it, a local machine needs "Windows SDK Signing Tools".
- **Azure: `AADSTS…`/credential error** — the app registration needs the
  "Trusted Signing Certificate Profile Signer" role on the account, and the
  endpoint/account/profile names must match the Azure resource exactly.
- **SignPath: artifact not found** — the upload step must run in the same job
  and its `artifact-id` output is what the action consumes; the SignPath
  artifact configuration has to list both file names.
- **Update check fails after signing** — the updater signature was not
  refreshed; `latest.json` points at an old `.sig`. Re-run the release job from
  step 4 or regenerate with `tauri signer sign`.
- **Checksums changed after signing** — expected: signing changes the bytes, so
  checksums are computed afterwards. Never compute them before signing.
