# Re-signs the NSIS installer for the Tauri updater after Authenticode signing
# changed its bytes (#501). Tauri signs the updater artifact during the build;
# an Authenticode signature added afterwards makes that minisign signature
# refer to stale bytes, and latest.json would point at a signature that never
# verifies - the update check would fail instead of installing.
#
# Runs only when Authenticode signing actually ran and the updater key exists.
# Without the key the build already skipped updater artifacts, so there is
# nothing to refresh.
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$version = (Get-Content (Join-Path $root 'src-tauri/tauri.conf.json') -Raw | ConvertFrom-Json).version
$nsisDir = Join-Path $root 'target/release/bundle/nsis'

if ([string]::IsNullOrEmpty($env:TAURI_SIGNING_PRIVATE_KEY)) {
    Write-Host '::warning::TAURI_SIGNING_PRIVATE_KEY is not set; there is no updater signature to refresh.'
    exit 0
}

$installer = Get-ChildItem $nsisDir -Filter "*-$version-*-setup.exe" -ErrorAction SilentlyContinue |
    Sort-Object LastWriteTime -Descending | Select-Object -First 1
if (-not $installer) { throw "the NSIS installer for $version was not found under $nsisDir" }
$signature = "$($installer.FullName).sig"
if (-not (Test-Path $signature)) {
    Write-Host '::warning::no updater signature next to the installer; the release will have no latest.json entry for it.'
    exit 0
}

Push-Location $root
try {
    & npx tauri signer sign $installer.FullName
    if ($LASTEXITCODE -ne 0) { throw "tauri signer sign failed with exit code $LASTEXITCODE" }
} finally {
    Pop-Location
}
if (-not (Test-Path $signature)) { throw "tauri signer sign did not write $signature" }
Write-Host "refreshed the updater signature: $signature ($((Get-Item $signature).Length) bytes)"
