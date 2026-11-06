<#
  Menghapus paket identitas PatPet dan (opsional) sertifikatnya.
  File di folder pemasangan dan data di %APPDATA%\PatPet tidak dihapus.

  Pakai:  powershell -ExecutionPolicy Bypass -File packaging\uninstall.ps1 [-RemoveCertificate]
#>
param([switch]$RemoveCertificate)
$ErrorActionPreference = 'Stop'
$subject = 'CN=PatPet Local'

Get-Process patpet -ErrorAction SilentlyContinue | Stop-Process -Force
Get-AppxPackage -Name PatPet | Remove-AppxPackage
Write-Host 'Paket identitas PatPet dihapus.'

if ($RemoveCertificate) {
    Get-ChildItem Cert:\CurrentUser\My | Where-Object { $_.Subject -eq $subject } | Remove-Item
    Write-Host '    Windows akan meminta izin admin untuk menghapus sertifikat dari daftar tepercaya.' -ForegroundColor Yellow
    $cmd = "Get-ChildItem Cert:\LocalMachine\TrustedPeople | Where-Object { `$_.Subject -eq '$subject' } | Remove-Item"
    Start-Process powershell -Verb RunAs -Wait -ArgumentList '-NoProfile', '-Command', $cmd
    Write-Host 'Sertifikat PatPet dihapus.'
}
