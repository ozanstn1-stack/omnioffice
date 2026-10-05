param(
    [Parameter(Mandatory=$true)][string]$Name,
    [string]$Out = "",
    [switch]$NoCapture
)
# Clicks a button in the running app by its accessible name (UI Automation),
# then optionally captures the window for visual verification.
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class Win32b {
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
}
"@

$proc = Get-Process -Name 'OmniOffice', 'pdf-swiss-army-knife' -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
if (-not $proc) { Write-Error 'app not running'; exit 1 }
$handle = $proc.MainWindowHandle
[Win32b]::ShowWindow($handle, 9) | Out-Null
[Win32b]::SetForegroundWindow($handle) | Out-Null
Start-Sleep -Milliseconds 300

$root = [System.Windows.Automation.AutomationElement]::FromHandle($handle)
$condition = New-Object System.Windows.Automation.PropertyCondition(
    [System.Windows.Automation.AutomationElement]::NameProperty, $Name)
$element = $root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
if (-not $element) {
    Write-Error "element '$Name' not found"
    exit 2
}
$pattern = $element.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)
$pattern.Invoke()
Write-Host "clicked '$Name'"
Start-Sleep -Milliseconds 900

if (-not $NoCapture) {
    if (-not $Out) { $Out = "D:\AI\projects\pdf-swiss-army-knife\docs\screenshots\capture.png" }
    $rect = New-Object Win32b+RECT
    [Win32b]::GetWindowRect($handle, [ref]$rect) | Out-Null
    $width = $rect.Right - $rect.Left
    $height = $rect.Bottom - $rect.Top
    $bmp = New-Object System.Drawing.Bitmap($width, $height)
    $graphics = [System.Drawing.Graphics]::FromImage($bmp)
    $graphics.CopyFromScreen($rect.Left, $rect.Top, 0, 0, (New-Object System.Drawing.Size($width, $height)))
    $graphics.Dispose()
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Out) | Out-Null
    $bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
    Write-Host "captured -> $Out"
}
