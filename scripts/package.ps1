# Packages the release artifacts:
#   release-artifacts/OmniOffice-Setup-<version>.exe     (NSIS installer)
#   release-artifacts/OmniOffice-Portable-<version>.zip  (portable build)
#   release-artifacts/SHA256SUMS.txt
#
# Run `npm run tauri build` first (or pass -Build to do it here).
param(
    [switch]$Build
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$version = (Get-Content (Join-Path $root 'src-tauri\tauri.conf.json') -Raw | ConvertFrom-Json).version
$releaseDir = Join-Path $root 'release-artifacts'
$targetRelease = Join-Path $root 'target\release'

if ($Build) {
    Push-Location $root
    try {
        & npm run tauri build
        if ($LASTEXITCODE -ne 0) { throw "tauri build failed with code $LASTEXITCODE" }
    } finally {
        Pop-Location
    }
}

New-Item -ItemType Directory -Force -Path $releaseDir | Out-Null

# ---------------------------------------------------------------- installer
# Match the installer for *this* version explicitly: old bundle outputs stay
# in target/release/bundle/nsis after a version bump, and picking "the first
# file" would silently ship the previous release.
$nsisDir = Join-Path $targetRelease 'bundle\nsis'
$nsis = Get-ChildItem $nsisDir -Filter "*-$version-*-setup.exe" |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1
if (-not $nsis) {
    $nsis = Get-ChildItem $nsisDir -Filter '*-setup.exe' |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1
}
if (-not $nsis) { throw 'NSIS installer not found. Run npm run tauri build first.' }
if ($nsis.Name -notlike "*$version*") {
    throw "Installer $($nsis.Name) does not match version $version. Re-run npm run tauri build."
}
$installerName = "OmniOffice-Setup-$version.exe"
$installerPath = Join-Path $releaseDir $installerName
Copy-Item $nsis.FullName $installerPath -Force
Write-Host "installer: $installerPath ($([math]::Round((Get-Item $installerPath).Length / 1MB, 1)) MB from $($nsis.Name))"

# The V3.1 release contract names the Windows installer
# `OmniOffice_<version>_x64-setup.exe` (Tauri's own bundle naming).
# Ship the identical file under that name as well so release consumers can use
# either; both are listed in SHA256SUMS.txt.
$contractInstallerName = "OmniOffice_${version}_x64-setup.exe"
$contractInstallerPath = Join-Path $releaseDir $contractInstallerName
Copy-Item $installerPath $contractInstallerPath -Force
Write-Host "installer (release name): $contractInstallerPath"

# ---------------------------------------------------------------- portable
$portableStage = Join-Path $env:TEMP "omnioffice-portable-$version"
Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $portableStage
New-Item -ItemType Directory -Force -Path $portableStage | Out-Null
$exeName = 'OmniOffice.exe'
if (-not (Test-Path (Join-Path $targetRelease $exeName))) {
    $exeName = 'pdf-swiss-army-knife.exe'
}
Copy-Item (Join-Path $targetRelease $exeName) (Join-Path $portableStage 'OmniOffice.exe') -Force
Copy-Item (Join-Path $targetRelease 'resources') (Join-Path $portableStage 'resources') -Recurse -Force
Copy-Item (Join-Path $root 'README.md') $portableStage -Force
Copy-Item (Join-Path $root 'LICENSE') $portableStage -Force
$portableZip = Join-Path $releaseDir "OmniOffice-Portable-$version.zip"
Remove-Item $portableZip -ErrorAction SilentlyContinue
Compress-Archive -Path (Join-Path $portableStage '*') -DestinationPath $portableZip -CompressionLevel Optimal
Remove-Item -Recurse -Force $portableStage
Write-Host "portable:  $portableZip ($([math]::Round((Get-Item $portableZip).Length / 1MB, 1)) MB)"

# ---------------------------------------------------------------- checksums
# SHA256 through .NET instead of Get-FileHash: when npm runs this script from a
# pwsh parent, Windows PowerShell 5.1 can inherit a PSModulePath that does not
# contain its own utility modules, and Get-FileHash then "does not exist".
function Get-Sha256([string]$Path) {
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        $stream = [System.IO.File]::OpenRead($Path)
        try {
            $bytes = $sha.ComputeHash($stream)
        } finally {
            $stream.Dispose()
        }
    } finally {
        $sha.Dispose()
    }
    return ([System.BitConverter]::ToString($bytes) -replace '-', '').ToLowerInvariant()
}

$lines = @()
foreach ($file in @($installerPath, $contractInstallerPath, $portableZip)) {
    $hash = Get-Sha256 $file
    $lines += "$hash  $(Split-Path -Leaf $file)"
}
Set-Content (Join-Path $releaseDir 'SHA256SUMS.txt') -Value ($lines -join "`n")
Write-Host "checksums: $(Join-Path $releaseDir 'SHA256SUMS.txt')"
Get-ChildItem $releaseDir | Select-Object Name, @{ n = 'MB'; e = { [math]::Round($_.Length / 1MB, 1) } }
