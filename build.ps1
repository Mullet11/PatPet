# Builds PatPet (release). The icon and manifest are embedded by build.rs.
# Usage:  powershell -ExecutionPolicy Bypass -File build.ps1
Set-Location $PSScriptRoot
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"

# cargo reports progress on stderr, which Windows PowerShell would otherwise treat as an error
cmd /c "cargo build --release 2>&1"
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
Write-Host "Built $(Join-Path $PSScriptRoot 'target\release\patpet.exe')."
