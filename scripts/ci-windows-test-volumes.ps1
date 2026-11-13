# Mounts small FAT32, exFAT and ReFS volumes (expandable VHDX files in RUNNER_TEMP) on CI's
# windows-latest runner, for git-core's Windows walker tests (#228), and names a folder on each
# in BROWSITORY_TEST_FAT32_DIR, BROWSITORY_TEST_EXFAT_DIR and BROWSITORY_TEST_REFS_DIR
# (through GITHUB_ENV). On CI those tests fail when a variable is missing; locally they skip.
#
# FAT32 and exFAT have no opens by file ID, no POSIX deletes or renames and no hard links, so
# the walker's by-name and classic fallbacks run there. ReFS is formatted with integrity
# streams on, so a new file gets FILE_ATTRIBUTE_INTEGRITY_STREAM.
#
# Needs an elevated shell (the runner is), and only the Storage module and diskpart, both part
# of Windows; no Hyper-V.
$ErrorActionPreference = 'Stop'

function Mount-TestVolume([string] $Name, [string] $FileSystem, [int] $SizeMB, [hashtable] $Format) {
    $vhd = Join-Path $env:RUNNER_TEMP "$Name.vhdx"
    $script = Join-Path $env:RUNNER_TEMP "$Name.diskpart"
    "create vdisk file=`"$vhd`" maximum=$SizeMB type=expandable" |
        Set-Content -Encoding ascii $script
    diskpart /s $script | Out-Host
    if ($LASTEXITCODE -ne 0) { throw "diskpart failed for $Name ($LASTEXITCODE)" }
    $disk = Mount-DiskImage -ImagePath $vhd -PassThru | Get-Disk
    Initialize-Disk -Number $disk.Number -PartitionStyle GPT
    $partition = New-Partition -DiskNumber $disk.Number -UseMaximumSize -AssignDriveLetter
    $volume = Format-Volume -Partition $partition -FileSystem $FileSystem -NewFileSystemLabel $Name `
        -Confirm:$false @Format
    # Read back: the object New-Partition returns may not show the letter assigned yet.
    $letter = (Get-Partition -DiskNumber $disk.Number | Where-Object DriveLetter).DriveLetter
    if (-not $letter) { throw "no drive letter for $Name" }
    $dir = "${letter}:\browsitory-tests"
    New-Item -ItemType Directory -Path $dir | Out-Null
    Write-Host "$Name`: $($volume.FileSystem) at $dir"
    return $dir
}

$fat32 = Mount-TestVolume 'fat32' 'FAT32' 256 @{}
$exfat = Mount-TestVolume 'exfat' 'exFAT' 256 @{}
$refs = Mount-TestVolume 'refs' 'ReFS' 4096 @{ SetIntegrityStreams = $true }

Add-Content -Encoding utf8 $env:GITHUB_ENV @(
    "BROWSITORY_TEST_FAT32_DIR=$fat32",
    "BROWSITORY_TEST_EXFAT_DIR=$exfat",
    "BROWSITORY_TEST_REFS_DIR=$refs"
)
