param(
    [Parameter(Mandatory=$true)][string]$Out,
    [int]$WaitMs = 1500
)
# Captures the running app window by attaching to its input queue so the
# SetForegroundWindow restriction does not block us.
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class WinFocus {
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
    [DllImport("user32.dll")] public static extern bool BringWindowToTop(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr hWnd, IntPtr after, int x, int y, int cx, int cy, uint flags);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, IntPtr pid);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
    [DllImport("user32.dll")] public static extern bool AttachThreadInput(uint idAttach, uint idAttachTo, bool fAttach);
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
}
"@

$proc = Get-Process -Name 'OmniOffice', 'pdf-swiss-army-knife' -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
if (-not $proc) { Write-Error 'app not running'; exit 1 }
$h = $proc.MainWindowHandle

[WinFocus]::ShowWindow($h, 9) | Out-Null
$targetThread = [WinFocus]::GetWindowThreadProcessId($h, [IntPtr]::Zero)
$currentThread = [WinFocus]::GetCurrentThreadId()
[WinFocus]::AttachThreadInput($currentThread, $targetThread, $true) | Out-Null
[WinFocus]::BringWindowToTop($h) | Out-Null
[WinFocus]::SetWindowPos($h, [IntPtr]::Zero, 0, 0, 0, 0, 0x0003) | Out-Null
[WinFocus]::SetForegroundWindow($h) | Out-Null
[WinFocus]::AttachThreadInput($currentThread, $targetThread, $false) | Out-Null
Start-Sleep -Milliseconds $WaitMs
$fg = [WinFocus]::GetForegroundWindow()
Write-Host "foreground is app: $($fg -eq $h)"

$rect = New-Object WinFocus+RECT
[WinFocus]::GetWindowRect($h, [ref]$rect) | Out-Null
$w = $rect.Right - $rect.Left
$ht = $rect.Bottom - $rect.Top
# keep the capture inside the virtual screen
Add-Type -AssemblyName System.Windows.Forms
$vs = [System.Windows.Forms.SystemInformation]::VirtualScreen
$x0 = [Math]::Max($rect.Left, $vs.X)
$y0 = [Math]::Max($rect.Top, $vs.Y)
$x1 = [Math]::Min($rect.Right, $vs.X + $vs.Width)
$y1 = [Math]::Min($rect.Bottom, $vs.Y + $vs.Height)
$w = $x1 - $x0
$ht = $y1 - $y0
if ($w -le 0 -or $ht -le 0) { Write-Error 'window is off screen'; exit 2 }

$bmp = New-Object System.Drawing.Bitmap($w, $ht)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($x0, $y0, 0, 0, (New-Object System.Drawing.Size($w, $ht)))
$g.Dispose()
New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Out) | Out-Null
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
$bmp.Dispose()
Write-Host "captured ${w}x${ht} -> $Out"
