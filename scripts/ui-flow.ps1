param(
    [Parameter(Mandatory=$true)][string]$Screen,
    [string]$Files = "",
    [Parameter(Mandatory=$true)][string]$Out,
    [string]$Actions = "",      # e.g. "click:1310,300;wait:3000;type:secret"
    [int]$WaitSeconds = 8,
    [double]$Scale = 0,
    [switch]$AutoRun,
    [switch]$Maximize,
    [string]$Exe = "D:\AI\projects\pdf-swiss-army-knife\target\release\OmniOffice.exe",
    [string]$NewTab = ""
)
# Drives a UI flow for validation/screenshots: launches the app with the dev
# hook, performs mouse/keyboard actions, then captures the window.
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class UiFlow {
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int cmd);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hWnd, IntPtr after, int x, int y, int cx, int cy, uint flags);
    [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr hWnd);
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
}
"@

Get-Process -Name 'OmniOffice', 'pdf-swiss-army-knife' -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Milliseconds 700

$env:PDFSAK_START_SCREEN = $Screen
$env:PDFSAK_ALWAYS_ON_TOP = '1'
if ($AutoRun) { $env:PDFSAK_DEV_RUN = '1' } else { Remove-Item Env:\PDFSAK_DEV_RUN -ErrorAction SilentlyContinue }
if ($Files) { $env:PDFSAK_DEV_FILES = $Files } else { Remove-Item Env:\PDFSAK_DEV_FILES -ErrorAction SilentlyContinue }
if ($NewTab) { $env:PDFSAK_DEV_NEW = $NewTab } else { Remove-Item Env:\PDFSAK_DEV_NEW -ErrorAction SilentlyContinue }

$exe = $Exe
$proc = Start-Process -FilePath $exe -WorkingDirectory "D:\AI\projects\pdf-swiss-army-knife\target\release" -PassThru

$handle = [IntPtr]::Zero
for ($i = 0; $i -lt $WaitSeconds * 4; $i++) {
    Start-Sleep -Milliseconds 250
    $proc.Refresh()
    if ($proc.HasExited) { Write-Error "app exited with code $($proc.ExitCode)"; exit 1 }
    if ($proc.MainWindowHandle -ne 0) { $handle = $proc.MainWindowHandle; break }
}
if ($handle -eq [IntPtr]::Zero) { Write-Error 'no window'; exit 2 }
Start-Sleep -Seconds 3
if ($Maximize) { [UiFlow]::ShowWindow($handle, 3) | Out-Null } else { [UiFlow]::ShowWindow($handle, 9) | Out-Null }
[UiFlow]::SetWindowPos($handle, [IntPtr](-1), 0, 0, 0, 0, 0x0003) | Out-Null   # TOPMOST
[UiFlow]::SetForegroundWindow($handle) | Out-Null
Start-Sleep -Milliseconds 1000

$rect = New-Object UiFlow+RECT
[UiFlow]::GetWindowRect($handle, [ref]$rect) | Out-Null
$script:scaleFactor = if ($Scale -gt 0) { $Scale } else { [Math]::Round([UiFlow]::GetDpiForWindow($handle) / 96.0, 3) }
Write-Host "ui scale factor: $script:scaleFactor"

foreach ($action in ($Actions -split ';')) {
    if (-not $action) { continue }
    $parts = $action -split ':'
    switch ($parts[0]) {
        'click' {
            $coords = $parts[1] -split ','
            $x = [int]($rect.Left + [int]$coords[0] * $script:scaleFactor)
            $y = [int]($rect.Top + [int]$coords[1] * $script:scaleFactor)
            [UiFlow]::SetCursorPos($x, $y) | Out-Null
            Start-Sleep -Milliseconds 150
            [UiFlow]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)  # LEFTDOWN
            Start-Sleep -Milliseconds 60
            [UiFlow]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)  # LEFTUP
            Write-Host "clicked $x,$y"
        }
        'type' {
            [System.Windows.Forms.SendKeys]::SendWait($parts[1])
            Write-Host "typed text"
        }
        'scroll' {
            $args2 = $parts[1] -split ','
            $delta = [int]$args2[0]
            $times = if ($args2.Count -gt 1) { [int]$args2[1] } else { 1 }
            for ($i = 0; $i -lt $times; $i++) {
                if ($delta -lt 0) { $wheel = [uint32](4294967296 + $delta) } else { $wheel = [uint32]$delta }
                [UiFlow]::mouse_event(0x0800, 0, 0, $wheel, [UIntPtr]::Zero)  # WHEEL
                Start-Sleep -Milliseconds 120
            }
            Write-Host "scrolled $delta x$times"
        }
        'wait' {
            Start-Sleep -Milliseconds ([int]$parts[1])
        }
        default { Write-Host "unknown action $action" }
    }
}

Start-Sleep -Milliseconds 800
[UiFlow]::ShowWindow($handle, 9) | Out-Null
[UiFlow]::SetWindowPos($handle, [IntPtr](-1), 0, 0, 0, 0, 0x0003) | Out-Null
[UiFlow]::SetForegroundWindow($handle) | Out-Null
Start-Sleep -Milliseconds 600
[UiFlow]::GetWindowRect($handle, [ref]$rect) | Out-Null
$w = $rect.Right - $rect.Left
$h = $rect.Bottom - $rect.Top
$bmp = New-Object System.Drawing.Bitmap($w, $h)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($rect.Left, $rect.Top, 0, 0, (New-Object System.Drawing.Size($w, $h)))
$g.Dispose()
New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Out) | Out-Null
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
$bmp.Dispose()
Write-Host "captured -> $Out"
