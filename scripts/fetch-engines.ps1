# OmniOffice - engine fetcher
# Downloads the native engines used by the app into src-tauri/resources/engines.
# All engines are open source and licensed permissively (see README "Third-party licenses").
#
#   pdfium.dll   - PDF rendering (BSD-3-Clause, prebuilt by bblanchon/pdfium-binaries)
#   qpdf.exe     - PDF security / structural engine (Apache-2.0)
#   tesseract    - OCR engine (Apache-2.0) + tessdata_fast language models (Apache-2.0)
#   PT Sans font - text stamp rendering (SIL OFL 1.1)
#   Liberation Sans fonts - PDF/A font embedding (SIL OFL 1.1)
#
# Usage: powershell -NoProfile -ExecutionPolicy Bypass -File scripts/fetch-engines.ps1 [-Force]

param(
    [switch]$Force,
    [switch]$UpdateLock,
    [switch]$VerifyLock
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$root = Split-Path -Parent $PSScriptRoot
$enginesDir = Join-Path $root 'src-tauri\resources\engines'
$fontsDir = Join-Path $root 'crates\pdfcore\assets\fonts'
$cacheBase = if ($env:LOCALAPPDATA) { $env:LOCALAPPDATA } else { [System.IO.Path]::GetTempPath() }
$cacheDir = Join-Path $cacheBase 'pdf-sak-cache'

# ---------------------------------------------------------------- supply chain
# Every artifact below is pinned by SHA-256 in engines.lock.json: a download
# that does not match stops the build instead of shipping an unverified binary.
# A missing entry is a hard failure too - never a warning. When an upstream
# release is updated on purpose, verify it and re-pin with -UpdateLock.
# `-VerifyLock` only checks that this script's whole download surface is
# covered by the lock and is what CI runs.
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
        ConvertTo-Json -Depth 6 | ForEach-Object { [System.IO.File]::WriteAllText($lockPath, $_, (New-Object System.Text.UTF8Encoding($false))) }
}

