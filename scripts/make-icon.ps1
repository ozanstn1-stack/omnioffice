# Generates the application icon (PNG) used by `tauri icon`.
# Design: rounded squircle with an indigo -> violet gradient, a white "O"
# ring and a small sparkle. No text; stays readable down to 16 px.
Add-Type -AssemblyName System.Drawing

$size = 1024
$bmp = New-Object System.Drawing.Bitmap($size, $size)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
$g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
$g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
$g.Clear([System.Drawing.Color]::Transparent)

# ---------------------------------------------------------------- squircle tile
$inset = 44
$rect = New-Object System.Drawing.Rectangle($inset, $inset, ($size - 2 * $inset), ($size - 2 * $inset))
$radius = 228
$tile = New-Object System.Drawing.Drawing2D.GraphicsPath
$tile.AddArc($rect.X, $rect.Y, $radius, $radius, 180, 90)
$tile.AddArc($rect.Right - $radius, $rect.Y, $radius, $radius, 270, 90)
$tile.AddArc($rect.Right - $radius, $rect.Bottom - $radius, $radius, $radius, 0, 90)
$tile.AddArc($rect.X, $rect.Bottom - $radius, $radius, $radius, 90, 90)
$tile.CloseFigure()

$tileGradient = New-Object System.Drawing.Drawing2D.LinearGradientBrush(
    $rect,
    [System.Drawing.Color]::FromArgb(255, 79, 70, 229),
    [System.Drawing.Color]::FromArgb(255, 124, 58, 237),
    52.0)
$blend = New-Object System.Drawing.Drawing2D.ColorBlend
$blend.Colors = @(
    [System.Drawing.Color]::FromArgb(255, 79, 70, 229),
    [System.Drawing.Color]::FromArgb(255, 99, 102, 241),
    [System.Drawing.Color]::FromArgb(255, 124, 58, 237),
    [System.Drawing.Color]::FromArgb(255, 147, 51, 234)
)
$blend.Positions = @(0.0, 0.42, 0.78, 1.0)
$tileGradient.InterpolationColors = $blend
$g.FillPath($tileGradient, $tile)

# top-left glass highlight: radial white glow fading out
$glowPath = New-Object System.Drawing.Drawing2D.GraphicsPath
$glowPath.AddEllipse(120, 60, 820, 820)
$glow = New-Object System.Drawing.Drawing2D.PathGradientBrush($glowPath)
$glow.CenterColor = [System.Drawing.Color]::FromArgb(46, 255, 255, 255)
$glow.SurroundColors = @([System.Drawing.Color]::FromArgb(0, 255, 255, 255))
$glow.CenterPoint = New-Object System.Drawing.PointF(400, 330)
$g.FillPath($glow, $glowPath)

# inner edge light for a glassy border
$edgePen = New-Object System.Drawing.Pen([System.Drawing.Color]::FromArgb(56, 255, 255, 255), 6)
$edgePen.Alignment = [System.Drawing.Drawing2D.PenAlignment]::Inset
$g.DrawPath($edgePen, $tile)

# ---------------------------------------------------------------- white "O" ring
$cx = 512.0
$cy = 506.0
$outer = 268.0
$inner = 170.0

function New-RingPath([double]$cx, [double]$cy, [double]$outer, [double]$inner) {
    $path = New-Object System.Drawing.Drawing2D.GraphicsPath
    $path.FillMode = [System.Drawing.Drawing2D.FillMode]::Alternate
    $path.AddEllipse([float]($cx - $outer), [float]($cy - $outer), [float](2 * $outer), [float](2 * $outer))
    $path.AddEllipse([float]($cx - $inner), [float]($cy - $inner), [float](2 * $inner), [float](2 * $inner))
    return $path
}

# soft drop shadow under the ring
$shadowPath = New-RingPath $cx ($cy + 16) ($outer + 4) ($inner - 4)
$shadow = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(38, 30, 27, 75))
$g.FillPath($shadow, $shadowPath)

# ring body: subtle top-to-bottom white -> periwinkle
$ringPath = New-RingPath $cx $cy $outer $inner
$ringRect = New-Object System.Drawing.RectangleF([float]($cx - $outer), [float]($cy - $outer), [float](2 * $outer), [float](2 * $outer))
$ringGradient = New-Object System.Drawing.Drawing2D.LinearGradientBrush(
    $ringRect,
    [System.Drawing.Color]::FromArgb(255, 255, 255, 255),
    [System.Drawing.Color]::FromArgb(255, 224, 231, 255),
    90.0)
$g.FillPath($ringGradient, $ringPath)

# inner shading to give the ring volume
$innerShadowPath = New-RingPath $cx $cy ($inner + 26) $inner
$innerShadow = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(22, 79, 70, 229))
$g.FillPath($innerShadow, $innerShadowPath)

# ---------------------------------------------------------------- sparkle
function New-SparklePath([double]$x, [double]$y, [double]$long, [double]$short) {
    $path = New-Object System.Drawing.Drawing2D.GraphicsPath
    $path.FillMode = [System.Drawing.Drawing2D.FillMode]::Winding
    $path.AddPolygon(@(
        (New-Object System.Drawing.PointF([float]($x), [float]($y - $long))),
        (New-Object System.Drawing.PointF([float]($x + $short), [float]$y)),
        (New-Object System.Drawing.PointF([float]$x, [float]($y + $long))),
        (New-Object System.Drawing.PointF([float]($x - $short), [float]$y))
    ))
    $path.AddPolygon(@(
        (New-Object System.Drawing.PointF([float]($x - $long), [float]$y)),
        (New-Object System.Drawing.PointF([float]$x, [float]($y - $short))),
        (New-Object System.Drawing.PointF([float]($x + $long), [float]$y)),
        (New-Object System.Drawing.PointF([float]$x, [float]($y + $short)))
    ))
    return $path
}

$sparkBrush = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 255, 255, 255))
$g.FillPath($sparkBrush, (New-SparklePath 782 248 60 18))
$dotBrush = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(190, 255, 255, 255))
$g.FillEllipse($dotBrush, 856, 158, 28, 28)

$g.Dispose()
$out = Join-Path $PSScriptRoot '..\assets\icon-source.png'
$bmp.Save($out, [System.Drawing.Imaging.ImageFormat]::Png)
$bmp.Dispose()
Write-Host "icon written: $out"
