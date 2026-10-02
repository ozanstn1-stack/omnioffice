# One-command release flow: build -> package -> SBOM/checksums -> local install
# -> GitHub release.
#
# This is the repeatable "apply the update here too" path. It always applies the
# new build to this machine (per-user, no administrator needed) and, with
# -Publish, creates/updates the GitHub release for the current version.
#
# Usage:
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts/release-local.ps1
#   powershell ... -File scripts/release-local.ps1 -SkipBuild          # reuse existing binaries
#   powershell ... -File scripts/release-local.ps1 -SkipAndroid        # Windows only
#   powershell ... -File scripts/release-local.ps1 -Publish            # also publish to GitHub
param(
    [switch]$SkipBuild,
    [switch]$SkipAndroid,
    [switch]$SkipInstall,
    [switch]$Publish,
    [switch]$Draft
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
        "Office-Swiss-Army-Knife-Setup-$version.exe",
        "Office Swiss Army Knife_${version}_x64-setup.exe",
        "Office-Swiss-Army-Knife-Portable-$version.zip",
        "PDF-Swiss-Army-Knife-Android-$version-arm64-v8a.apk",
        "PDF-Swiss-Army-Knife-Android-$version-armeabi-v7a.apk",
        "PDF-Swiss-Army-Knife-Android-$version-arm64-v8a.aab",
        "PDF-Swiss-Army-Knife-Android-$version-armeabi-v7a.aab",
        'sbom-rust.cyclonedx.json',
        'sbom-npm.cyclonedx.json',
        'build-info.json'
    )
    $lines = @()
    foreach ($name in $assets) {
        $path = Join-Path $releaseDir $name
        if (-not (Test-Path $path)) { Write-Host "    (missing, skipped: $name)"; continue }
        $hash = (Get-FileHash -Algorithm SHA256 $path).Hash.ToLower()
        $lines += "$hash  $name"
    }
    Set-Content (Join-Path $releaseDir 'SHA256SUMS.txt') -Value ($lines -join "`n") -Encoding ASCII

    if (-not $SkipInstall) {
        Write-Host "==> Applying to this machine (per-user)"
        & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'install-local.ps1') -Version $version
        if ($LASTEXITCODE -ne 0) { throw "local install failed ($LASTEXITCODE)" }
    }

    if ($Publish) {
        Write-Host "==> GitHub release $tag"
        $existing = & gh release view $tag --json tagName 2>$null
        if (-not $existing) {
            & git tag $tag 2>$null
            & git push origin $tag
            $draftFlag = if ($Draft) { '--draft' } else { '--latest' }
            & gh release create $tag --title "Office Swiss Army Knife $tag" --generate-notes $draftFlag
            if ($LASTEXITCODE -ne 0) { throw "gh release create failed ($LASTEXITCODE)" }
        }
        Get-ChildItem $releaseDir -File | Where-Object {
            $_.Name -match "3\.3\.0|$version" -or $_.Name -like 'sbom-*' -or $_.Name -like 'SHA256SUMS*' -or $_.Name -eq 'build-info.json'
        } | ForEach-Object {
            & gh release upload $tag $_.FullName --clobber
            if ($LASTEXITCODE -ne 0) { throw "gh release upload failed for $($_.Name)" }
        }
        & gh release view $tag --json url --jq .url
    }

    Write-Host "Done. Version $version."
} finally {
    Pop-Location
}
