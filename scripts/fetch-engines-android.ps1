# PDF Swiss Army Knife - Android engine fetcher
# Downloads the native engines used by the Android build into
#   src-tauri/resources/engines-android/<abi>/   (packaged as jniLibs)
#   src-tauri/resources/android-assets/tessdata/ (packaged as APK assets)
#
#   libpdfium.so    - PDF rendering (BSD-3-Clause, prebuilt by bblanchon/pdfium-binaries)
#   libtesseract.so - Tesseract CLI for Android (Apache-2.0, prebuilt by agnostic-apollo/tesseract-for-android)
#   tessdata_fast   - language models (Apache-2.0)
#
# Usage: powershell -NoProfile -ExecutionPolicy Bypass -File scripts/fetch-engines-android.ps1 [-Force]

param(
    [switch]$Force,
    [switch]$UpdateLock,
    [string[]]$Abis = @('arm64-v8a', 'armeabi-v7a', 'x86_64', 'x86')
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

# `powershell -File script.ps1 -Abis a,b` passes "a,b" as a single string, so
# accept both forms.
$Abis = @($Abis | ForEach-Object { $_ -split ',' } | ForEach-Object { $_.Trim() } | Where-Object { $_ })

$root = Split-Path -Parent $PSScriptRoot
$enginesDir = Join-Path $root 'src-tauri/resources/engines-android'
$assetsDir = Join-Path $root 'src-tauri/resources/android-assets/tessdata'
$desktopTessdata = Join-Path $root 'src-tauri/resources/engines/tesseract/tessdata'
$fontsDir = Join-Path $root 'crates/pdfcore/assets/fonts'
$cacheBase = if ($env:LOCALAPPDATA) { $env:LOCALAPPDATA } else { [System.IO.Path]::GetTempPath() }
$cacheDir = Join-Path $cacheBase 'pdf-sak-cache'
$isWindowsHost = [bool]($env:OS -eq 'Windows_NT') -or [bool]$IsWindows
$hostTag = if ($isWindowsHost) { 'windows-x86_64' } else { 'linux-x86_64' }

New-Item -ItemType Directory -Force -Path $enginesDir, $assetsDir, $cacheDir | Out-Null

# ---------------------------------------------------------------- supply chain
# Every artifact below is pinned by SHA-256 in engines.lock.json: a download that
# does not match stops the build instead of shipping an unverified binary. When an
# upstream release is updated on purpose, verify it and re-pin with -UpdateLock.
$lockPath = Join-Path $PSScriptRoot 'engines.lock.json'
$lock = if (Test-Path $lockPath) { Get-Content $lockPath -Raw | ConvertFrom-Json } else { [pscustomobject]@{ comment = ''; artifacts = [pscustomobject]@{} } }

function Get-PinnedHash {
    param([string]$Url)
    $entry = $lock.artifacts.PSObject.Properties[$Url]
    if (-not $entry) { return $null }
    return [string]$entry.Value.sha256
}

function Set-PinnedHash {
    param([string]$Url, [string]$Sha256)
    $artifacts = [ordered]@{}
    foreach ($property in $lock.artifacts.PSObject.Properties) { $artifacts[$property.Name] = $property.Value }
    $artifacts[$Url] = [pscustomobject]@{ sha256 = $Sha256 }
    $sorted = [ordered]@{}
    foreach ($key in ($artifacts.Keys | Sort-Object)) { $sorted[$key] = $artifacts[$key] }
    [pscustomobject]@{ comment = $lock.comment; artifacts = [pscustomobject]$sorted } |
        ConvertTo-Json -Depth 6 | Set-Content -Path $lockPath -Encoding UTF8
}

$script:unpinned = New-Object System.Collections.Generic.List[string]

function Assert-Artifact {
    param([string]$Url, [string]$Path, [switch]$Keep)
    $actual = (Get-FileHash -Algorithm SHA256 -Path $Path).Hash.ToLower()
    $expected = Get-PinnedHash -Url $Url
    if (-not $expected) {
        if ($UpdateLock) { Set-PinnedHash -Url $Url -Sha256 $actual; Write-Host "  pinned now: $actual"; return }
        if (-not $script:unpinned.Contains($Url)) { $script:unpinned.Add($Url) }
        return
    }
    if ($actual -eq $expected) { return }
    if ($UpdateLock) { Set-PinnedHash -Url $Url -Sha256 $actual; Write-Host "  re-pinned: $actual"; return }
    throw ("SHA-256 mismatch for " + $Url + "`n  expected " + $expected + "`n  actual   " + $actual + "`n" +
        "Refusing to use the file. If the upstream release changed on purpose, verify it and re-run with -UpdateLock.")
}

function Download-File {
    param([string]$Url, [string]$OutFile)
    if ((Test-Path $OutFile) -and -not $Force) {
        Assert-Artifact -Url $Url -Path $OutFile
        Write-Host "  cached: $OutFile"
        return
    }
    Write-Host "  downloading: $Url"
    $tmp = "$OutFile.part"
    Invoke-WebRequest -Uri $Url -OutFile $tmp -UseBasicParsing -Headers @{ 'User-Agent' = 'pdf-sak-build' }
    try { Assert-Artifact -Url $Url -Path $tmp } catch { Remove-Item -Force $tmp -ErrorAction SilentlyContinue; throw }
    Move-Item -Force $tmp $OutFile
}

function Find-NdkStrip {
    $candidates = @()
    if ($env:ANDROID_NDK_HOME) { $candidates += $env:ANDROID_NDK_HOME }
    if ($env:NDK_HOME) { $candidates += $env:NDK_HOME }
    $sdkRoots = @($env:ANDROID_HOME, $env:ANDROID_SDK_ROOT)
    if ($env:LOCALAPPDATA) { $sdkRoots += (Join-Path $env:LOCALAPPDATA 'Android/Sdk') }
    foreach ($sdkRoot in $sdkRoots) {
        if (-not $sdkRoot) { continue }
        $ndkRoot = Join-Path $sdkRoot 'ndk'
        if (Test-Path $ndkRoot) {
            $candidates += (Get-ChildItem $ndkRoot -Directory | Sort-Object Name -Descending | Select-Object -ExpandProperty FullName)
        }
    }
    foreach ($ndk in $candidates) {
        foreach ($name in @('llvm-strip', 'llvm-strip.exe')) {
            $strip = Join-Path $ndk "toolchains/llvm/prebuilt/$hostTag/bin/$name"
            if (Test-Path $strip) { return $strip }
        }
    }
    return $null
}

$pinned = @{
    pdfiumUrl  = 'https://github.com/bblanchon/pdfium-binaries/releases/download/chromium%2F8057/pdfium-android-{0}.tgz'
    tessUrl    = 'https://github.com/agnostic-apollo/tesseract-for-android/releases/download/v1.0.0/tesseract-binaries-v1.0.0.zip'
    tessLangs  = @('eng', 'tur', 'deu', 'nld', 'fra', 'spa', 'ita', 'bul', 'osd')
}

$pdfiumArch = @{
    'arm64-v8a'   = 'arm64'
    'armeabi-v7a' = 'arm'
    'x86_64'      = 'x64'
    'x86'         = 'x86'
}

$strip = Find-NdkStrip
if (-not $strip) { Write-Warning 'llvm-strip not found (install the Android NDK); tesseract binaries stay unstripped and much larger' }

# ---------------------------------------------------------------- pdfium
$pdfiumLicenseDir = Join-Path $enginesDir 'licenses/pdfium'
foreach ($archAbi in $Abis) {
    $target = Join-Path (Join-Path $enginesDir $archAbi) 'libpdfium.so'
    if ((Test-Path $target) -and -not $Force) { Write-Host "==> pdfium $archAbi (already present)"; continue }
    Write-Host "==> pdfium $archAbi"
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $target) | Out-Null
    $tgz = Join-Path $cacheDir "pdfium-android-$($pdfiumArch[$archAbi]).tgz"
    Download-File -Url ($pinned.pdfiumUrl -f $pdfiumArch[$archAbi]) -OutFile $tgz
    $tmp = Join-Path $cacheDir "pdfium-android-$archAbi-x"
    Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $tmp
    New-Item -ItemType Directory -Force -Path $tmp | Out-Null
    tar -xzf $tgz -C $tmp
    Copy-Item (Join-Path $tmp 'lib/libpdfium.so') $target -Force
    New-Item -ItemType Directory -Force -Path $pdfiumLicenseDir | Out-Null
    Copy-Item (Join-Path $tmp 'LICENSE') (Join-Path $pdfiumLicenseDir 'LICENSE') -Force -ErrorAction SilentlyContinue
    Copy-Item (Join-Path $tmp 'licenses/*') $pdfiumLicenseDir -Force -ErrorAction SilentlyContinue
    Write-Host "  -> $target"
}

