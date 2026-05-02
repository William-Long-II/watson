# scripts/capture-window.ps1 — capture the real Watson window pixels.
#
# Unlike a browser screenshot of the SPA, this grabs the actual
# desktop region where Watson's Tauri window is rendering. That
# includes OS-level effects we can't see in a browser: window
# transparency, color bleed at rounded corners, OS shadow, anything
# the user sees with their eyes.
#
# Usage:
#   pwsh scripts/capture-window.ps1 -OutPath out.png            # window only
#   pwsh scripts/capture-window.ps1 -OutPath out.png -Margin 60  # window + 60px desktop bleed
#   pwsh scripts/capture-window.ps1 -OutPath out.png -Title "Watson" -Margin 80
#
# Requires Watson to be visible (not minimized / hidden). Press
# Alt+Space first to summon it, then run this script.

[CmdletBinding()]
param(
    [string]$OutPath     = "watson-capture.png",
    [string]$ProcessName = "watson",
    [int]$Margin         = 50
)

Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms

# Win32 plumbing: read the on-screen bounding box of an HWND. We
# get the HWND from .NET's Get-Process MainWindowHandle (more
# reliable than FindWindow, which has been flaky against Tauri's
# title bytes — possibly a unicode/composition issue).
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class Win32 {
  [DllImport("user32.dll")]
  public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);

  [DllImport("user32.dll")]
  public static extern bool IsWindowVisible(IntPtr hWnd);

  [StructLayout(LayoutKind.Sequential)]
  public struct RECT { public int Left, Top, Right, Bottom; }
}
"@

$proc = Get-Process -Name $ProcessName -ErrorAction SilentlyContinue |
    Where-Object { $_.MainWindowHandle -ne 0 } |
    Select-Object -First 1
if (-not $proc) {
    Write-Error "No '$ProcessName' process with a main window. Press Alt+Space to summon Watson, then re-run."
    exit 1
}
$hwnd = $proc.MainWindowHandle
if (-not [Win32]::IsWindowVisible($hwnd)) {
    Write-Error "Watson window exists but is not visible. Press Alt+Space to summon, then re-run."
    exit 1
}

$rect = New-Object Win32+RECT
[void][Win32]::GetWindowRect($hwnd, [ref]$rect)

# Add margin so the screenshot includes the desktop bleed around
# the rounded window corners. Clamp to screen bounds so we don't
# blow up on multi-monitor setups.
$x = [Math]::Max(0, $rect.Left - $Margin)
$y = [Math]::Max(0, $rect.Top  - $Margin)
$w = ($rect.Right  - $rect.Left) + (2 * $Margin)
$h = ($rect.Bottom - $rect.Top)  + (2 * $Margin)

$bmp = New-Object System.Drawing.Bitmap $w, $h
$g   = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($x, $y, 0, 0, $bmp.Size)
$bmp.Save($OutPath, [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose()
$bmp.Dispose()

$abs = (Resolve-Path $OutPath).Path
Write-Host "saved -> $abs ($w x $h)"
