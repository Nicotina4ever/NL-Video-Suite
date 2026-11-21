# Promuovi_Anteprima.ps1 — carries the certified preview EXE into the stable channel.
# The stable binary is replaced ONLY when a v1-baseline report passes against the exact
# SHA256 of the candidate. The previous stable binary is always kept under app/stabile-backup.
# Owner decision: this script exists to make promotion one deliberate command, not to
# imply that promotion gates (human QA, portable, licenses) are already closed.
[CmdletBinding(SupportsShouldProcess=$true)]
param([string]$Candidate = (Join-Path $PSScriptRoot '..\..\sviluppo\target\release\nl-video-suite.exe'))
$ErrorActionPreference='Stop'
$suiteRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$exe = (Resolve-Path $Candidate).Path
$digest = (Get-FileHash $exe -Algorithm SHA256).Hash.ToLowerInvariant()
$reports = Get-ChildItem -LiteralPath (Join-Path $suiteRoot 'sviluppo/tests/artifacts/v1-baseline') -Directory -ErrorAction SilentlyContinue | Sort-Object Name -Descending
$proof = $null
foreach($report in $reports){
    $file = Join-Path $report.FullName 'report.json'
    if(-not (Test-Path $file)){ continue }
    $candidate = Get-Content $file -Raw | ConvertFrom-Json
    if($candidate.passed -eq $true -and $candidate.tested_executable_sha256 -eq $digest){ $proof = [pscustomobject]@{report=$file; data=$candidate}; break }
}
if(-not $proof){ throw "Nessun report v1-baseline PASS con SHA256 $digest : il candidato non è certificabile." }
$scope = if($proof.data.full_regression -eq $false){ 'ATTENZIONE: il report è mirato (targeted), NON una regressione completa.' } else { 'Regressione completa registrata.' }
$target = Join-Path $suiteRoot 'app/nl-video-suite.exe'
$backup = Join-Path $suiteRoot 'app/stabile-backup'
New-Item -ItemType Directory -Force -Path $backup | Out-Null
$stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
if(Test-Path $target){
    $oldHash = (Get-FileHash $target -Algorithm SHA256).Hash.ToLowerInvariant()
    if($oldHash -ne $digest){
        Copy-Item $target (Join-Path $backup "nl-video-suite-$stamp.exe")
        "Backup della stabile precedente: nl-video-suite-$stamp.exe (SHA $oldHash)"
    }
}
$yes = Read-Host "Promuovere il candidato SHA $digest (report $(Split-Path $proof.report -Leaf))? $scope`nDigita PROMUOCI per confermare"
if($yes -ne 'PROMUOCI'){ "Nessuna promozione."; exit 0 }
if($PSCmdlet.ShouldProcess($target,'replace with certified candidate')){
    Copy-Item $exe $target -Force
    $receipt = [ordered]@{promoted_at=(Get-Date).ToString('o'); sha256=$digest; source=$exe; proof=$proof.report; scope=$scope}
    $receipt | ConvertTo-Json | Set-Content (Join-Path $suiteRoot 'app/ultima-promozione.json') -Encoding utf8
    "Promozione completata e ricevuta in app/ultima-promozione.json. I gate umani/portable/licenze restano responsabilità distinte."
}
