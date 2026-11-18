param([switch]$PreparePython)
# Non-destructive snapshot: never move, delete, upgrade or pip-install into a
# source project. No hard links/junctions: archiving the source must be safe.
$ErrorActionPreference = 'Stop'
$suiteRoot = Split-Path -Parent $PSScriptRoot
if ((Split-Path -Leaf $suiteRoot) -eq 'sviluppo') { $suiteRoot = Split-Path -Parent $suiteRoot }
$runtimeRoot = Join-Path $suiteRoot 'runtime'
$engineRoot = Join-Path $suiteRoot 'engines'
$workflowRoot = Join-Path $suiteRoot 'workflows'
$clipsRoot = 'K:\AI\ClipsAI'
$zpRoot = 'K:\AI\ZioPaolo Video\ZP-Shorts-Studio-Stable'
$runningSuite = $null
try { $runningSuite = Invoke-RestMethod 'http://127.0.0.1:8088/api/health' -TimeoutSec 2 } catch { }
if ($runningSuite -and $runningSuite.application -eq 'nl-video-suite') {
    $activeJobs = Invoke-RestMethod 'http://127.0.0.1:8088/api/jobs' -TimeoutSec 3
    if (@($activeJobs | Where-Object { $_.status -in @('running','queued') }).Count -gt 0) {
        throw 'La suite ha lavori attivi: prepara il runtime ad app ferma.'
    }
}
New-Item -ItemType Directory -Force -Path $runtimeRoot,$engineRoot,$workflowRoot | Out-Null
$entries = New-Object System.Collections.Generic.List[object]
function Copy-SnapshotFile([string]$Source, [string]$Destination) {
    if (!(Test-Path -LiteralPath $Source -PathType Leaf)) { throw "Sorgente mancante: $Source" }
    $sourceHash = (Get-FileHash -LiteralPath $Source -Algorithm SHA256).Hash
    if (Test-Path -LiteralPath $Destination) {
        if ((Get-FileHash -LiteralPath $Destination -Algorithm SHA256).Hash -ne $sourceHash) {
            throw "Snapshot diverso gia presente, non sovrascritto: $Destination"
        }
    } else {
        New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Destination) | Out-Null
        Copy-Item -LiteralPath $Source -Destination $Destination
    }
    $entries.Add([pscustomobject]@{source=$Source; destination=$Destination.Substring($suiteRoot.Length+1); sha256=$sourceHash})
}
foreach ($name in @('motore.py','sgrossatore_raw.py','taglia_clip_per_editing.py','shorts.py','audio_events.py')) {
    Copy-SnapshotFile (Join-Path $clipsRoot $name) (Join-Path $engineRoot "clipsai\$name")
}
Copy-SnapshotFile (Join-Path $zpRoot 'src\models\short.py') (Join-Path $engineRoot 'zp_shorts\short.py')
# Preserve available source license notices alongside each snapshot.
foreach ($pair in @(@($clipsRoot,'clipsai'),@($zpRoot,'zp_shorts'))) {
    foreach ($license in Get-ChildItem -LiteralPath $pair[0] -File | Where-Object { $_.Name -match '^(LICENSE|COPYING|NOTICE)' }) {
        Copy-SnapshotFile $license.FullName (Join-Path $engineRoot ($pair[1]+'\'+$license.Name))
    }
}
foreach ($pair in @(@('K:\AI\Regista_AI\workflow_comfy','regista'),@('K:\AI\Calliope\example_ComfyUI_workflows','calliope'))) {
    if (!(Test-Path -LiteralPath $pair[0])) { continue }
    foreach ($file in Get-ChildItem -LiteralPath $pair[0] -Recurse -File -Filter '*.json') {
        if ($file.FullName -match '[\\/](node_modules|\.git|archive|archivio)[\\/]') { continue }
        $relative = $file.FullName.Substring($pair[0].Length).TrimStart('\','/')
        Copy-SnapshotFile $file.FullName (Join-Path $workflowRoot ($pair[1]+'\'+$relative))
    }
}
if ($PreparePython) {
    $sourcePython = Join-Path $clipsRoot 'python_env'
    $targetPython = Join-Path $runtimeRoot 'python'
    $readyMarker = Join-Path $runtimeRoot 'python-ready.json'
    # /E copies without removing anything. /XJ avoids hidden links to sources.
    # Do not combine DLLs from another Python/PyTorch installation.
    if (!(Test-Path -LiteralPath $readyMarker)) {
        & robocopy $sourcePython $targetPython /E /XJ /R:1 /W:1 /XD __pycache__ /XF '*.pyc' /NFL /NDL /NJH /NJS /NP
        if ($LASTEXITCODE -ge 8) { throw "Copia runtime fallita: robocopy $LASTEXITCODE" }
    } else { Write-Host 'Runtime gia preparato: nessun pacchetto sovrascritto.' }
    $pythonExe = Join-Path $targetPython 'python.exe'
    & $pythonExe -c "import sys,json,importlib.metadata as m; import torch,torchaudio,whisperx,requests; print(json.dumps({'python':sys.version,'torch':torch.__version__,'whisperx':m.version('whisperx')}))"
    if ($LASTEXITCODE -ne 0) { throw 'Verifica runtime fallita; gli originali sono invariati.' }
    & $pythonExe -m pip freeze --all | Out-File -LiteralPath (Join-Path $runtimeRoot 'requirements-installed.txt') -Encoding utf8
    if ($LASTEXITCODE -ne 0) { throw 'Inventario pacchetti fallito' }
    @{verified_at=(Get-Date).ToUniversalTime().ToString('o'); python=$pythonExe} | ConvertTo-Json | Out-File -LiteralPath $readyMarker -Encoding utf8
    # Optional AST weights belong to the engine snapshot, not its old folder.
    $astSource = Join-Path $clipsRoot 'models\hf'
    if (Test-Path -LiteralPath $astSource) {
        & robocopy $astSource (Join-Path $engineRoot 'clipsai\models\hf') /E /XJ /R:1 /W:1 /XD .locks /NFL /NDL /NJH /NJS /NP
        if ($LASTEXITCODE -ge 8) { throw 'Copia cache AST fallita' }
    }
}
$manifest = [ordered]@{
    created_at=(Get-Date).ToUniversalTime().ToString('o'); files=$entries;
    python_source=(Join-Path $clipsRoot 'python_env');
    python_ready=(Test-Path -LiteralPath (Join-Path $runtimeRoot 'python\python.exe'));
    original_projects_modified=$false;
    limitations=@('Parita funzionale completa non ancora raggiunta','ComfyUI e Ollama restano servizi esterni','Modelli Whisper e alignment esterni da conservare','Licenze da verificare prima della distribuzione a terzi')
}
$manifest | ConvertTo-Json -Depth 8 | Out-File -LiteralPath (Join-Path $runtimeRoot 'snapshot-manifest.json') -Encoding utf8
Write-Host 'Snapshot interni pronti. Nessun originale cancellato. Non eliminare ancora i vecchi progetti.'
exit 0
