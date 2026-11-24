<#
.SYNOPSIS
    Builds the portable NL Video Suite bundle for Windows.

.DESCRIPTION
    Builds (unless -NoBuild) the release executable, then assembles a self-contained folder with the
    executable, the copied ComfyUI workflow library, ffmpeg/ffprobe when they are in the suite's
    app\tools, the distribution notes and a launcher. Nothing AI is bundled: ComfyUI, Ollama and the
    language models stay external and are configured in Settings.
    The folder can be copied to another Windows PC and started from the .cmd launcher.

.PARAMETER Version
    Version label used in the folder and zip names.

.PARAMETER NoBuild
    Use the existing target\release\filmcraft.exe instead of building.

.PARAMETER SuiteRoot
    The NL Video Suite folder that holds `workflows` and `app\tools` (default: three levels above this
    script, i.e. the workspace root).

.PARAMETER OutputRoot
    Where the bundle folder and zip are written (default: packaging\nl\out).

.PARAMETER SkipZip
    Leave the folder only, without the .zip archive.
#>
[CmdletBinding()]
param(
    [string]$Version = "0.1.0",
    [switch]$NoBuild,
    [string]$SuiteRoot = "",
    [string]$OutputRoot = (Join-Path $PSScriptRoot "out"),
    [switch]$SkipZip
)

$ErrorActionPreference = "Stop"
$repo = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
if (-not $SuiteRoot) {
    $SuiteRoot = (Resolve-Path (Join-Path $repo "..")).Path
}

if (-not $NoBuild) {
    Push-Location $repo
    try {
        Write-Host "Building the release executable…"
        & cargo build --release -p filmcraft
        if ($LASTEXITCODE -ne 0) { throw "cargo build failed with exit code $LASTEXITCODE" }
    } finally {
        Pop-Location
    }
}

$exe = Join-Path $repo "target\release\filmcraft.exe"
if (-not (Test-Path $exe)) {
    throw "release binary not found: $exe. Run this script without -NoBuild, or build it with: cargo build --release -p filmcraft"
}

$name = "NLVideoSuite-$Version-windows-x64"
$out = Join-Path $OutputRoot $name
if (Test-Path $out) { Remove-Item $out -Recurse -Force }
New-Item -ItemType Directory -Path $out | Out-Null

Copy-Item $exe (Join-Path $out "NL Video Suite.exe")
$cli = Join-Path $repo "target\release\filmcraft-cli.exe"
if (Test-Path $cli) { Copy-Item $cli (Join-Path $out "NL Video Suite CLI.exe") } else { Write-Warning "release CLI not found: automation tests will be skipped" }

$workflows = Join-Path $SuiteRoot "workflows"
if (Test-Path $workflows) {
    Copy-Item $workflows (Join-Path $out "workflows") -Recurse
    Write-Host "Workflows copied from $workflows"
} else {
    Write-Warning "no workflows folder at ${workflows}: the bundle will need one for generation"
}

$tools = Join-Path $SuiteRoot "app\tools"
if (Test-Path $tools) {
    New-Item -ItemType Directory -Path (Join-Path $out "tools") | Out-Null
    foreach ($tool in @("ffmpeg.exe", "ffprobe.exe")) {
        $source = Join-Path $tools $tool
        if (Test-Path $source) { Copy-Item $source (Join-Path $out "tools\$tool") } else { Write-Warning "missing $tool" }
    }
} else {
    Write-Warning "no app\tools folder: ffmpeg must be on PATH on the target PC"
}

$distributionDoc = Join-Path $SuiteRoot "documentazione\DISTRIBUZIONE.md"
if (Test-Path $distributionDoc) { Copy-Item $distributionDoc (Join-Path $out "DISTRIBUZIONE.md") }

$launcher = @(
    '@echo off',
    'setlocal',
    'set "NL_SUITE_ROOT=%~dp0"',
    'start "NL Video Suite" "%~dp0NL Video Suite.exe"',
    'endlocal'
)
Set-Content -Path (Join-Path $out "Avvia NL Video Suite.cmd") -Value $launcher -Encoding ASCII

$readme = @(
    "NL Video Suite $Version - portable Windows bundle",
    "",
    "Start it with: Avvia NL Video Suite.cmd",
    "",
    "What is inside:",
    "  NL Video Suite.exe      the editor (no installer, no registry writes)",
    "  workflows\              the ComfyUI workflow library",
    "  tools\                  ffmpeg/ffprobe copied from the suite when available",
    "  DISTRIBUZIONE.md        how to build on Linux and macOS, and how to point the suite at AI services",
    "",
    "What is NOT inside, by design:",
    "  ComfyUI, Ollama or any language model, and any generated media. Install them separately and",
    "  configure their addresses in Settings > NL Video Suite (ComfyUI address, language-model",
    "  provider, ASR folder). The suite never downloads a model by itself and never starts a service.",
    "",
    "Settings are per user (%APPDATA%\NL Video Suite); projects are normal files you choose."
)
Set-Content -Path (Join-Path $out "README.txt") -Value $readme -Encoding UTF8

if (-not $SkipZip) {
    $zip = "${out}.zip"
    if (Test-Path $zip) { Remove-Item $zip -Force }
    Compress-Archive -Path $out -DestinationPath $zip -CompressionLevel Optimal
    Write-Host "Portable bundle: $out"
    Write-Host "Zip:            $zip"
} else {
    Write-Host "Portable bundle: $out"
}
