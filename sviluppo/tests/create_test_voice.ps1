param([Parameter(Mandatory=$true)][string]$Output)
$ErrorActionPreference='Stop'
Add-Type -AssemblyName System.Speech
$voice=New-Object System.Speech.Synthesis.SpeechSynthesizer
try {
    $voice.SelectVoice('Microsoft Elsa Desktop')
    $voice.SetOutputToWaveFile($Output)
    $voice.Speak('Questa e una prova della nuova applicazione. La voce resta chiara e i sottotitoli seguono il parlato.')
} finally {$voice.Dispose()}
