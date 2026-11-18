param([ValidateRange(1024,65535)][int]$Port=8099, [switch]$NoBrowser, [string]$DataDirectory, [switch]$OfferUpdate)
$ErrorActionPreference='Stop'
$suiteRoot=(Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..\..')).Path
$previewData=Join-Path $suiteRoot 'sviluppo\anteprima_creativa\data'
if($DataDirectory){
    $candidateData=[IO.Path]::GetFullPath($DataDirectory)
    $developmentRoot=[IO.Path]::GetFullPath((Join-Path $suiteRoot 'sviluppo')).TrimEnd('\')+'\'
    if(!$candidateData.StartsWith($developmentRoot,[StringComparison]::OrdinalIgnoreCase)){throw 'I dati di anteprima devono restare nella cartella sviluppo, separati dalla stabile.'}
    $previewData=$candidateData
}
$plannerCandidate=Join-Path $suiteRoot 'sviluppo\target\release\nl-video-suite-planner.exe'
$audioCandidate=Join-Path $suiteRoot 'sviluppo\target\release\nl-video-suite-audio-first.exe'
$productionCandidate=Join-Path $suiteRoot 'sviluppo\target\release\nl-video-suite-preproduction.exe'
$reports=Get-ChildItem -LiteralPath (Join-Path $suiteRoot 'sviluppo/tests/artifacts/v1-baseline') -Directory -ErrorAction SilentlyContinue | Sort-Object Name -Descending
$executable=$null
$candidates=Get-ChildItem -LiteralPath (Join-Path $suiteRoot 'sviluppo/target/release') -Filter 'nl-video-suite*.exe' -File -ErrorAction SilentlyContinue | Sort-Object LastWriteTimeUtc -Descending
foreach($candidate in $candidates){
    $digest=(Get-FileHash -LiteralPath $candidate.FullName -Algorithm SHA256).Hash
    foreach($reportDirectory in $reports){
        $reportFile=Join-Path $reportDirectory.FullName 'report.json'
        if(!(Test-Path -LiteralPath $reportFile -PathType Leaf)){continue}
        try{$proof=Get-Content -LiteralPath $reportFile -Raw | ConvertFrom-Json}catch{continue}
        if($proof.passed -eq $true -and $proof.tested_executable_sha256 -eq $digest -and @($proof.results | Where-Object {$_.status -eq 'passed' -and $_.test -match 'preproduction|browser|narrative_library|reference_audio'}).Count -gt 0){$executable=$candidate.FullName;break}
    }
    if($executable){break}
}
if(!$executable){throw 'Anteprima non disponibile: manca una build con verifica GUI riuscita e hash corrispondente. Nessuna compilazione automatica. Puoi aprire la versione Stabile; la build va preparata durante lo sviluppo.'}
$url="http://127.0.0.1:$Port"
$health=$null
try {$health=Invoke-RestMethod -Uri "$url/api/health" -TimeoutSec 5} catch {}
if ($health) {
    $reported=([string]$health.data_dir).Replace('\\?\','').TrimEnd('\')
    if ($health.application -ne 'nl-video-suite' -or $reported -ne $previewData.TrimEnd('\')) {throw "Porta $Port occupata da un'altra istanza. Nessun processo e' stato fermato; scegli un'altra porta."}
    if ($executable -eq $plannerCandidate -or $executable -eq $audioCandidate -or $executable -eq $productionCandidate) {
        $javascript=(Invoke-WebRequest -Uri "$url/app.js" -TimeoutSec 5).Content
        if (!$javascript.Contains('window.NLMontagePlanner=')) {throw "L'anteprima aperta e' precedente al Montage Planner. Salva e chiudila, poi rilancia. Nessun processo o progetto e' stato interrotto."}
        $narrativeJavascript=(Invoke-WebRequest -Uri "$url/narrative.js" -TimeoutSec 5).Content
        if (!$narrativeJavascript.Contains('window.NLProductionPackage=') -or !$narrativeJavascript.Contains('window.NLNarrativeAudioFirst=')) {throw "L'anteprima aperta precede audio-first e dossier. Salva e chiudi solo NL Video Suite, poi rilancia. Nessun servizio condiviso e' stato interrotto."}
        if ($executable -eq $productionCandidate -and (!$narrativeJavascript.Contains('window.NLProductionPreset=') -or !$javascript.Contains('workflow-import-file'))) {throw "L'anteprima aperta precede importazione API e preset condivisi. Salva e chiudi solo NL Video Suite, poi rilancia. Nessun servizio condiviso e' stato interrotto."}
        if ($executable -eq $audioCandidate -and !$narrativeJavascript.Contains('window.NLReferenceRanges=')) {throw "L'anteprima aperta precede i binding audio temporali. Salva e chiudi solo NL Video Suite, poi rilancia. Nessun processo e' stato interrotto."}
    }
    $listener=Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue | Select-Object -First 1
    $restart=$false
    if($listener){
        $previewOwner=Get-Process -Id $listener.OwningProcess -ErrorAction Stop
        if($previewOwner.Path -and $previewOwner.Path -ne $executable){
            if(!$OfferUpdate){throw 'Una precedente anteprima è ancora aperta. Usa il launcher principale per aggiornare dopo aver salvato.'}
            $releaseRoot=[IO.Path]::GetFullPath((Join-Path $suiteRoot 'sviluppo/target/release')).TrimEnd('\')+'\'
            if(!([IO.Path]::GetFullPath($previewOwner.Path)).StartsWith($releaseRoot,[StringComparison]::OrdinalIgnoreCase) -or $previewOwner.Name -notlike 'nl-video-suite*'){throw 'Istanza non riconosciuta come Anteprima di sviluppo: nessun processo interrotto.'}
            $currentJobs=Invoke-RestMethod -Uri "$url/api/jobs" -TimeoutSec 5
            $active=@($currentJobs | Where-Object {$_.status -in @('running','queued')})
            if($active.Count){throw 'Anteprima precedente ancora al lavoro. Attendi la conclusione in Lavori e recupero, poi ripremi Anteprima aggiornata. Non interrompo le elaborazioni.'}
            Add-Type -AssemblyName System.Windows.Forms
            $answer=[Windows.Forms.MessageBox]::Show('È pronta una nuova Anteprima. Prima salva le modifiche nella pagina aperta. Hai salvato e vuoi riavviare soltanto NL Video Suite? Progetti e cache restano conservati; Ollama e ComfyUI non vengono toccati.','Aggiorna Anteprima','YesNo','Question')
            if($answer -ne [Windows.Forms.DialogResult]::Yes){return}
            $currentJobs=Invoke-RestMethod -Uri "$url/api/jobs" -TimeoutSec 5
            $active=@($currentJobs | Where-Object {$_.status -in @('running','queued')})
            $verified=Get-Process -Id $previewOwner.Id -ErrorAction Stop
            if($active.Count -or $verified.Path -ne $previewOwner.Path -or $verified.StartTime -ne $previewOwner.StartTime){throw 'Lo stato della suite è cambiato: aggiornamento annullato, nessun processo interrotto.'}
            Stop-Process -Id $verified.Id -ErrorAction Stop
            $verified.WaitForExit(5000) | Out-Null
            $restart=$true
        }
    }
    if(!$restart){
        if (!$NoBrowser) {Start-Process $url}
        return [PSCustomObject]@{Started=$false;Url=$url;Data=$previewData;Executable=$executable;Id=$null}
    }
}
New-Item -ItemType Directory -Path $previewData -Force | Out-Null
$process=Start-Process -FilePath $executable -ArgumentList "--port $Port --data-dir `"$previewData`"" -WorkingDirectory $suiteRoot -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $previewData 'preview.log') -RedirectStandardError (Join-Path $previewData 'preview-error.log')
for ($attempt=0;$attempt -lt 8;$attempt++) {
    if ($process.HasExited) {throw "Anteprima non avviata: controlla $previewData\preview-error.log. La versione stabile e' intatta."}
    try {$health=Invoke-RestMethod -Uri "$url/api/health" -TimeoutSec 5;break} catch {Start-Sleep -Milliseconds 300}
}
if (!$health) {throw "Avvio ancora non verificato: controlla i log dell'anteprima. Nessun servizio IA e' stato avviato."}
if (!$NoBrowser) {Start-Process $url}
[PSCustomObject]@{Started=$true;Url=$url;Data=$previewData;Executable=$executable;Id=$process.Id}
