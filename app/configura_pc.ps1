param([switch]$Avvia, [switch]$NoBrowser, [switch]$Desktop, [int]$Port=8088, [string]$ToolsDirectory)
$ErrorActionPreference='Stop'
$suiteRoot=Split-Path -Parent $PSScriptRoot
$settingsFile=Join-Path $suiteRoot 'data/local-tools.json'
function Test-MediaTools([string]$Directory) {
    $Directory -and (Test-Path -LiteralPath (Join-Path $Directory 'ffmpeg.exe')) -and (Test-Path -LiteralPath (Join-Path $Directory 'ffprobe.exe'))
}
$selected=$ToolsDirectory
if (!$selected -and (Test-Path -LiteralPath $settingsFile)) {
    $saved=Get-Content -Raw -LiteralPath $settingsFile | ConvertFrom-Json
    if (Test-MediaTools $saved.directory) {$selected=$saved.directory}
}
if (!$selected -and (Test-MediaTools (Join-Path $PSScriptRoot 'tools'))) {$selected=Join-Path $PSScriptRoot 'tools'}
if (!$selected) {
    $found=Get-Command ffmpeg.exe -ErrorAction SilentlyContinue
    if ($found -and (Test-MediaTools (Split-Path -Parent $found.Source))) {$selected=Split-Path -Parent $found.Source}
}
if (!$selected -and !$NoBrowser) {
    Add-Type -AssemblyName System.Windows.Forms
    [Windows.Forms.MessageBox]::Show('Scegli un ffmpeg.exe gia installato, nella stessa cartella di ffprobe.exe. Non scarico o installo componenti. Annullando puoi aprire l''editor, ma import/export richiedono FFmpeg.','NL Video Suite - configurazione PC') | Out-Null
    $picker=New-Object Windows.Forms.OpenFileDialog
    $picker.Title='Scegli ffmpeg.exe';$picker.Filter='FFmpeg|ffmpeg.exe'
    if ($picker.ShowDialog() -eq 'OK') {$selected=Split-Path -Parent $picker.FileName}
    $picker.Dispose()
}
if ($selected) {
    $selected=(Resolve-Path -LiteralPath $selected).Path
    if (!(Test-MediaTools $selected)) {throw 'Servono ffmpeg.exe e ffprobe.exe nella cartella scelta.'}
    foreach($name in @('ffmpeg.exe','ffprobe.exe')) {& (Join-Path $selected $name) -version *> $null;if ($LASTEXITCODE -ne 0) {throw "Strumento non avviabile: $name"}}
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $settingsFile) | Out-Null
    @{directory=$selected} | ConvertTo-Json | Set-Content -LiteralPath $settingsFile -Encoding UTF8
    $env:PATH=$selected+[IO.Path]::PathSeparator+$env:PATH
    Write-Host 'FFmpeg configurato solo per questa copia. Nessuna modifica al PATH globale.'
} else {Write-Host 'FFmpeg non configurato: editor disponibile, elaborazione media da configurare.'}
if (!$Avvia) {Write-Host 'Le modifiche si applicano al prossimo avvio del server della suite.';exit 0}
& (Join-Path $PSScriptRoot 'start.ps1') -Port $Port -NoBrowser
if (!$NoBrowser) {
    $url="http://127.0.0.1:$Port"
    $edge=@((Join-Path ${env:ProgramFiles(x86)} 'Microsoft/Edge/Application/msedge.exe'),(Join-Path $env:ProgramFiles 'Microsoft/Edge/Application/msedge.exe')) | Where-Object {Test-Path -LiteralPath $_} | Select-Object -First 1
    if ($Desktop -and $edge) {
        # A dedicated app window, reusing installed Edge. No browser/runtime download.
        $profile=Join-Path $suiteRoot 'data/desktop-profile'
        Start-Process -FilePath $edge -ArgumentList @("--app=$url",("--user-data-dir=`"$profile`""),'--no-first-run','--disk-cache-size=33554432','--media-cache-size=33554432')
    } else {Start-Process $url}
}