function Get-Sha256Hex {
    param([Parameter(Mandatory = $true)][string]$Path)
    # .NET is part of the engine, not of a module that can fail to autoload.
    # The Windows runner has been seen without Get-FileHash (it lives in
    # Microsoft.PowerShell.Utility), so verification must not depend on it.
    try {
        $sha = [System.Security.Cryptography.SHA256]::Create()
        $stream = [System.IO.File]::OpenRead($Path)
        try {
            return ([System.BitConverter]::ToString($sha.ComputeHash($stream))).Replace('-', '').ToLowerInvariant()
        } finally {
            $stream.Dispose()
            $sha.Dispose()
        }
    } catch {
        # Fall through to the alternatives below.
    }
    if (Get-Command Get-FileHash -ErrorAction SilentlyContinue) {
        return (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant()
    }
    if (Get-Command certutil.exe -ErrorAction SilentlyContinue) {
        foreach ($line in (& certutil.exe -hashfile $Path SHA256)) {
            $candidate = ($line -replace '[^0-9a-fA-F]', '')
            if ($candidate.Length -eq 64) { return $candidate.ToLowerInvariant() }
        }
    }
    throw "Cannot compute a SHA-256 for $Path on this host (.NET crypto, Get-FileHash and certutil are all unavailable)."
}

function Assert-Artifact {
    param([string]$Url, [string]$Path, [switch]$Keep)
    $actual = Get-Sha256Hex -Path $Path
    $expected = Get-PinnedHash -Url $Url
    if (-not $expected) {
        if ($UpdateLock) { Set-PinnedHash -Url $Url -Sha256 $actual; Write-Host "  pinned now: $actual"; return }
        throw ("No pinned SHA-256 for " + $Url + "`n" +
            "Refusing to use an unverified download. Review the source, then record it deliberately with -UpdateLock.")
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

# Every URL this script can download, kept next to the pinned map so a new
# download cannot be added without also passing -VerifyLock in CI.
$pinned = @{
    pdfiumUrl = 'https://github.com/bblanchon/pdfium-binaries/releases/download/chromium%2F8057/pdfium-win-x64.tgz'
    qpdfUrl   = 'https://github.com/qpdf/qpdf/releases/download/v12.4.1/qpdf-12.4.1-msvc64.zip'
    tessUrl   = 'https://github.com/UB-Mannheim/tesseract/releases/download/v5.4.0.20240606/tesseract-ocr-w64-setup-5.4.0.20240606.exe'
    tessLangs = @('eng', 'tur', 'deu', 'nld', 'fra', 'spa', 'ita', 'bul', 'osd')
    tessDataBase = 'https://raw.githubusercontent.com/tesseract-ocr/tessdata_fast/main'
    tessdataLicenseUrl = 'https://raw.githubusercontent.com/tesseract-ocr/tessdata_fast/main/LICENSE'
    pdfTtfUrl = 'https://raw.githubusercontent.com/tesseract-ocr/tesseract/main/tessdata/pdf.ttf'
    # Version-pinned release asset: the unversioned 7-zip.org/a/7zr.exe is
    # replaced in place on every 7-Zip release, which broke the pinned hash.
    sevenZrUrl = 'https://github.com/ip7z/7zip/releases/download/23.01/7zr.exe'
    sevenZipUrl = 'https://www.7-zip.org/a/7z2301-x64.exe'
    ptSansBase = 'https://raw.githubusercontent.com/google/fonts/main/ofl/ptsans'
    liberationUrl = 'https://github.com/liberationfonts/liberation-fonts/files/7261482/liberation-fonts-ttf-2.1.5.tar.gz'
}

$allArtifactUrls = @(
    $pinned.pdfiumUrl,
    $pinned.qpdfUrl,
    $pinned.tessUrl,
    $pinned.tessdataLicenseUrl,
    $pinned.pdfTtfUrl,
    $pinned.sevenZrUrl,
    $pinned.sevenZipUrl,
    $pinned.liberationUrl
)
$allArtifactUrls += $pinned.tessLangs | ForEach-Object { "$($pinned.tessDataBase)/$_.traineddata" }
$allArtifactUrls += @('PT_Sans-Web-Regular.ttf', 'PT_Sans-Web-Bold.ttf', 'OFL.txt') | ForEach-Object { "$($pinned.ptSansBase)/$_" }

if ($VerifyLock) {
    $missing = @($allArtifactUrls | Where-Object { -not (Get-PinnedHash -Url $_) })
    if ($missing.Count -gt 0) {
        throw ("engines.lock.json is missing SHA-256 entries for:`n  " + ($missing -join "`n  ") +
            "`nReview each source, then re-run the fetch script with -UpdateLock to record the hashes.")
    }
    Write-Host "engines.lock.json covers all $($allArtifactUrls.Count) artifacts this script can download."
    exit 0
}

New-Item -ItemType Directory -Force -Path $enginesDir, $fontsDir, $cacheDir | Out-Null

# ---------------------------------------------------------------- pdfium
$pdfiumDir = Join-Path $enginesDir 'pdfium'
$pdfiumDll = Join-Path $pdfiumDir 'pdfium.dll'
if ($Force -or -not (Test-Path $pdfiumDll)) {
    Write-Host '==> pdfium'
    New-Item -ItemType Directory -Force -Path $pdfiumDir | Out-Null
    $tgz = Join-Path $cacheDir 'pdfium-win-x64.tgz'
    Download-File -Url $pinned.pdfiumUrl -OutFile $tgz
    $tmp = Join-Path $cacheDir 'pdfium-x'
    Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $tmp
    New-Item -ItemType Directory -Force -Path $tmp | Out-Null
    tar -xzf $tgz -C $tmp
    Copy-Item (Join-Path $tmp 'bin\pdfium.dll') $pdfiumDll -Force
    Copy-Item (Join-Path $tmp 'LICENSE') (Join-Path $pdfiumDir 'LICENSE') -Force -ErrorAction SilentlyContinue
    Write-Host "  -> $pdfiumDll"
} else { Write-Host '==> pdfium (already present)' }

# ---------------------------------------------------------------- qpdf
$qpdfDir = Join-Path $enginesDir 'qpdf'
$qpdfExe = Join-Path $qpdfDir 'qpdf.exe'
if ($Force -or -not (Test-Path $qpdfExe)) {
    Write-Host '==> qpdf'
    New-Item -ItemType Directory -Force -Path $qpdfDir | Out-Null
    $zip = Join-Path $cacheDir 'qpdf-msvc64.zip'
    Download-File -Url $pinned.qpdfUrl -OutFile $zip
    $tmp = Join-Path $cacheDir 'qpdf-x'
    Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $tmp
    Expand-Archive -Path $zip -DestinationPath $tmp -Force
    $binDir = Get-ChildItem $tmp -Recurse -Directory -Filter 'bin' | Select-Object -First 1 -ExpandProperty FullName
    Copy-Item (Join-Path $binDir '*.exe') $qpdfDir -Force
    Copy-Item (Join-Path $binDir '*.dll') $qpdfDir -Force -ErrorAction SilentlyContinue
    $licenseDir = Get-ChildItem $tmp -Recurse -Directory -Filter 'doc' -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($licenseDir) { Copy-Item (Join-Path $licenseDir.FullName '..\*.txt') $qpdfDir -Force -ErrorAction SilentlyContinue }
    Copy-Item (Join-Path $tmp 'qpdf-*\README*') $qpdfDir -Force -ErrorAction SilentlyContinue
    Write-Host "  -> $qpdfExe"
} else { Write-Host '==> qpdf (already present)' }

# ---------------------------------------------------------------- 7-Zip (for NSIS extraction)
# 7zr.exe only reads .7z; the NSIS payload needs the full 7z.exe, which we
# bootstrap by extracting the official 7-Zip self-extracting installer.
$sevenZr = Join-Path $cacheDir '7zr.exe'
if (-not (Test-Path $sevenZr)) {
    Download-File -Url $pinned.sevenZrUrl -OutFile $sevenZr
}
$sevenZip = Join-Path $cacheDir '7zip-full\7z.exe'
if (-not (Test-Path $sevenZip)) {
    $sevenInstaller = Join-Path $cacheDir '7z-installer.exe'
    Download-File -Url $pinned.sevenZipUrl -OutFile $sevenInstaller
    $sevenTmp = Join-Path $cacheDir '7zip-full'
    Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $sevenTmp
    New-Item -ItemType Directory -Force -Path $sevenTmp | Out-Null
    & $sevenZr x $sevenInstaller "-o$sevenTmp" -y | Out-Null
    if (-not (Test-Path $sevenZip)) { throw 'failed to bootstrap 7z.exe' }
}

# ---------------------------------------------------------------- tesseract
# The UB-Mannheim installer carries the whole toolchain: tesseract.exe, 15
# training tools (text2image, lstmtraining, ...), winpath.exe, the uninstaller
# and 51 DLLs, 223 MB uncompressed. OmniOffice only ever runs tesseract.exe (see
# pdfcore::engines::tesseract_command), so only that exe and the DLLs it can
# actually load are shipped. The set is derived, not guessed: the static import
# table of tesseract.exe is followed recursively through the DLLs of the
# payload. For the pinned 5.4.0.20240606 build that is tesseract.exe plus 26
# DLLs, 116 MB (about 26 MB smaller in the compressed installer):
#   libtesseract-5, libleptonica-6, libarchive-13, libstdc++-6, libgcc_s_seh-1,
#   libwinpthread-1, libtiff-6, libjpeg-8, libgif-7, libopenjp2-7, libpng16-16,
#   libwebp-7, libwebpmux-3, libsharpyuv-0, libLerc, libdeflate, libjbig-0,
#   libb2-1, libbz2-1, libcrypto-3-x64, libexpat-1, libiconv-2, liblz4,
#   liblzma-5, libzstd, zlib1
# Not shipped: the pango/cairo/glib/harfbuzz/fontconfig/ICU/brotli stack (only
# text2image links it), the training exes, winpath.exe and the uninstaller. The
# payload has no delay-load tables and none of the binaries checked (tesseract,
# libtesseract, libleptonica, libarchive, libtiff) names another payload DLL as
# a string, so the static closure is the whole runtime set.
# Anything unexpected (a payload that cannot be parsed, a closure without
# libtesseract/libleptonica) falls back to copying every DLL, still without the
# training exes, instead of risking a tesseract.exe that cannot start.

# DLL names a PE file lists in its static import table, or $null when the file
# is not a PE image this parser understands.
function Get-PeImports {
    param([string]$Path)
    $b = [System.IO.File]::ReadAllBytes($Path)
    if ($b.Length -lt 0x40 -or $b[0] -ne 0x4D -or $b[1] -ne 0x5A) { return $null }
    $pe = [int64][BitConverter]::ToUInt32($b, 0x3C)
    if ($pe + 24 -gt $b.Length -or [BitConverter]::ToUInt32($b, [int]$pe) -ne 0x00004550) { return $null }
    $sectionCount = [int][BitConverter]::ToUInt16($b, [int]($pe + 6))
    $optionalSize = [int64][BitConverter]::ToUInt16($b, [int]($pe + 20))
    $optional = $pe + 24
    $magic = [BitConverter]::ToUInt16($b, [int]$optional)
    if ($magic -eq 0x20B) { $directories = $optional + 112 }
    elseif ($magic -eq 0x10B) { $directories = $optional + 96 }
    else { return $null }
    # Data directory 1 is the import table.
    $importRva = [int64][BitConverter]::ToUInt32($b, [int]($directories + 8))
    if ($importRva -eq 0) { return ,@() }
    $sections = $optional + $optionalSize
    $toOffset = {
        param([int64]$Rva)
        for ($i = 0; $i -lt $sectionCount; $i++) {
            $s = [int]($sections + 40 * $i)
            $virtualSize = [int64][BitConverter]::ToUInt32($b, $s + 8)
            $virtualAddress = [int64][BitConverter]::ToUInt32($b, $s + 12)
            $rawSize = [int64][BitConverter]::ToUInt32($b, $s + 16)
            $rawPointer = [int64][BitConverter]::ToUInt32($b, $s + 20)
            $span = [Math]::Max($virtualSize, $rawSize)
            if ($Rva -ge $virtualAddress -and $Rva -lt ($virtualAddress + $span)) { return ($Rva - $virtualAddress + $rawPointer) }
        }
        return -1
    }
    $names = New-Object System.Collections.ArrayList
    $descriptor = & $toOffset $importRva
    if ($descriptor -lt 0) { return $null }
    # One 20-byte descriptor per imported DLL; the table ends with a zeroed one.
    while ($descriptor + 20 -le $b.Length) {
        $nameRva = [int64][BitConverter]::ToUInt32($b, [int]($descriptor + 12))
        if ($nameRva -eq 0) { break }
        $nameOffset = & $toOffset $nameRva
        if ($nameOffset -lt 0) { return $null }
        $end = [int]$nameOffset
        while ($end -lt $b.Length -and $b[$end] -ne 0) { $end++ }
        [void]$names.Add([System.Text.Encoding]::ASCII.GetString($b, [int]$nameOffset, $end - [int]$nameOffset))
        $descriptor += 20
    }
    return ,@($names)
}

# Every exe/dll of $Dir that $Roots load, directly or through other payload
# DLLs (imports that are not in the payload are Windows system DLLs). Returns
# $null when any file cannot be parsed or a root is missing.
function Get-RuntimeClosure {
    param([string]$Dir, [string[]]$Roots)
    $present = @{}
    foreach ($file in Get-ChildItem -Path $Dir -File) {
        if ($file.Extension -in '.dll', '.exe') { $present[$file.Name] = $file.Name }
    }
    $needed = @{}
    $queue = New-Object System.Collections.ArrayList
    foreach ($rootName in $Roots) {
        if (-not $present.ContainsKey($rootName)) { return $null }
        $needed[$present[$rootName]] = $true
        [void]$queue.Add($present[$rootName])
    }
    while ($queue.Count -gt 0) {
        $name = [string]$queue[0]
        $queue.RemoveAt(0)
        $imports = Get-PeImports -Path (Join-Path $Dir $name)
        if ($null -eq $imports) { return $null }
        foreach ($dependency in $imports) {
            if ($present.ContainsKey($dependency) -and -not $needed.ContainsKey($present[$dependency])) {
                $needed[$present[$dependency]] = $true
                [void]$queue.Add($present[$dependency])
            }
        }
    }
    return ,@($needed.Keys | Sort-Object)
}

$tessDir = Join-Path $enginesDir 'tesseract'
$tessExe = Join-Path $tessDir 'tesseract.exe'
if ($Force -or -not (Test-Path $tessExe)) {
    Write-Host '==> tesseract'
    New-Item -ItemType Directory -Force -Path $tessDir | Out-Null
    $setup = Join-Path $cacheDir 'tesseract-setup.exe'
    Download-File -Url $pinned.tessUrl -OutFile $setup
    $tmp = Join-Path $cacheDir 'tesseract-x'
    Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $tmp
    New-Item -ItemType Directory -Force -Path $tmp | Out-Null
    & $sevenZip x $setup "-o$tmp" -y | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "7zr extraction failed with code $LASTEXITCODE" }
    # The installer payload puts the program at the root of $INSTDIR and the
    # runtime tessdata bits (configs, tessconfigs, pdf.ttf) under tessdata\.
    # Only tesseract.exe and the DLLs it loads are kept (see the note above);
    # stale files from an earlier, fuller fetch are removed so -Force really
    # produces the lean layout.
    Get-ChildItem -Path $tessDir -File | Where-Object { $_.Extension -in '.exe', '.dll' } | Remove-Item -Force
    $closure = $null
    try { $closure = Get-RuntimeClosure -Dir $tmp -Roots @('tesseract.exe') }
    catch { Write-Warning "could not follow the tesseract.exe imports: $($_.Exception.Message)" }
    $closureLooksRight = $closure -and ($closure | Where-Object { $_ -like 'libtesseract*' }) -and ($closure | Where-Object { $_ -like 'libleptonica*' })
    if ($closureLooksRight) {
        foreach ($name in $closure) { Copy-Item (Join-Path $tmp $name) $tessDir -Force }
        $shipped = @($closure).Count
        $total = @(Get-ChildItem -Path $tmp -File | Where-Object { $_.Extension -in '.exe', '.dll' }).Count
        Write-Host "  -> tesseract.exe + $($shipped - 1) DLLs of $total executables/libraries in the installer"
    } else {
        Write-Warning 'tesseract import closure unavailable or implausible: copying every DLL (training tools and the uninstaller are still left out)'
        Copy-Item (Join-Path $tmp 'tesseract.exe') $tessDir -Force
        Get-ChildItem $tmp -Filter '*.dll' | Copy-Item -Destination $tessDir -Force
    }
    $tessDataDir = Join-Path $tessDir 'tessdata'
    New-Item -ItemType Directory -Force -Path $tessDataDir | Out-Null
    $payloadData = Join-Path $tmp 'tessdata'
    # TESSDATA_PREFIX points at the tessdata directory, so tesseract looks for
    # output configs (pdf, hocr, ...) in <tessdata>\configs. Copying them next
    # to the executable instead silently broke searchable-PDF output: tesseract
    # printed "read_params_file: Can't open pdf", exited 0 and wrote only the
    # text file. Copy them where the engine actually reads them.
    foreach ($sub in @('configs', 'tessconfigs')) {
        $p = Join-Path $payloadData $sub
        if (Test-Path $p) {
            Copy-Item $p (Join-Path $tessDataDir $sub) -Recurse -Force
            Copy-Item $p (Join-Path $tessDir $sub) -Recurse -Force
        }
    }
    $payloadFont = Join-Path $payloadData 'pdf.ttf'
    if (Test-Path $payloadFont) { Copy-Item $payloadFont (Join-Path $tessDataDir 'pdf.ttf') -Force }
    Write-Host "  -> $tessExe"
} else { Write-Host '==> tesseract (already present)' }

$tessDataDir = Join-Path $tessDir 'tessdata'
New-Item -ItemType Directory -Force -Path $tessDataDir | Out-Null
foreach ($lang in $pinned.tessLangs) {
    $target = Join-Path $tessDataDir "$lang.traineddata"
    if ($Force -or -not (Test-Path $target)) {
        Download-File -Url "$($pinned.tessDataBase)/$lang.traineddata" -OutFile $target
    }
}
# keep tessdata_fast license text next to the models
$lic = Join-Path $tessDataDir 'LICENSE.tessdata_fast.txt'
if (-not (Test-Path $lic)) {
    Download-File -Url $pinned.tessdataLicenseUrl -OutFile $lic
}
# GlyphLessFont needed by tesseract's PDF renderer (searchable PDF output);
# it is copied out of the installer payload during extraction, with a direct
# download as a fallback so a stripped payload can never break OCR silently.
$pdfFont = Join-Path $tessDataDir 'pdf.ttf'
if (-not (Test-Path $pdfFont)) {
    try {
        Download-File -Url $pinned.pdfTtfUrl -OutFile $pdfFont
        Write-Host '  -> pdf.ttf (downloaded)'
    } catch {
        Write-Warning 'pdf.ttf missing: searchable PDF output may fail'
    }
}
# The PDF/hOCR output configs are what tesseract reads from <tessdata>\configs.
$pdfConfig = Join-Path $tessDataDir 'configs\pdf'
if (-not (Test-Path $pdfConfig)) {
    Write-Warning "tessdata\configs\pdf missing: searchable PDF output may fail"
}

# ---------------------------------------------------------------- fonts (PT Sans, OFL)
foreach ($f in @('PT_Sans-Web-Regular.ttf', 'PT_Sans-Web-Bold.ttf')) {
    $target = Join-Path $fontsDir $f
    if ($Force -or -not (Test-Path $target)) {
        Write-Host "==> font $f"
        Download-File -Url "$($pinned.ptSansBase)/$f" -OutFile $target
    }
}
$ofl = Join-Path $fontsDir 'OFL.txt'
if (-not (Test-Path $ofl)) {
    Download-File -Url "$($pinned.ptSansBase)/OFL.txt" -OutFile $ofl
}

# ---------------------------------------------------------------- fonts (Liberation Sans, OFL)
# Used by fontembed.rs as the metric-compatible substitute for Helvetica/Arial.
# The four faces already ship with the browser extension, so copy them when
# present (offline-friendly) and only fall back to the pinned release archive.
$liberationFiles = @(
    'LiberationSans-Regular.ttf',
    'LiberationSans-Italic.ttf',
    'LiberationSans-Bold.ttf',
    'LiberationSans-BoldItalic.ttf'
)
$liberationMissing = @($liberationFiles | Where-Object { $Force -or -not (Test-Path (Join-Path $fontsDir $_)) })
if ($liberationMissing.Count -gt 0) {
    $liberationVendor = Join-Path $root 'chrome-extension\public\vendor\standard_fonts'
    $missingFromVendor = @()
    foreach ($f in $liberationMissing) {
        $target = Join-Path $fontsDir $f
        $local = Join-Path $liberationVendor $f
        if (Test-Path $local) {
            Write-Host "==> font $f (copy from chrome-extension vendor)"
            Copy-Item $local $target -Force
        } else {
            $missingFromVendor += $f
        }
    }
    if ($missingFromVendor.Count -gt 0) {
        Write-Host '==> liberation fonts (download)'
        $archive = Join-Path $cacheDir 'liberation-fonts-ttf-2.1.5.tar.gz'
        Download-File -Url $pinned.liberationUrl -OutFile $archive
        $tmp = Join-Path $cacheDir 'liberation-x'
        Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $tmp
        New-Item -ItemType Directory -Force -Path $tmp | Out-Null
        tar -xzf $archive -C $tmp
        foreach ($f in $missingFromVendor) {
            $found = Get-ChildItem $tmp -Recurse -Filter $f | Select-Object -First 1
            if (-not $found) { throw "liberation archive did not contain $f" }
            Copy-Item $found.FullName (Join-Path $fontsDir $f) -Force
        }
    }
} else { Write-Host '==> liberation fonts (already present)' }

# ---------------------------------------------------------------- verify
Write-Host ''
Write-Host ('host: PowerShell ' + $PSVersionTable.PSVersion + ' (' + $ExecutionContext.SessionState.LanguageMode + ')')
Write-Host 'Engine status:'
& $qpdfExe --version 2>&1 | Select-Object -First 1
$env:TESSDATA_PREFIX = $tessDataDir
& $tessExe --version 2>&1 | Select-Object -First 1
$env:TESSDATA_PREFIX = $null
Write-Host "pdfium.dll: $((Get-Item $pdfiumDll).Length) bytes"
Write-Host "tessdata languages: $((Get-ChildItem $tessDataDir -Filter *.traineddata).Count)"
Write-Host 'Done.'
