param([ValidateSet('Stable','Preview')][string]$Version, [switch]$NoBrowser, [int]$Port=0, [string]$DataDirectory)
$ErrorActionPreference='Stop'
$suiteRoot=Split-Path -Parent $PSScriptRoot
try {
    if (!$Version) {
        Add-Type -AssemblyName System.Windows.Forms
        Add-Type -AssemblyName System.Drawing
        [Windows.Forms.Application]::EnableVisualStyles()
        $form=New-Object Windows.Forms.Form
        $form.Text='NL Video Suite';$form.Size=New-Object Drawing.Size(510,310)
        $form.StartPosition='CenterScreen';$form.FormBorderStyle='FixedDialog';$form.MaximizeBox=$false
        $form.BackColor=[Drawing.ColorTranslator]::FromHtml('#17121E');$form.ForeColor=[Drawing.Color]::White
        $label=New-Object Windows.Forms.Label;$label.Text="NL VIDEO SUITE`nScegli la versione da aprire";$label.Location=New-Object Drawing.Point(24,22);$label.Size=New-Object Drawing.Size(450,52);$label.Font=New-Object Drawing.Font('Segoe UI',13);$form.Controls.Add($label)
        foreach($entry in @(@('Stable','Stabile','app/nl-video-suite.exe - progetti abituali',86),@('Preview','Anteprima aggiornata','Ultima build verificata - dati separati',152))) {
            $button=New-Object Windows.Forms.Button;$button.Text=$entry[1]+"`n"+$entry[2];$button.Tag=$entry[0];$button.Location=New-Object Drawing.Point(24,$entry[3]);$button.Size=New-Object Drawing.Size(445,57);$button.FlatStyle='Flat';$button.Font=New-Object Drawing.Font('Segoe UI',10);$button.FlatAppearance.BorderColor=[Drawing.ColorTranslator]::FromHtml('#AD85C7');$button.BackColor=[Drawing.ColorTranslator]::FromHtml('#39254D');if($entry[0] -eq 'Preview'){$button.BackColor=[Drawing.ColorTranslator]::FromHtml('#7531FF')};$button.ForeColor=[Drawing.Color]::White
            $button.Add_Click({$this.FindForm().Tag=$this.Tag;$this.FindForm().Close()});$form.Controls.Add($button)
        }
        $status=New-Object Windows.Forms.LinkLabel;$status.Text='Promozione 1.0 bloccata - stato e gate di release';$status.LinkColor=[Drawing.ColorTranslator]::FromHtml('#AD85C7');$status.Location=New-Object Drawing.Point(24,226);$status.Size=New-Object Drawing.Size(450,32);$status.Add_LinkClicked({$gateFile=Join-Path $suiteRoot 'sviluppo/roadmap/V1_PROMOTION.md';if(Test-Path -LiteralPath $gateFile){Start-Process $gateFile}else{[Windows.Forms.MessageBox]::Show('La promozione viene gestita nello sviluppo dopo i gate finali. Questa distribuzione non modifica la versione stabile.','Gate di release') | Out-Null}});$form.Controls.Add($status)
        $form.Add_Shown({$this.Activate()});$form.ShowDialog() | Out-Null;$Version=$form.Tag;$form.Dispose();if(!$Version){exit 0}
    }
    if($Version -eq 'Preview') {
        if(!(Test-Path -LiteralPath (Join-Path $suiteRoot 'sviluppo/tools/Avvia_Anteprima_Creativa.ps1'))){throw 'Anteprima non inclusa in questa distribuzione. Puoi aprire la Stabile; nessuna compilazione o installazione verrà avviata.'}
        $argsPreview=@{NoBrowser=$NoBrowser;OfferUpdate=(!$NoBrowser)};if($Port){$argsPreview.Port=$Port};if($DataDirectory){$argsPreview.DataDirectory=$DataDirectory}
        & (Join-Path $suiteRoot 'sviluppo/tools/Avvia_Anteprima_Creativa.ps1') @argsPreview
    } else {
        if($DataDirectory){throw 'La versione stabile usa soltanto il suo launcher e i suoi dati abituali.'}
        $stablePort=8088;if($Port){$stablePort=$Port}
        $existing=$null;try{$existing=Invoke-RestMethod "http://127.0.0.1:$stablePort/api/health" -TimeoutSec 3}catch{}
        if($existing -and ([string]$existing.data_dir).Replace('\\?\','').TrimEnd('\') -ne (Join-Path $suiteRoot 'data').TrimEnd('\')){throw 'La porta della Stabile è usata da dati diversi. Nessun processo interrotto: chiudi la vecchia istanza oppure usa una porta diversa.'}
        $argsStable=@{NoBrowser=$NoBrowser};if($Port){$argsStable.Port=$Port}
        & (Join-Path $PSScriptRoot 'start.ps1') @argsStable
    }
} catch {
    if(!$NoBrowser){Add-Type -AssemblyName System.Windows.Forms;[Windows.Forms.MessageBox]::Show($_.Exception.Message,'NL Video Suite - avvio non riuscito','OK','Error') | Out-Null}
    Write-Error $_;exit 1
}
