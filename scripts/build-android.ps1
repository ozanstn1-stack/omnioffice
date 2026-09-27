# PDF Swiss Army Knife - Android builder
#
# Builds an APK with the same layout as `tauri android build`: the frontend is
# embedded in the Rust library (tauri/custom-protocol), the OCR engine and
# pdfium ship as native libraries, the language models as APK assets.
#
# The Tauri CLI creates symbolic links while preparing `jniLibs`, which needs
# Windows Developer Mode. This script compiles the Rust library with cargo and
# copies it instead, so plain Windows machines and CI build the same APK.
#
# Usage:
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-android.ps1
#   powershell ... -File scripts/build-android.ps1 -Abi arm64-v8a,armeabi-v7a
#   powershell ... -File scripts/build-android.ps1 -Debug -SkipFrontend

param(
    [ValidateSet('arm64-v8a', 'armeabi-v7a', 'x86_64', 'x86')]
    [string[]]$Abi = @('arm64-v8a'),
    [switch]$Debug,
    [switch]$SkipFrontend,
    [switch]$SkipEngines,
    [switch]$Bundle,
    [string]$AndroidHome,
    [string]$NdkHome
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

# `powershell -File script.ps1 -Abi a,b` passes "a,b" as a single string, so
# accept both forms.
$Abi = @($Abi | ForEach-Object { $_ -split ',' } | ForEach-Object { $_.Trim() } | Where-Object { $_ })

$root = Split-Path -Parent $PSScriptRoot
$tauriDir = Join-Path $root 'src-tauri'
$genDir = Join-Path $tauriDir 'gen\android'
$appDir = Join-Path $genDir 'app'

if (-not (Test-Path $genDir)) {
    throw "Android project not found at $genDir. Run 'npm run tauri android init' first."
}

function Resolve-Sdk {
    param([string]$Explicit)
    $candidates = @()
    if ($Explicit) { $candidates += $Explicit }
    if ($env:ANDROID_HOME) { $candidates += $env:ANDROID_HOME }
    if ($env:ANDROID_SDK_ROOT) { $candidates += $env:ANDROID_SDK_ROOT }
    $candidates += (Join-Path $env:LOCALAPPDATA 'Android\Sdk')
    foreach ($candidate in $candidates) {
        if ($candidate -and (Test-Path (Join-Path $candidate 'platform-tools'))) { return $candidate }
    }
    throw 'Android SDK not found. Set ANDROID_HOME or pass -AndroidHome.'
}

function Resolve-Ndk {
    param([string]$Explicit, [string]$Sdk)
    if ($Explicit -and (Test-Path $Explicit)) { return $Explicit }
    foreach ($key in @('ANDROID_NDK_HOME', 'NDK_HOME')) {
        $value = [Environment]::GetEnvironmentVariable($key)
        if ($value -and (Test-Path $value)) { return $value }
    }
    $ndkRoot = Join-Path $Sdk 'ndk'
    if (Test-Path $ndkRoot) {
        $latest = Get-ChildItem $ndkRoot -Directory | Sort-Object Name -Descending | Select-Object -First 1
        if ($latest) { return $latest.FullName }
    }
    throw 'Android NDK not found. Install one with `sdkmanager "ndk;27.3.13750724"` or pass -NdkHome.'
}

# PowerShell 5.1 turns anything a native tool writes to stderr into an error
# record, and with $ErrorActionPreference = 'Stop' that aborts the build before
# $LASTEXITCODE is ever consulted. Cargo, Gradle, esbuild and npm all write
# progress bars, ANSI colour codes and warnings to stderr on a *successful*
# run, so the script could never get past the frontend step. Run native tools
# with the preference relaxed and decide on the exit code, which is the only
# thing that actually says whether the build worked.
function Invoke-Native {
    param(
        [Parameter(Mandatory = $true)][string]$Command,
        [string[]]$Arguments = @(),
        [string]$What = 'command',
        [string]$WorkingDirectory
    )
    $previous = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    $global:LASTEXITCODE = 0
    try {
        Push-Location $(if ($WorkingDirectory) { $WorkingDirectory } else { $root })
        try {
            & $Command @Arguments
            $code = $LASTEXITCODE
        } finally {
            Pop-Location
        }
    } finally {
        $ErrorActionPreference = $previous
    }
    if ($code -ne 0) { throw "$What failed with exit code $code" }
}

$sdk = Resolve-Sdk -Explicit $AndroidHome
$ndk = Resolve-Ndk -Explicit $NdkHome -Sdk $sdk
$env:ANDROID_HOME = $sdk
$env:ANDROID_SDK_ROOT = $sdk
$env:ANDROID_NDK_HOME = $ndk
$env:NDK_HOME = $ndk
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"

Write-Host "==> Android SDK: $sdk"
Write-Host "==> Android NDK: $ndk"

# ---------------------------------------------------------------- engines
if (-not $SkipEngines) {
    $missing = $Abi | Where-Object {
        -not (Test-Path (Join-Path $tauriDir "resources\engines-android\$_\libpdfium.so"))
    }
    if ($missing) {
        Write-Host '==> Fetching Android engines'
        Invoke-Native -Command 'powershell' -Arguments @(
            '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File',
            (Join-Path $PSScriptRoot 'fetch-engines-android.ps1'), '-Abis', ($Abi -join ',')
        ) -What 'engine fetch'
    } else {
        Write-Host '==> Android engines present'
    }
}

# ---------------------------------------------------------------- frontend
if (-not $SkipFrontend) {
    Write-Host '==> Building frontend'
    Invoke-Native -Command 'npm' -Arguments @('run', 'build') -What 'frontend build'
}
if (-not (Test-Path (Join-Path $root 'dist\index.html'))) {
    throw 'dist/index.html is missing - run without -SkipFrontend first.'
}

# ------------------------------------------------------------------ assets
# The Tauri Android runtime reads `tauri.conf.json` from the APK assets and
# resolves bundled resources from the same place. Desktop engine binaries are
# not shipped in the APK, so the resource list is emptied for Android.
Write-Host '==> Injecting Android assets'
$assetsDir = Join-Path $appDir 'src\main\assets'
New-Item -ItemType Directory -Force -Path $assetsDir | Out-Null
$config = Get-Content (Join-Path $tauriDir 'tauri.conf.json') -Raw | ConvertFrom-Json
$platformConf = Join-Path $tauriDir 'tauri.android.conf.json'
if (Test-Path $platformConf) {
    $android = Get-Content $platformConf -Raw | ConvertFrom-Json
    foreach ($property in $android.PSObject.Properties) {
        $config | Add-Member -MemberType NoteProperty -Name $property.Name -Value $property.Value -Force
    }
}
$config.bundle.resources = @()
$config.bundle.targets = @()
if (-not $config.PSObject.Properties['plugins']) {
    $config | Add-Member -MemberType NoteProperty -Name plugins -Value ([PSCustomObject]@{}) -Force
}
$config | ConvertTo-Json -Depth 32 | Set-Content (Join-Path $assetsDir 'tauri.conf.json') -Encoding UTF8

# Gradle reads the app version from `tauri.properties` (normally written by the
# Tauri CLI, which this script deliberately does not call).
$versionName = $config.version
$versionParts = @($versionName -split '[.\-+]')
$major = [int]($versionParts[0]); $minor = if ($versionParts.Count -gt 1) { [int]$versionParts[1] } else { 0 }
$patch = if ($versionParts.Count -gt 2) { [int]$versionParts[2] } else { 0 }
$versionCode = $major * 1000000 + $minor * 1000 + $patch
@(
    '// THIS IS AN AUTOGENERATED FILE. DO NOT EDIT THIS FILE DIRECTLY.',
    "tauri.android.versionName=$versionName",
    "tauri.android.versionCode=$versionCode"
) | Set-Content (Join-Path $appDir 'tauri.properties') -Encoding UTF8
Write-Host "==> App version $versionName ($versionCode)"

# ------------------------------------------------------------------ rust
$hostTag = if ($env:OS -eq 'Windows_NT') { 'windows-x86_64' } else { 'linux-x86_64' }
$toolchain = Join-Path $ndk "toolchains\llvm\prebuilt\$hostTag\bin"
$profile = if ($Debug) { 'debug' } else { 'release' }
$minSdk = 24

$androidAbis = @{
    'arm64-v8a'   = @{ Triple = 'aarch64-linux-android'; Clang = 'aarch64-linux-android' }
    'armeabi-v7a' = @{ Triple = 'armv7-linux-androideabi'; Clang = 'armv7a-linux-androideabi' }
    'x86_64'      = @{ Triple = 'x86_64-linux-android'; Clang = 'x86_64-linux-android' }
    'x86'         = @{ Triple = 'i686-linux-android'; Clang = 'i686-linux-android' }
}

# Environment the Tauri/Wry build scripts expect on Android builds. The Kotlin
# glue (Rust.kt, Ipc.kt, TauriActivity.kt, ...) is regenerated on every build.
$identifier = $config.identifier
$javaPackageDir = Join-Path $appDir ("src\main\java\" + ($identifier -replace '\.', '\') + '\generated')
New-Item -ItemType Directory -Force -Path $javaPackageDir | Out-Null
$env:WRY_ANDROID_PACKAGE = $identifier
$env:WRY_ANDROID_LIBRARY = 'pdf_sak_lib'
$env:WRY_ANDROID_KOTLIN_FILES_OUT_DIR = $javaPackageDir
$env:TAURI_ANDROID_PACKAGE_UNESCAPED = $identifier

foreach ($archAbi in $Abi) {
    $info = $androidAbis[$archAbi]
    $linker = Join-Path $toolchain "$($info.Clang)$minSdk-clang.cmd"
    if (-not (Test-Path $linker)) {
        $linker = Join-Path $toolchain "$($info.Clang)$minSdk-clang"
    }
    if (-not (Test-Path $linker)) { throw "linker not found for $archAbi in $toolchain" }
    $clangxx = $linker -replace '-clang(\.cmd)?$', '-clang++$1'
    $ar = Join-Path $toolchain 'llvm-ar.exe'
    if (-not (Test-Path $ar)) { $ar = Join-Path $toolchain 'llvm-ar' }

    $envKey = $info.Triple.ToUpperInvariant().Replace('-', '_')
    [Environment]::SetEnvironmentVariable("CARGO_TARGET_${envKey}_LINKER", $linker, 'Process')
    [Environment]::SetEnvironmentVariable("CARGO_TARGET_${envKey}_RUSTFLAGS", '-Clink-arg=-landroid -Clink-arg=-llog -Clink-arg=-lOpenSLES', 'Process')
    [Environment]::SetEnvironmentVariable('TARGET_CC', $linker, 'Process')
    [Environment]::SetEnvironmentVariable('TARGET_CXX', $clangxx, 'Process')
    [Environment]::SetEnvironmentVariable('TARGET_AR', $ar, 'Process')
    [Environment]::SetEnvironmentVariable('ANDROID_NATIVE_API_LEVEL', "$minSdk", 'Process')
    [Environment]::SetEnvironmentVariable('TAURI_ANDROID_PROJECT_PATH', $genDir, 'Process')

    $cargoArgs = @(
        'build',
        '--package', 'pdf-swiss-army-knife',
        '--manifest-path', (Join-Path $tauriDir 'Cargo.toml'),
        '--target', $info.Triple,
        '--lib',
        '--features', 'tauri/custom-protocol'
    )
    if (-not $Debug) { $cargoArgs += '--release' }

    Write-Host "==> Cargo build $archAbi ($profile)"
    Invoke-Native -Command 'cargo' -Arguments $cargoArgs -What "cargo build for $archAbi"

    $built = Join-Path $root "target\$($info.Triple)\$profile\libpdf_sak_lib.so"
    if (-not (Test-Path $built)) { throw "library not found at $built" }
    $jniDir = Join-Path $appDir "src\main\jniLibs\$archAbi"
    New-Item -ItemType Directory -Force -Path $jniDir | Out-Null
    Copy-Item $built (Join-Path $jniDir 'libpdf_sak_lib.so') -Force

    # C++ runtime is only needed when a dependency links against it.
    $bytes = [System.IO.File]::ReadAllBytes($built)
    $text = [System.Text.Encoding]::ASCII.GetString($bytes, 0, [Math]::Min($bytes.Length, 4000000))
    if ($text.Contains('libc++_shared.so')) {
        $cxx = Join-Path $ndk "toolchains\llvm\prebuilt\$hostTag\sysroot\usr\lib\$($info.Clang)\libc++_shared.so"
        if (Test-Path $cxx) { Copy-Item $cxx (Join-Path $jniDir 'libc++_shared.so') -Force }
    }
    Write-Host "    -> src/main/jniLibs/$archAbi/libpdf_sak_lib.so"
}

# ------------------------------------------------------------------ gradle
$flavorByAbi = @{
    'arm64-v8a'   = 'arm64'
    'armeabi-v7a' = 'arm'
    'x86_64'      = 'x86_64'
    'x86'         = 'x86'
}
$buildType = if ($Debug) { 'Debug' } else { 'Release' }

foreach ($archAbi in $Abi) {
    $flavor = $flavorByAbi[$archAbi]
    $task = "assemble$($flavor.Substring(0,1).ToUpperInvariant())$($flavor.Substring(1))$buildType"
    Write-Host "==> Gradle $task ($archAbi)"
    Invoke-Native -Command (Join-Path $genDir 'gradlew.bat') -Arguments @(
        $task, '--console=plain', "-PndkDir=$ndk"
    ) -What "gradle $task" -WorkingDirectory $genDir

    $apkDir = Join-Path $appDir "build\outputs\apk\$flavor\$($buildType.ToLowerInvariant())"
    $apks = Get-ChildItem $apkDir -Filter '*.apk' -ErrorAction SilentlyContinue
    if (-not $apks) { throw "no APK produced in $apkDir" }
    $outDir = Join-Path $root 'release-artifacts'
    New-Item -ItemType Directory -Force -Path $outDir | Out-Null
    foreach ($apk in $apks) {
        # Release-asset name contract: PDF-Swiss-Army-Knife-Android-<version>-<abi>.apk
        $target = Join-Path $outDir ("PDF-Swiss-Army-Knife-Android-{0}-{1}.apk" -f $versionName, $archAbi)
        Copy-Item $apk.FullName $target -Force
        Write-Host ("    -> {0} ({1:N1} MB)" -f $target, ((Get-Item $target).Length / 1MB))
    }

    if ($Bundle) {
        $bundleTask = "bundle$($flavor.Substring(0,1).ToUpperInvariant())$($flavor.Substring(1))$buildType"
        Write-Host "==> Gradle $bundleTask ($archAbi)"
        Invoke-Native -Command (Join-Path $genDir 'gradlew.bat') -Arguments @(
            $bundleTask, '--console=plain', "-PndkDir=$ndk"
        ) -What "gradle $bundleTask" -WorkingDirectory $genDir

        $aabDir = Join-Path $appDir "build\outputs\bundle\$flavor$buildType"
        $aabs = Get-ChildItem $aabDir -Filter '*.aab' -ErrorAction SilentlyContinue
        if (-not $aabs) { throw "no AAB produced in $aabDir" }
        foreach ($aab in $aabs) {
            $target = Join-Path $outDir ("PDF-Swiss-Army-Knife-Android-{0}-{1}.aab" -f $versionName, $archAbi)
            Copy-Item $aab.FullName $target -Force
            Write-Host ("    -> {0} ({1:N1} MB)" -f $target, ((Get-Item $target).Length / 1MB))
        }
    }
}

Write-Host 'Done.'
