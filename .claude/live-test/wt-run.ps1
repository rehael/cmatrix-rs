<#
Live check of cmatrix in a new Windows Terminal window, without touching other windows.

Launches `cmd /k <Command>` in a new window, optionally resizes it, captures the window
every -Every seconds for -Duration seconds (PrintWindow, so other windows on top do not
matter), then stops cmatrix through its console: -Stop 'q' (any character) writes a key
press with conkey.exe, -Stop break raises Ctrl+Break with sendbreak.exe. Captures once
more (after.png) and closes the window with WM_CLOSE.

Never use SendKeys for this: focus can return to the user's own terminal and the keys
land there.

Run with Windows PowerShell 5.1 (System.Drawing):
  powershell -NoProfile -ExecutionPolicy Bypass -File .claude\live-test\wt-run.ps1 `
    -Command '<target dir>\release\cmatrix.exe -F <repo>\.claude\live-test\test.txt' `
    -OutDir $env:TEMP\cm-live -Width 900 -Height 500 -Duration 20
Use absolute paths in -Command: the new window starts in the profile's directory.
Helpers are compiled with rustc into -OutDir on first use.
#>
param(
    [Parameter(Mandatory)][string]$Command,
    [Parameter(Mandatory)][string]$OutDir,
    [int]$Width = 0,
    [int]$Height = 0,
    [double]$Every = 0.5,
    [double]$Duration = 10,
    [string]$Stop = 'q'
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
Add-Type -Name U -Namespace N -MemberDefinition @'
[StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
[DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
[DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
[DllImport("user32.dll")] public static extern bool IsWindow(IntPtr h);
[DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint flags);
[DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
[DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr a, int x, int y, int cx, int cy, uint f);
[DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint msg, IntPtr w, IntPtr l);
[DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr FindWindowEx(IntPtr p, IntPtr a, string c, string t);
public static System.Collections.Generic.List<long> Terminals() {
    var list = new System.Collections.Generic.List<long>();
    IntPtr h = IntPtr.Zero;
    while ((h = FindWindowEx(IntPtr.Zero, h, "CASCADIA_HOSTING_WINDOW_CLASS", null)) != IntPtr.Zero) list.Add(h.ToInt64());
    return list;
}
'@
[void][N.U]::SetProcessDPIAware()
New-Item -ItemType Directory -Force $OutDir | Out-Null

function Helper([string]$name) {
    $exe = Join-Path $OutDir "$name.exe"
    if (-not (Test-Path $exe)) {
        & rustc --edition 2024 -O (Join-Path $PSScriptRoot "$name.rs") -o $exe | Out-Host
        if ($LASTEXITCODE) { throw "rustc $name.rs failed" }
    }
    $exe
}

function Shot([IntPtr]$h, [string]$path) {
    $r = New-Object N.U+RECT
    [void][N.U]::GetWindowRect($h, [ref]$r)
    $bmp = New-Object System.Drawing.Bitmap ($r.R - $r.L), ($r.B - $r.T)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $hdc = $g.GetHdc()
    [void][N.U]::PrintWindow($h, $hdc, 2)   # PW_RENDERFULLCONTENT
    $g.ReleaseHdc($hdc)
    $bmp.Save($path)
    $g.Dispose(); $bmp.Dispose()
}

$stopper = if ($Stop -eq 'break') { Helper 'sendbreak' } else { Helper 'conkey' }
$before = [N.U]::Terminals()
$launched = Get-Date
Start-Process wt.exe -ArgumentList "-w new cmd /k $Command"
$h = [IntPtr]::Zero
for ($i = 0; $i -lt 50 -and $h -eq [IntPtr]::Zero; $i++) {
    Start-Sleep -Milliseconds 200
    foreach ($t in [N.U]::Terminals()) { if (-not $before.Contains($t)) { $h = [IntPtr]$t } }
}
if ($h -eq [IntPtr]::Zero) { throw 'new Windows Terminal window not found' }
"window $h"

# Windows Terminal applies its launch size (often maximised) after the window appears.
Start-Sleep -Milliseconds 1500
if ($Width -gt 0 -and $Height -gt 0) {
    [void][N.U]::ShowWindow($h, 9)   # SW_RESTORE
    [void][N.U]::SetWindowPos($h, [IntPtr]::Zero, 40, 40, $Width, $Height, 0x0014)
}
$p = Get-Process cmatrix -ErrorAction SilentlyContinue |
    Where-Object { $_.StartTime -gt $launched } | Sort-Object StartTime -Descending | Select-Object -First 1
if ($p) { "cmatrix pid $($p.Id)" } else { 'cmatrix not running (exited early or failed to start)' }

$n = 0
while (((Get-Date) - $launched).TotalSeconds -lt $Duration) {
    $at = ((Get-Date) - $launched).TotalSeconds
    Shot $h (Join-Path $OutDir ('{0:000}_{1:00.0}s.png' -f $n, $at))
    $n++
    Start-Sleep -Milliseconds ([int]($Every * 1000))
}
"captured $n frames in $OutDir"

if ($p) {
    if ($Stop -eq 'break') { & $stopper $p.Id } else { & $stopper $p.Id $Stop }
    "stop '$Stop' sent, helper exit $LASTEXITCODE"
    Start-Sleep -Seconds 2
    "cmatrix still running: $([bool](Get-Process -Id $p.Id -ErrorAction SilentlyContinue))"
}
Shot $h (Join-Path $OutDir 'after.png')
[void][N.U]::PostMessage($h, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)   # WM_CLOSE
Start-Sleep -Seconds 2
"window closed: $(-not [N.U]::IsWindow($h))"
