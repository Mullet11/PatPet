<#
  Memasang PatPet dengan identitas aplikasi, supaya notifikasi dari semua
  aplikasi bisa tampil di island.

  Yang dilakukan:
    1. Menyalin target\release\patpet.exe ke folder pemasangan.
    2. Membuat paket identitas (PatPet.msix) dari packaging\AppxManifest.xml.
    3. Membuat sertifikat "CN=PatPet Local" di sertifikat pribadi Anda
       (sekali saja) dan menandatangani paket dengannya.
    4. Mempercayai sertifikat itu di komputer ini (butuh izin admin, sekali saja).
    5. Mendaftarkan paket dan menjalankan PatPet.

  Pakai:  powershell -ExecutionPolicy Bypass -File packaging\install.ps1
  Hapus:  powershell -ExecutionPolicy Bypass -File packaging\uninstall.ps1
#>
param(
    [string]$InstallDir = (Join-Path $env:LOCALAPPDATA 'Programs\PatPet')
)
$ErrorActionPreference = 'Stop'

$subject = 'CN=PatPet Local'
$root = Split-Path $PSScriptRoot
$exe = Join-Path $root 'target\release\patpet.exe'
$out = Join-Path $PSScriptRoot 'out'
$stage = Join-Path $out 'package'
$msix = Join-Path $out 'PatPet.msix'

if (-not (Test-Path $exe)) { throw "patpet.exe belum ada. Jalankan dulu: cargo build --release" }

# Alat dari Windows SDK (terpasang bersama Visual Studio Build Tools).
$sdk = Get-ChildItem 'C:\Program Files (x86)\Windows Kits\10\bin\10.*\x64\makeappx.exe' -ErrorAction SilentlyContinue |
    Sort-Object FullName -Descending | Select-Object -First 1
if (-not $sdk) { throw 'makeappx.exe tidak ditemukan. Pasang Windows SDK lewat Visual Studio Installer.' }
$makeappx = $sdk.FullName
$signtool = Join-Path $sdk.DirectoryName 'signtool.exe'

Write-Host '1/5 Menyalin patpet.exe' -ForegroundColor Cyan
Get-Process patpet -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Milliseconds 500
New-Item -ItemType Directory -Force $InstallDir | Out-Null
Copy-Item $exe $InstallDir -Force

Write-Host '2/5 Membuat paket identitas' -ForegroundColor Cyan
Remove-Item $stage -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force (Join-Path $stage 'Assets') | Out-Null
Copy-Item (Join-Path $PSScriptRoot 'AppxManifest.xml') $stage
# Logo paket: frame pertama sprite kucing, diperbesar tanpa dihaluskan.
Add-Type -AssemblyName System.Drawing
$sprite = [System.Drawing.Image]::FromFile((Join-Path $root 'assets\pet\cat_idle.png'))
try {
    foreach ($logo in @(@{ Name = 'Square44x44Logo.png'; Size = 44 }, @{ Name = 'Square150x150Logo.png'; Size = 150 }, @{ Name = 'StoreLogo.png'; Size = 50 })) {
        $bmp = New-Object System.Drawing.Bitmap $logo.Size, $logo.Size
        $g = [System.Drawing.Graphics]::FromImage($bmp)
        $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::NearestNeighbor
        $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::Half
        $g.DrawImage($sprite, (New-Object System.Drawing.Rectangle 0, 0, $logo.Size, $logo.Size), (New-Object System.Drawing.Rectangle 0, 0, 32, 32), [System.Drawing.GraphicsUnit]::Pixel)
        $bmp.Save((Join-Path $stage "Assets\$($logo.Name)"), [System.Drawing.Imaging.ImageFormat]::Png)
        $g.Dispose(); $bmp.Dispose()
    }
} finally { $sprite.Dispose() }
& $makeappx pack /d $stage /p $msix /nv /o | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'makeappx gagal membuat paket.' }

Write-Host '3/5 Menandatangani paket' -ForegroundColor Cyan
$cert = Get-ChildItem Cert:\CurrentUser\My | Where-Object { $_.Subject -eq $subject -and $_.HasPrivateKey } | Select-Object -First 1
if (-not $cert) {
    $cert = New-SelfSignedCertificate -Type Custom -Subject $subject -FriendlyName 'PatPet (lokal)' `
        -KeyUsage DigitalSignature -CertStoreLocation Cert:\CurrentUser\My `
        -TextExtension @('2.5.29.37={text}1.3.6.1.5.5.7.3.3', '2.5.29.19={text}')
}
& $signtool sign /fd SHA256 /sha1 $cert.Thumbprint /s My $msix | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'signtool gagal menandatangani paket.' }

Write-Host '4/5 Mempercayai sertifikat di komputer ini' -ForegroundColor Cyan
$trusted = Get-ChildItem Cert:\LocalMachine\TrustedPeople | Where-Object { $_.Thumbprint -eq $cert.Thumbprint }
if (-not $trusted) {
    $cer = Join-Path $out 'PatPet.cer'
    Export-Certificate -Cert $cert -FilePath $cer | Out-Null
    Write-Host '    Windows akan meminta izin admin. Pilih "Yes".' -ForegroundColor Yellow
    $cmd = "Import-Certificate -FilePath '$cer' -CertStoreLocation Cert:\LocalMachine\TrustedPeople | Out-Null"
    Start-Process powershell -Verb RunAs -Wait -ArgumentList '-NoProfile', '-Command', $cmd
    $trusted = Get-ChildItem Cert:\LocalMachine\TrustedPeople | Where-Object { $_.Thumbprint -eq $cert.Thumbprint }
    if (-not $trusted) { throw 'Sertifikat belum dipercaya. Ulangi dan pilih "Yes" pada permintaan izin admin.' }
}

Write-Host '5/5 Mendaftarkan paket' -ForegroundColor Cyan
Get-AppxPackage -Name PatPet | Remove-AppxPackage
Add-AppxPackage -Path $msix -ExternalLocation $InstallDir
Start-Process (Join-Path $InstallDir 'patpet.exe')

Write-Host ''
Write-Host "Selesai. PatPet terpasang di $InstallDir dan sudah berjalan." -ForegroundColor Green
Write-Host 'Langkah terakhir (sekali saja):'
Write-Host '  1. Izinkan PatPet membaca notifikasi saat Windows bertanya.'
Write-Host '  2. Settings > System > Notifications > matikan "Show notification banners"'
Write-Host '     supaya notifikasi hanya muncul di island, tidak dobel di pojok kanan bawah.'
