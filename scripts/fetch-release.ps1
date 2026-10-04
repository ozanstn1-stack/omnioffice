# Downloads the release assets of a version from GitHub into a discoverable
# folder next to the project (default: <projects>\OmniOffice-<v>).
#
# Use this when the artifacts were produced by CI (tag push) and this machine
# only needs to receive them; `release-local.ps1` does the same copy step after
# a local build.
#
# Usage:
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts/fetch-release.ps1
#   powershell ... -File scripts/fetch-release.ps1 -Version 3.3.1
#   powershell ... -File scripts/fetch-release.ps1 -Version 3.3.1 -Dest D:\releases
param(
    [string]$Version,
    [string]$Dest
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot

if (-not $Version) {
    $Version = (Get-Content (Join-Path $root 'src-tauri\tauri.conf.json') -Raw | ConvertFrom-Json).version
}
$tag = "v$Version"

if (-not $Dest) {
    $projects = Split-Path -Parent $root
    $Dest = Join-Path $projects "OmniOffice-$Version"
}
New-Item -ItemType Directory -Force -Path $Dest | Out-Null

Write-Host "==> Downloading release $tag into $Dest"
& gh release download $tag --dir $Dest --clobber
if ($LASTEXITCODE -ne 0) {
    throw "gh release download failed for $tag (is the release published?)"
}

# Verify every file the release's checksum manifests cover.
$verified = 0
foreach ($manifest in @('SHA256SUMS.txt', 'SHA256SUMS-android.txt', 'SHA256SUMS-windows.txt')) {
    $manifestPath = Join-Path $Dest $manifest
    if (-not (Test-Path $manifestPath)) { continue }
    foreach ($line in Get-Content $manifestPath) {
        if ($line -match '^([0-9a-f]{64})\s+(.+)$') {
            $expected = $Matches[1]
            $name = $Matches[2]
            $file = Join-Path $Dest $name
            if (-not (Test-Path $file)) { continue }
            $actual = (Get-FileHash -Algorithm SHA256 $file).Hash.ToLower()
            if ($actual -ne $expected) {
                throw "Checksum mismatch for $name"
            }
            $verified += 1
        }
    }
}

Get-ChildItem $Dest | Select-Object Name, @{ n = 'MB'; e = { [math]::Round($_.Length / 1MB, 1) } } | Format-Table -AutoSize
Write-Host "Release assets for $tag are in $Dest ($verified checksums verified)."
