# Applies a built release to this machine without requiring administrator
# rights: extracts the portable ZIP into a stable per-user folder and refreshes
# the Start Menu / Desktop shortcuts.
#
# The per-machine install (if any) lives in "C:\Program Files\Office Swiss Army
# Knife" and can only be replaced by running the NSIS installer elevated. This
# script is the always-works local update path; it never touches Program Files.
#
# Usage:
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts/install-local.ps1
#   powershell ... -File scripts/install-local.ps1 -Version 3.3.0 -Zip <path>
param(
    [string]$Version,
    [string]$Zip,
    [switch]$NoShortcuts
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot

if (-not $Version) {
    $Version = (Get-Content (Join-Path $root 'src-tauri\tauri.conf.json') -Raw | ConvertFrom-Json).version
}
if (-not $Zip) {
    $Zip = Join-Path $root "release-artifacts\Office-Swiss-Army-Knife-Portable-$Version.zip"
}
if (-not (Test-Path $Zip)) {
    throw "Portable ZIP not found: $Zip (run 'npm run package' first)"
}

$target = Join-Path $env:LOCALAPPDATA 'Programs\Office Swiss Army Knife'
$exeName = 'PDF-Swiss-Army-Knife.exe'

# The app must not be running while its files are replaced.
$running = Get-Process -Name 'PDF-Swiss-Army-Knife' -ErrorAction SilentlyContinue
if ($running) {
    throw "Close Office Swiss Army Knife before updating (running PID: $($running.Id -join ', '))."
}

Write-Host "==> Extracting $Zip"
Write-Host "    -> $target"
if (Test-Path $target) {
    Remove-Item -LiteralPath $target -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $target | Out-Null
Expand-Archive -LiteralPath $Zip -DestinationPath $target -Force

$exe = Join-Path $target $exeName
if (-not (Test-Path $exe)) {
    throw "The archive did not contain $exeName"
}

if (-not $NoShortcuts) {
    $shell = New-Object -ComObject WScript.Shell
    $shortcutPaths = @(
        (Join-Path ([Environment]::GetFolderPath('Programs')) 'Office Swiss Army Knife.lnk'),
        (Join-Path ([Environment]::GetFolderPath('Desktop')) 'Office Swiss Army Knife.lnk')
    )
    foreach ($shortcutPath in $shortcutPaths) {
        $shortcut = $shell.CreateShortcut($shortcutPath)
        $shortcut.TargetPath = $exe
        $shortcut.WorkingDirectory = $target
        $shortcut.Description = "Office Swiss Army Knife $Version"
        $shortcut.Save()
    }
    Write-Host "==> Shortcuts refreshed (Start Menu + Desktop)"
}

$installed = (Get-Item $exe).VersionInfo
Write-Host "==> Installed version: $($installed.ProductVersion) at $exe"
Write-Host "Done."
