<#
.SYNOPSIS
  Provision a USB stick as a RemBoot drive.

.DESCRIPTION
  Creates a GPT USB with two partitions and copies the app onto it:
    - Partition 1: FAT32 EFI System Partition  ->  \EFI\BOOT\BOOTX64.EFI (RemBoot)
    - Partition 2: exFAT data partition        ->  your *.iso files (+ remboot.conf)
  RemBoot boots from the FAT partition and reads the ISOs off the exFAT one
  itself (its own read-only exFAT reader + on-demand virtual CD), so no Ventoy
  or any other tool is needed on the stick.

  *** THIS ERASES THE ENTIRE TARGET DISK. *** Run from an elevated PowerShell.

.PARAMETER DiskNumber
  The disk number of the USB (see `Get-Disk`). Double-check it!

.PARAMETER IsoSource
  Optional folder whose *.iso files are copied to the data partition. You can
  also skip this and drag-drop ISOs in Explorer afterwards.

.PARAMETER EfiPath
  Path to BOOTX64.EFI. Defaults to dist\EFI\BOOT\BOOTX64.EFI (produced by
  tools/build.sh). Build first if it is missing.

.PARAMETER EspSizeMB
  Size of the FAT32 boot partition. 512 MB is plenty.

.EXAMPLE
  .\tools\make-usb.ps1 -DiskNumber 2 -IsoSource "C:\Users\me\Desktop\iso"
#>
#Requires -RunAsAdministrator
[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)][int]$DiskNumber,
  [string]$IsoSource,
  [string]$EfiPath = "$PSScriptRoot\..\dist\EFI\BOOT\BOOTX64.EFI",
  [string]$ConfigPath = "$PSScriptRoot\..\remboot.conf.example",
  [int]$EspSizeMB = 512,
  [switch]$Force
)

$ErrorActionPreference = 'Stop'
$ESP_GUID = '{c12a7328-f81f-11d2-ba4b-00a0c93ec93b}'

if (-not (Test-Path $EfiPath)) {
  throw "BOOTX64.EFI not found at '$EfiPath'. Build it first: wsl -d Ubuntu -u root -- bash /mnt/c/GitHub/RemBoot/tools/build.sh"
}

$disk = Get-Disk -Number $DiskNumber -ErrorAction Stop
if ($disk.IsSystem -or $disk.IsBoot) {
  throw "Disk $DiskNumber is a system/boot disk. Refusing."
}
if ($disk.BusType -ne 'USB' -and -not $Force) {
  throw "Disk $DiskNumber is not a USB device (BusType=$($disk.BusType)). Re-run with -Force only if you are absolutely sure."
}

$sizeGB = [math]::Round($disk.Size / 1GB, 1)
Write-Host ""
Write-Host "About to ERASE and repartition:" -ForegroundColor Yellow
Write-Host ("  Disk {0}: {1}  ({2} GB, BusType={3})" -f $DiskNumber, $disk.FriendlyName, $sizeGB, $disk.BusType)
Write-Host ("  -> Partition 1: FAT32 {0} MB  (RemBoot app)" -f $EspSizeMB)
Write-Host "  -> Partition 2: exFAT (rest)   (your ISOs)"
Write-Host ""
if (-not $Force) {
  $answer = Read-Host "Type  ERASE $DiskNumber  to continue"
  if ($answer -ne "ERASE $DiskNumber") { Write-Host "Aborted."; return }
}

Write-Host "Clearing disk..." -ForegroundColor Cyan
Clear-Disk -Number $DiskNumber -RemoveData -RemoveOEM -Confirm:$false -ErrorAction SilentlyContinue
Initialize-Disk -Number $DiskNumber -PartitionStyle GPT -ErrorAction SilentlyContinue | Out-Null

Write-Host "Creating FAT32 boot partition..." -ForegroundColor Cyan
$esp = New-Partition -DiskNumber $DiskNumber -Size ($EspSizeMB * 1MB) -GptType $ESP_GUID
Format-Volume -Partition $esp -FileSystem FAT32 -NewFileSystemLabel 'REMBOOT' -Confirm:$false | Out-Null
$esp | Add-PartitionAccessPath -AssignDriveLetter
$espLetter = (Get-Partition -DiskNumber $DiskNumber -PartitionNumber $esp.PartitionNumber).DriveLetter

Write-Host "Creating exFAT data partition..." -ForegroundColor Cyan
$data = New-Partition -DiskNumber $DiskNumber -UseMaximumSize -AssignDriveLetter
Format-Volume -Partition $data -FileSystem exFAT -NewFileSystemLabel 'REMBOOT_DATA' -Confirm:$false | Out-Null
$dataLetter = (Get-Partition -DiskNumber $DiskNumber -PartitionNumber $data.PartitionNumber).DriveLetter

Write-Host "Copying RemBoot -> ${espLetter}:\EFI\BOOT\BOOTX64.EFI" -ForegroundColor Cyan
New-Item -ItemType Directory -Force -Path "${espLetter}:\EFI\BOOT" | Out-Null
Copy-Item -Path $EfiPath -Destination "${espLetter}:\EFI\BOOT\BOOTX64.EFI" -Force

if ($IsoSource) {
  $isos = Get-ChildItem -Path $IsoSource -Filter *.iso -File -ErrorAction Stop
  Write-Host ("Copying {0} ISO(s) to {1}: (this can take a while)..." -f $isos.Count, $dataLetter) -ForegroundColor Cyan
  foreach ($iso in $isos) {
    Write-Host "  $($iso.Name)"
    Copy-Item -Path $iso.FullName -Destination "${dataLetter}:\$($iso.Name)" -Force
  }
}

if (Test-Path $ConfigPath) {
  Copy-Item -Path $ConfigPath -Destination "${dataLetter}:\remboot.conf" -Force
  Write-Host "Wrote a starter remboot.conf to ${dataLetter}:\ (edit it, or press E in the menu)."
}

Write-Host ""
Write-Host "Done. RemBoot USB is ready." -ForegroundColor Green
Write-Host ("  Boot partition : {0}:  (FAT32, the app)" -f $espLetter)
Write-Host ("  Data partition : {0}:  (exFAT, drop more .iso files here anytime)" -f $dataLetter)
Write-Host "Boot the target PC from this USB (UEFI, Secure Boot OFF) via its boot menu."
