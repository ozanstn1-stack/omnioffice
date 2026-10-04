$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$releaseDir = Join-Path $root 'release-artifacts'
$outRoot = Join-Path $root 'src-tauri\gen\android\app\build\outputs\apk'
$version = (Get-Content (Join-Path $root 'src-tauri\tauri.conf.json') -Raw | ConvertFrom-Json).version

$flavorByAbi = @{
    'arm64-v8a'   = 'arm64'
    'armeabi-v7a' = 'arm'
}

New-Item -ItemType Directory -Force -Path $releaseDir | Out-Null

$copied = @()
foreach ($abi in @('arm64-v8a', 'armeabi-v7a')) {
    $apk = Join-Path $outRoot "$($flavorByAbi[$abi])\release\app-$($flavorByAbi[$abi])-release.apk"
    if (-not (Test-Path $apk)) { throw "missing $apk - run npm run android:build:all first" }
    $target = Join-Path $releaseDir "OmniOffice-Android-$version-$abi.apk"
    Copy-Item $apk $target -Force
    $copied += $target
    Write-Host "$abi -> $target ($([math]::Round((Get-Item $target).Length / 1MB, 1)) MB)"
}

# Only the current version's Android files: the sums file has to describe what
# is being published, not every APK ever built.
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
foreach ($file in $copied) {
    $hash = Get-Sha256 $file
    $lines += "$hash  $(Split-Path -Leaf $file)"
}
$sums = Join-Path $releaseDir 'SHA256SUMS-android.txt'
Set-Content $sums -Value ($lines -join "`n")
Write-Host "--- $sums"
Get-Content $sums
