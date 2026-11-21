# Avvia_Shell.ps1 — starts the Premiere-style shell (web-app) against a running backend.
# Expectation: the suite backend is already listening (stable 8088 or preview 8099);
# the dev proxy forwards /api there. Nothing is built automatically.
param([int]$BackendPort = 8099, [int]$ShellPort = 5173)
$ErrorActionPreference='Stop'
$suiteRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$backend = "http://127.0.0.1:$BackendPort"
try { Invoke-RestMethod "$backend/api/health" -TimeoutSec 4 | Out-Null } catch {
    throw "Backend non in ascolto su $backend. Avvia prima la suite (Avvia_NL_Video_Suite.cmd o Avvia_Anteprima_Creativa)."
}
$node = Get-Command node -ErrorAction Stop
"Node: $($node.Source)"
Push-Location (Join-Path $suiteRoot 'sviluppo/web-app')
try {
    if (-not (Test-Path 'node_modules')) { "Installazione dipendenze shell..."; npm install --no-audit --no-fund }
    $env:NL_SUITE_BACKEND = $backend
    "Shell su http://127.0.0.1:$ShellPort (proxy $backend). Ctrl+C per chiudere."
    npm run dev -- --port $ShellPort --strictPort
} finally { Pop-Location }