# ---------------------------------------------------------------- tesseract CLI
$tessZip = Join-Path $cacheDir 'tesseract-android.zip'
$tessNeeded = $Abis | Where-Object { -not (Test-Path (Join-Path (Join-Path $enginesDir $_) 'libtesseract.so')) }
if ($tessNeeded -or $Force) {
    Write-Host '==> tesseract'
    Download-File -Url $pinned.tessUrl -OutFile $tessZip
    $tmp = Join-Path $cacheDir 'tesseract-android-x'
    Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $tmp
    Expand-Archive -Path $tessZip -DestinationPath $tmp -Force
    foreach ($archAbi in $Abis) {
        $source = Join-Path $tmp "tesseract-$archAbi"
        if (-not (Test-Path $source)) { Write-Warning "no tesseract build for $archAbi"; continue }
        $targetDir = Join-Path $enginesDir $archAbi
        New-Item -ItemType Directory -Force -Path $targetDir | Out-Null
        $target = Join-Path $targetDir 'libtesseract.so'
        Copy-Item $source $target -Force
        if ($strip) { & $strip $target }
        Write-Host ("  -> {0} ({1:N1} MB)" -f $target, ((Get-Item $target).Length / 1MB))
    }
} else { Write-Host '==> tesseract (already present)' }

# ---------------------------------------------------------------- tessdata (assets)
$tessDataAssets = $assetsDir
New-Item -ItemType Directory -Force -Path $tessDataAssets | Out-Null
foreach ($lang in $pinned.tessLangs) {
    $name = "$lang.traineddata"
    $target = Join-Path $tessDataAssets $name
    if ((Test-Path $target) -and -not $Force) { continue }
    $local = Join-Path $desktopTessdata $name
    if (Test-Path $local) {
        Write-Host "==> tessdata $lang (copy from desktop engines)"
        Copy-Item $local $target -Force
    } else {
        Write-Host "==> tessdata $lang"
        Download-File -Url "https://raw.githubusercontent.com/tesseract-ocr/tessdata_fast/main/$name" -OutFile $target
    }
}
# runtime configs (tesseract looks these up next to the language models)
foreach ($sub in @('configs', 'tessconfigs')) {
    $target = Join-Path $tessDataAssets $sub
    if ((Test-Path $target) -and -not $Force) { continue }
    $local = Join-Path $desktopTessdata $sub
    if (Test-Path $local) {
        Write-Host "==> tessdata $sub (copy from desktop engines)"
        Copy-Item $local $target -Recurse -Force
    } else {
        Write-Host "==> tessdata $sub (download)"
        $url = "https://github.com/tesseract-ocr/tesseract/archive/refs/tags/4.1.0.tar.gz"
        $tgz = Join-Path $cacheDir 'tesseract-src-4.1.0.tar.gz'
        Download-File -Url $url -OutFile $tgz
        $tmp = Join-Path $cacheDir 'tesseract-src'
        Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $tmp
        New-Item -ItemType Directory -Force -Path $tmp | Out-Null
        tar -xzf $tgz -C $tmp
        Copy-Item (Join-Path $tmp "tesseract-4.1.0/tessdata/$sub") $target -Recurse -Force
    }
}
$pdfTtf = Join-Path $tessDataAssets 'pdf.ttf'
if (-not (Test-Path $pdfTtf)) {
    $local = Join-Path $desktopTessdata 'pdf.ttf'
    if (Test-Path $local) { Copy-Item $local $pdfTtf -Force }
}
$license = Join-Path $tessDataAssets 'LICENSE.tessdata_fast.txt'
if (-not (Test-Path $license)) {
    Download-File -Url 'https://raw.githubusercontent.com/tesseract-ocr/tessdata_fast/main/LICENSE' -OutFile $license
}

