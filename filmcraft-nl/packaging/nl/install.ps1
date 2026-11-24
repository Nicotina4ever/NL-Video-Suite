<#
.SYNOPSIS
    Per-user installer for the Windows portable bundle of NL Video Suite.

.DESCRIPTION
    Copies the portable bundle under the user's Programs folder (no administrator rights), creates
    Start Menu and optional desktop shortcuts and an uninstall entry. It does not install, start or
    download ComfyUI, Ollama, language models or ffmpeg: the bundle carries ffmpeg when the suite has
    it, and the AI services stay external and are configured in Settings > NL Video Suite.

.PARAMETER Bundle
    Portable folder produced by packaging\nl\portable.ps1 (contains "NL Video Suite.exe"). Defaults
    to the current folder.

.PARAMETER Destination
    Install folder. Defaults to %LOCALAPPDATA%\Programs\NL Video Suite.

.PARAMETER DesktopShortcut
    Also create a desktop shortcut.

.PARAMETER NoShortcuts
    Copy the files only (used by tests and by a portable use).

.PARAMETER Uninstall
    Remove the installed folder and shortcuts.
#>
[CmdletBinding()]
param(
    [string]$Bundle = "",
    [string]$Destination = (Join-Path $env:LOCALAPPDATA "Programs\NL Video Suite"),
    [switch]$DesktopShortcut,
    [switch]$NoShortcuts,
    [switch]$Uninstall
)

$ErrorActionPreference = "Stop"
$shortcutName = "NL Video Suite.lnk"
$startMenu = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs"
$desktop = [Environment]::GetFolderPath("Desktop")

if ($Uninstall) {
    if (Test-Path $Destination) { Remove-Item $Destination -Recurse -Force }
    foreach ($dir in @($startMenu, $desktop)) {
        $link = Join-Path $dir $shortcutName
        if (Test-Path $link) { Remove-Item $link -Force }
    }
    Write-Host "NL Video Suite removed from $Destination"
    exit 0
}

if (-not $Bundle) { $Bundle = (Get-Location).Path }
$exe = Join-Path $Bundle "NL Video Suite.exe"
if (-not (Test-Path $exe)) {
    throw "no 'NL Video Suite.exe' in $Bundle (run packaging\nl\portable.ps1 first)"
}

New-Item -ItemType Directory -Path $Destination -Force | Out-Null
Copy-Item (Join-Path $Bundle "*") $Destination -Recurse -Force
$installedExe = Join-Path $Destination "NL Video Suite.exe"

if (-not $NoShortcuts) {
    $shell = New-Object -ComObject WScript.Shell
    foreach ($dir in @($startMenu, $desktop)) {
        if ($dir -eq $desktop -and -not $DesktopShortcut) { continue }
        New-Item -ItemType Directory -Path $dir -Force | Out-Null
        $link = $shell.CreateShortcut((Join-Path $dir $shortcutName))
        $link.TargetPath = $installedExe
        $link.WorkingDirectory = $Destination
        $link.Description = "NL Video Suite"
        $link.Save()
    }
}

# Uninstall entry under the user's Programs and Features (HKCU, no admin).
$key = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\NLVideoSuite"
New-Item -Path $key -Force | Out-Null
Set-ItemProperty -Path $key -Name DisplayName -Value "NL Video Suite"
Set-ItemProperty -Path $key -Name DisplayVersion -Value "0.2.0"
Set-ItemProperty -Path $key -Name InstallLocation -Value $Destination
Set-ItemProperty -Path $key -Name DisplayIcon -Value $installedExe
Set-ItemProperty -Path $key -Name NoModify -Value 1 -Type DWord
Set-ItemProperty -Path $key -Name NoRepair -Value 1 -Type DWord

Write-Host "NL Video Suite installed in $Destination"
Write-Host "Uninstall: .\install.ps1 -Destination '$Destination' -Uninstall"
