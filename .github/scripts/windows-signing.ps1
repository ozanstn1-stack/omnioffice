# Authenticode signing for the Windows release artifacts (#501).
#
# The provider is chosen by the workflow from the available secrets; with no
# secrets the release job skips this script entirely and ships the installer
# unsigned, exactly as before. Supported providers:
#
#   pfx    - a base64 PKCS#12 certificate in WINDOWS_CERT_PFX whose password is
#            WINDOWS_CERT_PASSWORD. Works, but CA/Browser Forum requires code
#            signing keys in hardware since 2023, so treat it as the legacy /
#            test path (see docs/signing.md).
#   azure  - Azure Trusted Signing: AZURE_TENANT_ID, AZURE_CLIENT_ID,
#            AZURE_CLIENT_SECRET, AZURE_TS_ENDPOINT, AZURE_TS_ACCOUNT and
#            AZURE_TS_PROFILE. Uses Microsoft's Trusted Signing dlib with
#            signtool; the client version is pinned.
#
# SignPath.io runs through its GitHub Action in release.yml instead (it signs an
# uploaded artifact and returns it), so this script does not handle it.
#
# Modes:
#   -Action sign   sign the app exe and the NSIS installer in target/release
#   -Action verify verify either the default files or -Paths (semicolon list)
param(
    [Parameter(Mandatory = $true)][ValidateSet('sign', 'verify')][string]$Action,
    [ValidateSet('pfx', 'azure')][string]$Provider,
    [string]$Paths = ''
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$version = (Get-Content (Join-Path $root 'src-tauri/tauri.conf.json') -Raw | ConvertFrom-Json).version
$timestampUrl = 'http://timestamp.digicert.com'

function Find-SignTool {
    $candidates = @()
    if ($env:SIGNTOOL_PATH) { $candidates += $env:SIGNTOOL_PATH }
    $command = Get-Command signtool.exe -ErrorAction SilentlyContinue
    if ($command) { $candidates += $command.Source }
    foreach ($base in @(${env:ProgramFiles(x86)}, $env:ProgramFiles)) {
        if (-not $base) { continue }
        $kits = Join-Path $base 'Windows Kits/10/bin'
        if (-not (Test-Path $kits)) { continue }
        $candidates += Get-ChildItem $kits -Recurse -Filter signtool.exe -ErrorAction SilentlyContinue |
            Where-Object { $_.FullName -match '\\x64\\' } |
            Sort-Object FullName -Descending |
            Select-Object -ExpandProperty FullName
    }
    $found = $candidates | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
    if (-not $found) { throw 'signtool.exe was not found; the Windows SDK is required for Authenticode signing.' }
    return $found
}

function Get-ReleaseFiles {
    $files = @()
    $appExe = Join-Path $root 'target/release/OmniOffice.exe'
    if (-not (Test-Path $appExe)) { $appExe = Join-Path $root 'target/release/pdf-swiss-army-knife.exe' }
    if (-not (Test-Path $appExe)) { throw "the application exe was not found under target/release (expected OmniOffice.exe)" }
    $files += $appExe

    $nsisDir = Join-Path $root 'target/release/bundle/nsis'
    $installer = Get-ChildItem $nsisDir -Filter "*-$version-*-setup.exe" -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTime -Descending | Select-Object -First 1
    if (-not $installer) {
        $installer = Get-ChildItem $nsisDir -Filter '*-setup.exe' -ErrorAction SilentlyContinue |
            Sort-Object LastWriteTime -Descending | Select-Object -First 1
    }
    if (-not $installer) { throw "the NSIS installer was not found under $nsisDir; run tauri build first" }
    if ($installer.Name -notlike "*$version*") { throw "the newest installer ($($installer.Name)) does not match version $version" }
    $files += $installer.FullName
    return $files
}

function Invoke-SignTool([string[]]$arguments) {
    & $script:signTool @arguments
    if ($LASTEXITCODE -ne 0) { throw "signtool failed with exit code $LASTEXITCODE ($arguments -join ' ')" }
}

function Assert-Signed([string[]]$files) {
    $failed = @()
    foreach ($file in $files) {
        $signature = Get-AuthenticodeSignature $file
        Write-Host ("{0}: {1} ({2})" -f (Split-Path -Leaf $file), $signature.Status, $signature.SignerCertificate.Subject)
        if ($signature.Status -ne 'Valid') { $failed += (Split-Path -Leaf $file) }
    }
    if ($failed.Count -gt 0) { throw "not validly signed: $($failed -join ', ')" }
}

switch ($Action) {
    'sign' {
        if (-not $Provider) { throw 'sign requires -Provider pfx|azure' }
        $script:signTool = Find-SignTool
        $files = Get-ReleaseFiles
        Write-Host "signing with $Provider ($($files.Count) file(s)); signtool: $script:signTool"

        if ($Provider -eq 'pfx') {
            if (-not $env:WINDOWS_CERT_PFX) { throw 'WINDOWS_CERT_PFX is empty' }
            if (-not $env:WINDOWS_CERT_PASSWORD) { throw 'WINDOWS_CERT_PASSWORD is empty' }
            $pfx = Join-Path $env:RUNNER_TEMP 'authenticode.pfx'
            [System.IO.File]::WriteAllBytes($pfx, [Convert]::FromBase64String($env:WINDOWS_CERT_PFX))
            try {
                foreach ($file in $files) {
                    Invoke-SignTool @('sign', '/fd', 'SHA256', '/f', $pfx, '/p', $env:WINDOWS_CERT_PASSWORD,
                        '/tr', $timestampUrl, '/td', 'SHA256', $file)
                }
            } finally {
                Remove-Item $pfx -Force -ErrorAction SilentlyContinue
            }
        } elseif ($Provider -eq 'azure') {
            foreach ($name in @('AZURE_TENANT_ID', 'AZURE_CLIENT_ID', 'AZURE_CLIENT_SECRET',
                    'AZURE_TS_ENDPOINT', 'AZURE_TS_ACCOUNT', 'AZURE_TS_PROFILE')) {
                if (-not (Get-Item "env:$name" -ErrorAction SilentlyContinue).Value) { throw "$name is empty" }
            }
            # Microsoft.Trusted.Signing.Client ships the dlib that signtool loads.
            $clientVersion = '1.0.30'
            $clientDir = Join-Path $env:RUNNER_TEMP 'trusted-signing-client'
            $archive = Join-Path $env:RUNNER_TEMP "Microsoft.Trusted.Signing.Client.$clientVersion.zip"
            if (-not (Test-Path $clientDir)) {
                Invoke-WebRequest -Uri "https://www.nuget.org/api/v2/package/Microsoft.Trusted.Signing.Client/$clientVersion" -OutFile $archive
                Expand-Archive -LiteralPath $archive -DestinationPath $clientDir -Force
            }
            $dlib = Join-Path $clientDir 'bin/x64/Azure.CodeSigning.Dlib.dll'
            if (-not (Test-Path $dlib)) { throw "the Trusted Signing dlib was not found at $dlib" }
            $metadata = @{
                Endpoint                = $env:AZURE_TS_ENDPOINT
                CodeSigningAccountName  = $env:AZURE_TS_ACCOUNT
                CertificateProfileName  = $env:AZURE_TS_PROFILE
            } | ConvertTo-Json
            $metadataPath = Join-Path $env:RUNNER_TEMP 'trusted-signing-metadata.json'
            Set-Content -Path $metadataPath -Value $metadata -Encoding ascii
            foreach ($file in $files) {
                Invoke-SignTool @('sign', '/fd', 'SHA256', '/tr', 'http://timestamp.acs.microsoft.com', '/td', 'SHA256',
                    '/dlib', $dlib, '/dmdf', $metadataPath, $file)
            }
        }

        Assert-Signed $files
    }
    'verify' {
        if ($Paths) {
            $files = $Paths.Split(';') | Where-Object { $_ } | ForEach-Object { (Resolve-Path $_).Path }
        } else {
            $files = Get-ReleaseFiles
        }
        Assert-Signed $files
    }
    default { throw "unknown action $Action" }
}