# ---------------------------------------------------------------- bundled fonts
# pdfcore `include_bytes!`es these at compile time (text stamps + PDF/A font
# embedding), so the Android build needs them in the checkout. They are part of
# the repository; this step only restores them from the browser-extension
# vendor copy or the pinned Liberation release when a checkout is incomplete.
$bundledFonts = @(
    'PT_Sans-Web-Regular.ttf',
    'PT_Sans-Web-Bold.ttf',
    'LiberationSans-Regular.ttf',
    'LiberationSans-Italic.ttf',
    'LiberationSans-Bold.ttf',
    'LiberationSans-BoldItalic.ttf'
)
$fontVendorDirs = @(
    (Join-Path $root 'crates/pdfcore/assets/fonts'),
    (Join-Path $root 'chrome-extension/public/vendor/standard_fonts')
)
$missingFonts = @($bundledFonts | Where-Object { -not (Test-Path (Join-Path $fontsDir $_)) })
if ($missingFonts.Count -gt 0) {
    Write-Host '==> bundled fonts'
    New-Item -ItemType Directory -Force -Path $fontsDir | Out-Null
    foreach ($f in $missingFonts) {
        $local = $fontVendorDirs | ForEach-Object { Join-Path $_ $f } | Where-Object { Test-Path $_ } | Select-Object -First 1
        if ($local) {
            Copy-Item $local (Join-Path $fontsDir $f) -Force
            Write-Host "  -> $f (copy)"
        } elseif ($f -like 'LiberationSans-*') {
            $archive = Join-Path $cacheDir 'liberation-fonts-ttf-2.1.5.tar.gz'
            Download-File -Url 'https://github.com/liberationfonts/liberation-fonts/files/7261482/liberation-fonts-ttf-2.1.5.tar.gz' -OutFile $archive
            $tmp = Join-Path $cacheDir 'liberation-x'
            if (-not (Test-Path (Join-Path $tmp $f))) {
                Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $tmp
                New-Item -ItemType Directory -Force -Path $tmp | Out-Null
                tar -xzf $archive -C $tmp
            }
            $found = Get-ChildItem $tmp -Recurse -Filter $f | Select-Object -First 1
            if (-not $found) { throw "liberation archive did not contain $f" }
            Copy-Item $found.FullName (Join-Path $fontsDir $f) -Force
            Write-Host "  -> $f (download)"
        } else {
            Write-Warning "bundled font $f is missing; text stamp rendering and PDF/A embedding will fail to compile"
        }
    }
} else { Write-Host '==> bundled fonts (already present)' }

# ---------------------------------------------------------------- verify
if ($script:unpinned.Count -gt 0) {
    Write-Warning ("not pinned yet: " + ($script:unpinned -join ', '))
    Write-Warning 'Verify these sources, then re-run with -UpdateLock to record their SHA-256 in engines.lock.json.'
}
Write-Host ''
Write-Host 'Android engine status:'
foreach ($archAbi in $Abis) {
    $dir = Join-Path $enginesDir $archAbi
    $pdfium = Join-Path $dir 'libpdfium.so'
    $tess = Join-Path $dir 'libtesseract.so'
    $pdfiumSize = if (Test-Path $pdfium) { '{0:N1} MB' -f ((Get-Item $pdfium).Length / 1MB) } else { 'missing' }
    $tessSize = if (Test-Path $tess) { '{0:N1} MB' -f ((Get-Item $tess).Length / 1MB) } else { 'missing' }
    Write-Host ("  {0,-12} pdfium: {1,-9} tesseract: {2}" -f $archAbi, $pdfiumSize, $tessSize)
}
Write-Host "  tessdata: $((Get-ChildItem $tessDataAssets -Filter *.traineddata).Count) languages"
Write-Host 'Done.'
