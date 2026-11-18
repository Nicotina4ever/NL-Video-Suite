$ErrorActionPreference='Stop'
$suiteRoot=Split-Path -Parent $PSScriptRoot
$pidFile=Join-Path $suiteRoot 'data\server.pid'
if (!(Test-Path -LiteralPath $pidFile)) {Write-Host 'La suite non risulta avviata.';exit 0}
$suiteProcessId=[int](Get-Content -Raw -LiteralPath $pidFile).Trim()
$process=Get-Process -Id $suiteProcessId -ErrorAction SilentlyContinue
if (!$process) {Write-Host 'La suite e gia ferma.';exit 0}
$expected=Join-Path $PSScriptRoot 'nl-video-suite.exe'
if ($process.Path -ne $expected) {throw 'PID non appartenente a questa copia portable: nessun processo terminato.'}
$jobsFile=Join-Path $suiteRoot 'data\jobs.json'
if (Test-Path -LiteralPath $jobsFile) {
    $jobs=Get-Content -Raw -LiteralPath $jobsFile | ConvertFrom-Json
    $active=@($jobs | Where-Object { $_.status -in @('queued','running') })
    if ($active.Count -gt 0) {throw 'Ci sono lavori in corso. Attendi la loro conclusione prima di fermare la suite; i servizi IA non sono stati toccati.'}
}
# Service launchers are children of the suite too. A recursive taskkill would
# terminate ComfyUI/Ollama along with the server, including independent sessions.
Stop-Process -Id $suiteProcessId -Force
$process.WaitForExit(5000) | Out-Null
Write-Host 'Suite fermata. I servizi IA esterni non sono stati modificati.'
