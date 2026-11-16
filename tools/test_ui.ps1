# PatPet UI self-test. Run from the project folder after building:
#   powershell -ExecutionPolicy Bypass -File tools\test_ui.ps1
# 1. closing animation: asks the running copy to quit (patpet.exe --quit) and samples the notch
# 2. Caps Lock pill: renders PATPET_DEMO=caps/capsoff to PNG files in target\test
# 3. opening animation: starts PatPet and samples the notch while it pops out
# Leaves PatPet running at the end.
$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
$exe = Join-Path $root "target\release\patpet.exe"
$out = Join-Path $root "target\test"
New-Item -ItemType Directory -Force $out | Out-Null
$watch = Join-Path $PSScriptRoot "notch_watch.ps1"

function Watch($secs, $name) {
    $f = Join-Path $out "$name.txt"
    Start-Process powershell -ArgumentList "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", "`"$watch`"", $secs -NoNewWindow -PassThru -RedirectStandardOutput $f
}

# ---- 1. closing
if (Get-Process patpet -ErrorAction SilentlyContinue) {
    $p = Watch 2.5 "quit"; Start-Sleep -Milliseconds 700
    & $exe --quit
    $p.WaitForExit()
    Start-Sleep -Milliseconds 300
    $still = [bool](Get-Process patpet -ErrorAction SilentlyContinue)
    "== closing animation (process still running afterwards: $still)"
    Get-Content (Join-Path $out "quit.txt")
} else {
    "== closing animation: skipped, PatPet was not running"
}
Get-Process patpet -ErrorAction SilentlyContinue | Stop-Process -Force

# ---- 2. Caps Lock pill snapshots
Add-Type -AssemblyName System.Drawing
foreach ($d in "caps", "capsoff") {
    $bmpf = Join-Path $out "$d.bmp"
    Remove-Item $bmpf -ErrorAction SilentlyContinue
    $env:PATPET_DEMO = $d; $env:PATPET_DUMP = $bmpf
    Start-Process $exe -Wait
    Remove-Item Env:PATPET_DEMO, Env:PATPET_DUMP
    if (Test-Path $bmpf) {
        $img = [System.Drawing.Image]::FromFile($bmpf)
        "== $d pill: $($img.Width)x$($img.Height)px -> $out\$d.png"
        $img.Save((Join-Path $out "$d.png"), [System.Drawing.Imaging.ImageFormat]::Png); $img.Dispose()
    } else { "== $d pill: NO SNAPSHOT" }
}

# ---- 3. opening
$p = Watch 2 "open"; Start-Sleep -Milliseconds 700
Start-Process $exe
$p.WaitForExit()
"== opening animation"
Get-Content (Join-Path $out "open.txt")
