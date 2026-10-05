param(
    [Parameter(Mandatory=$true)][string]$Screen,
    [string]$Files = "",
    [Parameter(Mandatory=$true)][string]$Out,
    [int]$WaitSeconds = 8
)
# Launches the app with the development launch hook (start screen + files),
# waits for the UI, and captures the window using PrintWindow (works even when
# the window is not on top). Used for screenshots and validation walkthroughs.
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class WinCap {
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hWnd, IntPtr hdcBlt, uint nFlags);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hWnd, IntPtr hWndInsertAfter, int X, int Y, int cx, int cy, uint uFlags);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
}
"@

Get-Process -Name 'OmniOffice', 'pdf-swiss-army-knife' -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Milliseconds 600

$env:PDFSAK_START_SCREEN = $Screen
if ($Files) { $env:PDFSAK_DEV_FILES = $Files } else { Remove-Item Env:\PDFSAK_DEV_FILES -ErrorAction SilentlyContinue }
$env:PDFSAK_ALWAYS_ON_TOP = '1'

$exe = "D:\AI\projects\pdf-swiss-army-knife\target\release\OmniOffice.exe"
$proc = Start-Process -FilePath $exe -WorkingDirectory "D:\AI\projects\pdf-swiss-army-knife\target\release" -PassThru

# Wait for the window to appear
$handle = [IntPtr]::Zero
for ($i = 0; $i -lt $WaitSeconds * 4; $i++) {
    Start-Sleep -Milliseconds 250
    $proc.Refresh()
    if ($proc.HasExited) { Write-Error "app exited with code $($proc.ExitCode)"; exit 1 }
    if ($proc.MainWindowHandle -ne 0) { $handle = $proc.MainWindowHandle; break }
}
if ($handle -eq [IntPtr]::Zero) { Write-Error 'window did not appear'; exit 2 }
Start-Sleep -Seconds 3   # let the UI settle (fonts, thumbnails)

[WinCap]::ShowWindow($handle, 9) | Out-Null
[WinCap]::SetWindowPos($handle, [IntPtr](-1), 0, 0, 0, 0, 0x0003) | Out-Null   # TOPMOST, NOMOVE|NOSIZE
[WinCap]::SetForegroundWindow($handle) | Out-Null
Start-Sleep -Milliseconds 1200

$rect = New-Object WinCap+RECT
[WinCap]::GetWindowRect($handle, [ref]$rect) | Out-Null
$width = $rect.Right - $rect.Left
$height = $rect.Bottom - $rect.Top
if ($width -le 0 -or $height -le 0) { Write-Error 'invalid window rect'; exit 3 }

$bmp = New-Object System.Drawing.Bitmap($width, $height)
$graphics = [System.Drawing.Graphics]::FromImage($bmp)
$graphics.CopyFromScreen($rect.Left, $rect.Top, 0, 0, (New-Object System.Drawing.Size($width, $height)))
$graphics.Dispose()
[WinCap]::SetWindowPos($handle, [IntPtr](-2), 0, 0, 0, 0, 0x0003) | Out-Null   # NOTOPMOST
New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Out) | Out-Null
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
$bmp.Dispose()
Write-Host "captured $Screen -> $Out"
