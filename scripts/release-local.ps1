# One-command release flow: build -> package -> SBOM/checksums -> local install
# -> GitHub release.
#
# This is the repeatable "apply the update here too" path. It always applies the
# new build to this machine (per-user, no administrator needed) and, with
# -Publish, creates/updates the GitHub release for the current version.
#
# The finished artifacts are also copied to a discoverable folder next to the
# project (default: <projects>\OmniOffice-<version>) so the
# installer and APKs are easy to find without digging through release-artifacts.
#
# Usage:
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts/release-local.ps1
#   powershell ... -File scripts/release-local.ps1 -SkipBuild          # reuse existing binaries
#   powershell ... -File scripts/release-local.ps1 -SkipAndroid        # Windows only
#   powershell ... -File scripts/release-local.ps1 -Publish            # also publish to GitHub
#   powershell ... -File scripts/release-local.ps1 -ExportDir D:\releases
param(
    [switch]$SkipBuild,
    [switch]$SkipAndroid,
    [switch]$SkipInstall,
    [switch]$Publish,
    [switch]$Draft,
    [string]$ExportDir
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$releaseDir = Join-Path $root 'release-artifacts'
$version = (Get-Content (Join-Path $root 'src-tauri\tauri.conf.json') -Raw | ConvertFrom-Json).version
$tag = "v$version"

# Cargo is not always on PATH in a fresh shell.
$cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
if ((Test-Path $cargoBin) -and ($env:PATH -notlike "*$cargoBin*")) {
    $env:PATH = "$cargoBin;$env:PATH"
}

Push-Location $root
try {
    if (-not $SkipBuild) {
        Write-Host "==> Windows release build"
        & npm run app:build
        if ($LASTEXITCODE -ne 0) { throw "tauri build failed ($LASTEXITCODE)" }
        Write-Host "==> Windows package"
        & npm run package
        if ($LASTEXITCODE -ne 0) { throw "package failed ($LASTEXITCODE)" }
    }

    if (-not $SkipAndroid) {
        Write-Host "==> Android build (arm64-v8a + armeabi-v7a, APK + AAB)"
        & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'build-android.ps1') -Abi arm64-v8a,armeabi-v7a -Bundle
        if ($LASTEXITCODE -ne 0) { throw "android build failed ($LASTEXITCODE)" }
    }

    Write-Host "==> Release metadata"
    & npm run build:info
    if ($LASTEXITCODE -ne 0) { throw "build-info failed ($LASTEXITCODE)" }
    & node (Join-Path $PSScriptRoot 'sbom-rust.mjs')
    if ($LASTEXITCODE -ne 0) { throw "rust sbom failed ($LASTEXITCODE)" }
    & npm sbom --sbom-format cyclonedx | Set-Content (Join-Path $releaseDir 'sbom-npm.cyclonedx.json') -Encoding UTF8

    Write-Host "==> Checksums"
    $assets = @(
        "OmniOffice-Setup-$version.exe",
        "OmniOffice_${version}_x64-setup.exe",
        "OmniOffice-Portable-$version.zip",
        "OmniOffice-Android-$version-arm64-v8a.apk",
        "OmniOffice-Android-$version-armeabi-v7a.apk",
        "OmniOffice-Android-$version-arm64-v8a.aab",
        "OmniOffice-Android-$version-armeabi-v7a.aab",
        'sbom-rust.cyclonedx.json',
        'sbom-npm.cyclonedx.json',
        'build-info.json'
    )
    # The Chrome extension has its own version line; include the newest package
    # whenever one has been built so the release carries it too.
    $extension = Get-ChildItem $releaseDir -Filter '*Chrome-Extension-*.zip' -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1
    if ($extension) { $assets += $extension.Name }
    $lines = @()
    foreach ($name in $assets) {
        $path = Join-Path $releaseDir $name
        if (-not (Test-Path $path)) { Write-Host "    (missing, skipped: $name)"; continue }
        $hash = (Get-FileHash -Algorithm SHA256 $path).Hash.ToLower()
        $lines += "$hash  $name"
    }
    Set-Content (Join-Path $releaseDir 'SHA256SUMS.txt') -Value ($lines -join "`n") -Encoding ASCII

    # Copy the finished artifacts somewhere obvious. `release-artifacts` is
    # inside the repo and gets mixed with older versions; this folder is named
    # after the release so the installer/APKs are easy to find.
    $export = if ($ExportDir) { $ExportDir } else { Join-Path (Split-Path -Parent $root) "OmniOffice-$version" }
    Write-Host "==> Exporting artifacts to $export"
    New-Item -ItemType Directory -Force -Path $export | Out-Null
    Get-ChildItem $releaseDir -File | Where-Object {
        $_.Name -like "*$version*" -or $_.Name -like 'sbom-*' -or $_.Name -like 'SHA256SUMS*' -or $_.Name -eq 'build-info.json' -or $_.Name -like '*Chrome-Extension-*'
    } | ForEach-Object {
        Copy-Item $_.FullName (Join-Path $export $_.Name) -Force
    }
    Get-ChildItem $export | Select-Object Name, @{ n = 'MB'; e = { [math]::Round($_.Length / 1MB, 1) } } | Format-Table -AutoSize

    if (-not $SkipInstall) {
        Write-Host "==> Applying to this machine (per-user)"
        & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'install-local.ps1') -Version $version
        if ($LASTEXITCODE -ne 0) { throw "local install failed ($LASTEXITCODE)" }
    }

    if ($Publish) {
        Write-Host "==> GitHub release $tag"
        # `gh release view` writes "release not found" to stderr when the
        # release does not exist yet; with $ErrorActionPreference = 'Stop' that
        # native stderr terminates the script before the release is created.
        # Relax the preference for the probe and restore it afterwards.
        $previousPreference = $ErrorActionPreference
        $ErrorActionPreference = 'Continue'
        try {
            $null = & gh release view $tag 2>&1
            $releaseExists = ($LASTEXITCODE -eq 0)
            if (-not $releaseExists) {
                $null = & git rev-parse -q --verify "refs/tags/$tag" 2>&1
                if ($LASTEXITCODE -ne 0) {
                    $null = & git tag $tag 2>&1
                }
                & git push origin $tag
                if ($LASTEXITCODE -ne 0) { throw "git push origin $tag failed ($LASTEXITCODE)" }
                $draftFlag = if ($Draft) { '--draft' } else { '--latest' }
                & gh release create $tag --title "OmniOffice $tag" --generate-notes $draftFlag
                if ($LASTEXITCODE -ne 0) { throw "gh release create failed ($LASTEXITCODE)" }
            }
            Get-ChildItem $releaseDir -File | Where-Object {
                $_.Name -like "*$version*" -or $_.Name -like 'sbom-*' -or $_.Name -like 'SHA256SUMS*' -or $_.Name -eq 'build-info.json' -or $_.Name -like '*Chrome-Extension-*'
            } | ForEach-Object {
                & gh release upload $tag $_.FullName --clobber
                if ($LASTEXITCODE -ne 0) { throw "gh release upload failed for $($_.Name)" }
            }
            & gh release view $tag --json url --jq .url
        } finally {
            $ErrorActionPreference = $previousPreference
        }
    }

    Write-Host "Done. Version $version."
} finally {
    Pop-Location
}
