param([int]$Port=8088,[switch]$NoBrowser)
$ErrorActionPreference='Stop'
$suiteRoot=Split-Path -Parent $PSScriptRoot
$suiteExe=Join-Path $PSScriptRoot 'nl-video-suite.exe'
if (!(Test-Path -LiteralPath $suiteExe)) {throw 'Pacchetto incompleto: app\nl-video-suite.exe assente. Non serve installare Rust: ricopia il pacchetto portable completo.'}
function Normalize-SuitePath([string]$Value) { [IO.Path]::GetFullPath($Value.Replace('\\?\','')).TrimEnd('\') }
$suiteUrl="http://127.0.0.1:$Port"
function Get-SuiteHealth {try {Invoke-RestMethod "$suiteUrl/api/health" -TimeoutSec 4} catch {$null}}
$health=Get-SuiteHealth
if ($health) {
    if ($health.application -ne 'nl-video-suite' -or !$health.installation_root -or (Normalize-SuitePath $health.installation_root) -ne (Normalize-SuitePath $suiteRoot)) {
        throw "La porta $Port appartiene a un'altra istanza. Fermala o avvia app\start.ps1 -Port 8089. Nessun processo modificato."
    }
    Write-Host "NL Video Suite gia attiva: $suiteUrl"
    if (!$NoBrowser) {Start-Process $suiteUrl}; exit 0
}
$logs=Join-Path $suiteRoot 'data\logs'
New-Item -ItemType Directory -Force -Path $logs | Out-Null
$argsText="--port $Port --data-dir `"$(Join-Path $suiteRoot 'data')`""
$process=Start-Process -FilePath $suiteExe -ArgumentList $argsText -WorkingDirectory $suiteRoot -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $logs 'server.log') -RedirectStandardError (Join-Path $logs 'server.err.log')
for ($attempt=0;$attempt -lt 30;$attempt++) {
    Start-Sleep -Milliseconds 400
    $process.Refresh()
    if ($process.HasExited) {throw "Avvio non riuscito. Leggi data\logs\server.err.log oppure esegui Verifica_PC.cmd."}
    $health=Get-SuiteHealth
    if ($health -and $health.application -eq 'nl-video-suite' -and $health.installation_root -and (Normalize-SuitePath $health.installation_root) -eq (Normalize-SuitePath $suiteRoot)) {
        Write-Host "NL Video Suite pronta: $suiteUrl"
        if (!$NoBrowser) {Start-Process $suiteUrl}; exit 0
    }
}
throw 'Avvio non confermato. Controlla i log prima di rilanciare.'
