# PDF Swiss Army Knife - engine fetcher
# Downloads the native engines used by the app into src-tauri/resources/engines.
# All engines are open source and licensed permissively (see README "Third-party licenses").
#
#   pdfium.dll   - PDF rendering (BSD-3-Clause, prebuilt by bblanchon/pdfium-binaries)
#   qpdf.exe     - PDF security / structural engine (Apache-2.0)
#   tesseract    - OCR engine (Apache-2.0) + tessdata_fast language models (Apache-2.0)
#   PT Sans font - text stamp rendering (SIL OFL 1.1)
#
# Usage: powershell -NoProfile -ExecutionPolicy Bypass -File scripts/fetch-engines.ps1 [-Force]

param(
    [switch]$Force
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$root = Split-Path -Parent $PSScriptRoot
$enginesDir = Join-Path $root 'src-tauri\resources\engines'
$fontsDir = Join-Path $root 'crates\pdfcore\assets\fonts'
$cacheDir = Join-Path $env:LOCALAPPDATA 'pdf-sak-cache'

New-Item -ItemType Directory -Force -Path $enginesDir, $fontsDir, $cacheDir | Out-Null

function Download-File {
    param([string]$Url, [string]$OutFile)
    if ((Test-Path $OutFile) -and -not $Force) {
        Write-Host "  cached: $OutFile"
        return
    }
    Write-Host "  downloading: $Url"
    $tmp = "$OutFile.part"
    Invoke-WebRequest -Uri $Url -OutFile $tmp -UseBasicParsing -Headers @{ 'User-Agent' = 'pdf-sak-build' }
    Move-Item -Force $tmp $OutFile
}

$pinned = @{
    pdfiumUrl = 'https://github.com/bblanchon/pdfium-binaries/releases/download/chromium%2F8057/pdfium-win-x64.tgz'
    qpdfUrl   = 'https://github.com/qpdf/qpdf/releases/download/v12.4.1/qpdf-12.4.1-msvc64.zip'
    tessUrl   = 'https://github.com/UB-Mannheim/tesseract/releases/download/v5.4.0.20240606/tesseract-ocr-w64-setup-5.4.0.20240606.exe'
    tessLangs = @('eng', 'tur', 'deu', 'nld', 'fra', 'spa', 'ita', 'bul', 'osd')
}

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
    Download-File -Url 'https://www.7-zip.org/a/7zr.exe' -OutFile $sevenZr
}
$sevenZip = Join-Path $cacheDir '7zip-full\7z.exe'
if (-not (Test-Path $sevenZip)) {
    $sevenInstaller = Join-Path $cacheDir '7z-installer.exe'
    Download-File -Url 'https://www.7-zip.org/a/7z2301-x64.exe' -OutFile $sevenInstaller
    $sevenTmp = Join-Path $cacheDir '7zip-full'
    Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $sevenTmp
    New-Item -ItemType Directory -Force -Path $sevenTmp | Out-Null
    & $sevenZr x $sevenInstaller "-o$sevenTmp" -y | Out-Null
    if (-not (Test-Path $sevenZip)) { throw 'failed to bootstrap 7z.exe' }
}

# ---------------------------------------------------------------- tesseract
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
    Get-ChildItem $tmp -Filter '*.exe' | Copy-Item -Destination $tessDir -Force
    Get-ChildItem $tmp -Filter '*.dll' | Copy-Item -Destination $tessDir -Force
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
        Download-File -Url "https://raw.githubusercontent.com/tesseract-ocr/tessdata_fast/main/$lang.traineddata" -OutFile $target
    }
}
# keep tessdata_fast license text next to the models
$lic = Join-Path $tessDataDir 'LICENSE.tessdata_fast.txt'
if (-not (Test-Path $lic)) {
    Download-File -Url 'https://raw.githubusercontent.com/tesseract-ocr/tessdata_fast/main/LICENSE' -OutFile $lic
}
# GlyphLessFont needed by tesseract's PDF renderer (searchable PDF output);
# it is copied out of the installer payload during extraction, with a direct
# download as a fallback so a stripped payload can never break OCR silently.
$pdfFont = Join-Path $tessDataDir 'pdf.ttf'
if (-not (Test-Path $pdfFont)) {
    try {
        Download-File -Url 'https://raw.githubusercontent.com/tesseract-ocr/tesseract/main/tessdata/pdf.ttf' -OutFile $pdfFont
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
        Download-File -Url "https://raw.githubusercontent.com/google/fonts/main/ofl/ptsans/$f" -OutFile $target
    }
}
$ofl = Join-Path $fontsDir 'OFL.txt'
if (-not (Test-Path $ofl)) {
    Download-File -Url 'https://raw.githubusercontent.com/google/fonts/main/ofl/ptsans/OFL.txt' -OutFile $ofl
}

# ---------------------------------------------------------------- verify
Write-Host ''
Write-Host 'Engine status:'
& $qpdfExe --version 2>&1 | Select-Object -First 1
$env:TESSDATA_PREFIX = $tessDataDir
& $tessExe --version 2>&1 | Select-Object -First 1
$env:TESSDATA_PREFIX = $null
Write-Host "pdfium.dll: $((Get-Item $pdfiumDll).Length) bytes"
Write-Host "tessdata languages: $((Get-ChildItem $tessDataDir -Filter *.traineddata).Count)"
Write-Host 'Done.'
