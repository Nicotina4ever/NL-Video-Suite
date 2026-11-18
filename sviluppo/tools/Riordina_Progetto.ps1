param([switch]$RuntimeOnly,[switch]$Finalize)
$ErrorActionPreference='Stop'
$suiteRoot=Split-Path -Parent $PSScriptRoot
if ((Split-Path -Leaf $suiteRoot) -eq 'sviluppo') {$suiteRoot=Split-Path -Parent $suiteRoot}
$suiteRoot=[IO.Path]::GetFullPath($suiteRoot).TrimEnd('\')
$quarantine=Join-Path $suiteRoot 'da_eliminare'
New-Item -ItemType Directory -Force -Path $quarantine | Out-Null
$indexPath=Join-Path $quarantine 'indice.json'
$script:records=New-Object System.Collections.Generic.List[object]
if (Test-Path -LiteralPath $indexPath) {foreach($entry in (Get-Content -Raw -LiteralPath $indexPath | ConvertFrom-Json)) {$script:records.Add($entry)}}
function Move-SuiteItem([string]$Relative,[string]$DestinationRelative,[string]$Reason) {
    $source=[IO.Path]::GetFullPath((Join-Path $suiteRoot $Relative))
    $destination=[IO.Path]::GetFullPath((Join-Path $suiteRoot $DestinationRelative))
    # Validate final absolute targets before every directory move. No shell
    # chaining, globs, deletion, root moves, or operations on the four sources.
    if (!$source.StartsWith($suiteRoot+'\',[StringComparison]::OrdinalIgnoreCase) -or !$destination.StartsWith($suiteRoot+'\',[StringComparison]::OrdinalIgnoreCase)) {throw 'Percorso fuori dalla suite'}
    if (!(Test-Path -LiteralPath $source)) {return}
    if (Test-Path -LiteralPath $destination) {throw "Destinazione gia presente, non sovrascritta: $destination"}
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $destination) | Out-Null
    $size=if ((Get-Item -LiteralPath $source).PSIsContainer) {(Get-ChildItem -LiteralPath $source -File -Recurse | Measure-Object Length -Sum).Sum}else{(Get-Item -LiteralPath $source).Length}
    Move-Item -LiteralPath $source -Destination $destination
    $script:records.Add([pscustomobject]@{from=$Relative;to=$DestinationRelative;bytes=$size;reason=$Reason;at=(Get-Date).ToUniversalTime().ToString('o')})
    $script:records | ConvertTo-Json -Depth 6 | Out-File -LiteralPath $indexPath -Encoding utf8
}
if ($RuntimeOnly) {
    $libRoot=Join-Path $suiteRoot 'runtime\python\Lib\site-packages\torch\lib'
    foreach($file in Get-ChildItem -LiteralPath $libRoot -File -Filter '*.lib') {
        $relative=$file.FullName.Substring($suiteRoot.Length+1)
        Move-SuiteItem $relative ('da_eliminare\librerie_sviluppo_torch\'+$file.Name) 'Libreria per linking/compilazione C++; non usata dal runtime ordinario. Conservata per ripristino, da collaudare senza JIT/torch.compile.'
    }
    Move-SuiteItem 'runtime\python\get-pip.py' 'da_eliminare\provisioning\get-pip.py' 'Bootstrap pip gia eseguito; pip e metadati restano nel runtime.'
}
if ($Finalize) {
    $pidFile=Join-Path $suiteRoot 'data\server.pid'
    if (Test-Path -LiteralPath $pidFile) {
        $suiteProcessId=[int](Get-Content -Raw -LiteralPath $pidFile).Trim()
        $process=Get-Process -Id $suiteProcessId -ErrorAction SilentlyContinue
        if ($process -and $process.Path -like ($suiteRoot+'\target\*')) {throw 'Ferma prima la vecchia istanza target; il nuovo eseguibile deve essere in app.'}
    }
    Move-SuiteItem 'target' 'da_eliminare\build_cache_target' 'Cache di compilazione, debug, PDB e vecchi eseguibili; il binario operativo e in app. Rigenerabile con Cargo.'
    foreach($folder in @('src','bridge','web','tests','tools')) {Move-SuiteItem $folder ('sviluppo\'+$folder) 'Materiale di sviluppo conservato separato dalla distribuzione portable.'}
    foreach($file in @('Cargo.toml','Cargo.lock','Prepara_Runtime.cmd')) {Move-SuiteItem $file ('sviluppo\'+$file) 'Build e provisioning per sviluppatori, non richiesti per avviare la suite.'}
    foreach($file in @('ARCHITECTURE.md','RUNTIME.md','STATUS.md','WORKFLOW_DOCTOR.md')) {Move-SuiteItem $file ('documentazione\'+$file) 'Documentazione tecnica conservata.'}
    Move-SuiteItem 'README.md' 'documentazione\README_SVILUPPO.md' 'Guida tecnica precedente conservata; sostituita da guida utente portable in root.'
    foreach($file in @('start.ps1','stop.ps1')) {Move-SuiteItem $file ('da_eliminare\launcher_sostituiti\'+$file) 'Launcher precedente dipendente da target e da Cargo, sostituito da app/start.ps1 e app/stop.ps1.'}
}
Write-Host 'Riordino completato. Nessun file cancellato; indice in da_eliminare\indice.json.'
