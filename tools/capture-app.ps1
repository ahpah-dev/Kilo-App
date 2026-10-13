param(
  [string]$Theme = 'light',
  [string]$Ui = 'polished',
  [string]$Out = 'shot.png',
  [int]$Port = 9333
)
$ErrorActionPreference = 'Stop'
$py = "C:\Users\David\.dsh\dsh-runtimes\dsh-primary-runtime\dependencies\python\python.exe"
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System; using System.Runtime.InteropServices;
public class Cap7 {
  [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr h, ref POINT p);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int cx, int cy, uint f);
  [DllImport("user32.dll")] public static extern IntPtr SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
  [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
}
"@

Get-Process kilo-app -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 3
for ($i = 0; $i -lt 10 -and (Get-Process kilo-app -ErrorAction SilentlyContinue); $i++) { Start-Sleep -Seconds 1 }

$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=$Port --remote-allow-origins=*"
$exe = "$env:LOCALAPPDATA\Kilo App\kilo-app.exe"

# Launching right after a previous instance can fail while WebView2 releases the
# user-data directory, so retry a few times.
$p = $null
for ($attempt = 1; $attempt -le 4; $attempt++) {
  Get-Process kilo-app -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
  Start-Sleep -Seconds 4
  $p = Start-Process -FilePath $exe -PassThru
  Start-Sleep -Seconds 15
  $p.Refresh()
  if (-not $p.HasExited) { break }
  Write-Host "  launch attempt $attempt exited early (code $($p.ExitCode)); retrying"
  Start-Sleep -Seconds 4
}
if ($null -eq $p -or $p.HasExited) {
  Remove-Item Env:\WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS -ErrorAction SilentlyContinue
  throw "app would not stay running after 4 attempts"
}

Write-Host "applying theme='$Theme' ui='$Ui' via CDP..."
& $py tools\cdp.py $Port $Theme $Ui
Start-Sleep -Seconds 2

$h = $p.MainWindowHandle
$HWND_TOPMOST = New-Object IntPtr(-1)
$HWND_NOTOPMOST = New-Object IntPtr(-2)
$SWP_SHOWWINDOW = 0x40

# Minimise every other top-level window so nothing overlaps the capture.
Add-Type @"
using System; using System.Collections.Generic; using System.Runtime.InteropServices; using System.Text;
public class Wins {
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr l);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
  [DllImport("user32.dll")] public static extern int GetWindowTextLength(IntPtr h);
  public static List<IntPtr> Visible() {
    var list = new List<IntPtr>();
    EnumWindows((h, l) => { if (IsWindowVisible(h) && !IsIconic(h)) list.Add(h); return true; }, IntPtr.Zero);
    return list;
  }
}
"@
$keep = $h
$others = [Wins]::Visible() | Where-Object { $_ -ne $keep }
foreach ($w in $others) { [Wins]::ShowWindow($w, 6) | Out-Null }   # SW_MINIMIZE
Start-Sleep -Seconds 2

[Cap7]::ShowWindow($h, 9) | Out-Null
[Cap7]::SetWindowPos($h, $HWND_TOPMOST, 60, 60, 1244, 844, $SWP_SHOWWINDOW) | Out-Null
[Cap7]::SetForegroundWindow($h) | Out-Null
Start-Sleep -Seconds 3

$cr = New-Object Cap7+RECT; [Cap7]::GetClientRect($h, [ref]$cr) | Out-Null
$o = New-Object Cap7+POINT; [Cap7]::ClientToScreen($h, [ref]$o) | Out-Null
$w = $cr.Right; $ht = $cr.Bottom
$bmp = New-Object System.Drawing.Bitmap($w, $ht)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($o.X, $o.Y, 0, 0, $bmp.Size)
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose(); $bmp.Dispose()
[Cap7]::SetWindowPos($h, $HWND_NOTOPMOST, 0,0,0,0, 0x1 -bor 0x2) | Out-Null
Write-Host "captured $Theme/$Ui -> $Out (${w}x${ht})"

$p.CloseMainWindow() | Out-Null
Start-Sleep -Seconds 3
if (-not $p.HasExited) { $p | Stop-Process -Force }
Remove-Item Env:\WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS -ErrorAction SilentlyContinue
