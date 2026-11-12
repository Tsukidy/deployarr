<#
.SYNOPSIS
Creates the fixed UEFI and BIOS BCD stores that deployarr serves over TFTP.

.DESCRIPTION
Runs only on Windows, because bcdedit does. Writes uefi.bcd and bios.bcd to
OutputDir. Before it creates a store, it deletes the files bcdedit writes for
that store: the store itself and its .LOG, .LOG1, and .LOG2 files. It leaves
every other file in OutputDir alone. The GUIDs are fixed, so a rerun gives the
same stores. See docs/architecture.md#bcd for the elements and the reason for
each.

.PARAMETER OutputDir
Folder for the two stores. The script creates it.

.PARAMETER NoLoaderPath
Leaves the loader's path element out, as ConfigMgr's store does.

.PARAMETER RamdiskTftpBlockSize
Sets ramdisktftpblocksize, from 512 to 32768. The limits match the server's
TFTP limits in docs/architecture.md#tftp. The element is omitted when not given.

.PARAMETER RamdiskTftpWindowSize
Sets ramdisktftpwindowsize, from 1 to 32. The limits match the server's TFTP
limits in docs/architecture.md#tftp. The element is omitted when not given.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string]$OutputDir,

    [switch]$NoLoaderPath,

    [ValidateRange(512, 32768)]
    [int]$RamdiskTftpBlockSize,

    [ValidateRange(1, 32)]
    [int]$RamdiskTftpWindowSize
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$SetBlockSize = $PSBoundParameters.ContainsKey('RamdiskTftpBlockSize')
$SetWindowSize = $PSBoundParameters.ContainsKey('RamdiskTftpWindowSize')

$Variants = @(
    @{ Name = 'uefi'; LoaderId = '{7b0e1d6a-5c11-4f3e-9a52-2d3f6a8c0e01}'; Winload = 'winload.efi' },
    @{ Name = 'bios'; LoaderId = '{7b0e1d6a-5c11-4f3e-9a52-2d3f6a8c0e02}'; Winload = 'winload.exe' }
)

function Invoke-Bcdedit {
    param([string[]]$BcdArgs)
    $output = & bcdedit.exe @BcdArgs 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "bcdedit $($BcdArgs -join ' ') failed with exit code ${LASTEXITCODE}: $output"
    }
}

function New-BcdStore {
    param([hashtable]$Variant)

    $store = Join-Path $OutputDir "$($Variant.Name).bcd"
    $loader = $Variant.LoaderId

    foreach ($suffix in '', '.LOG', '.LOG1', '.LOG2') {
        Remove-Item -LiteralPath "$store$suffix" -Force -ErrorAction SilentlyContinue
    }

    Invoke-Bcdedit '/createstore', $store

    Invoke-Bcdedit '/store', $store, '/create', '{bootmgr}', '/d', 'deployarr boot manager'
    Invoke-Bcdedit '/store', $store, '/set', '{bootmgr}', 'timeout', '30'
    Invoke-Bcdedit '/store', $store, '/set', '{bootmgr}', 'fontpath', '\boot\fonts'

    Invoke-Bcdedit '/store', $store, '/create', '{ramdiskoptions}', '/d', 'deployarr ramdisk options'
    Invoke-Bcdedit '/store', $store, '/set', '{ramdiskoptions}', 'ramdisksdidevice', 'boot'
    Invoke-Bcdedit '/store', $store, '/set', '{ramdiskoptions}', 'ramdisksdipath', '\boot\boot.sdi'
    if ($SetBlockSize) {
        Invoke-Bcdedit '/store', $store, '/set', '{ramdiskoptions}', 'ramdisktftpblocksize', "$RamdiskTftpBlockSize"
    }
    if ($SetWindowSize) {
        Invoke-Bcdedit '/store', $store, '/set', '{ramdiskoptions}', 'ramdisktftpwindowsize', "$RamdiskTftpWindowSize"
    }

    Invoke-Bcdedit '/store', $store, '/create', $loader, '/d', 'deployarr WinPE', '/application', 'osloader'
    $ramdiskDevice = 'ramdisk=[boot]\boot\x64\boot.wim,{ramdiskoptions}'
    Invoke-Bcdedit '/store', $store, '/set', $loader, 'device', $ramdiskDevice
    Invoke-Bcdedit '/store', $store, '/set', $loader, 'osdevice', $ramdiskDevice
    Invoke-Bcdedit '/store', $store, '/set', $loader, 'systemroot', '\windows'
    Invoke-Bcdedit '/store', $store, '/set', $loader, 'winpe', 'yes'
    if (-not $NoLoaderPath) {
        Invoke-Bcdedit '/store', $store, '/set', $loader, 'path', "\windows\system32\boot\$($Variant.Winload)"
    }

    Invoke-Bcdedit '/store', $store, '/set', '{bootmgr}', 'displayorder', $loader
    Invoke-Bcdedit '/store', $store, '/set', '{bootmgr}', 'default', $loader

    $store
}

New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null
foreach ($variant in $Variants) {
    New-BcdStore $variant
}
