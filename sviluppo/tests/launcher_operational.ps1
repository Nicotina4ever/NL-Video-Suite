$ErrorActionPreference='Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @"
using System;using System.Collections.Generic;using System.Text;using System.Runtime.InteropServices;
public static class NLWindowProbe {
 public delegate bool Child(IntPtr h,IntPtr p);
 [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr h,Child c,IntPtr p);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr h,StringBuilder b,int n);
 [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr h,int m,IntPtr w,IntPtr l);
 [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h,IntPtr dc,uint flags);
 public static IntPtr[] Children(IntPtr h){var a=new List<IntPtr>();EnumChildWindows(h,(x,p)=>{a.Add(x);return true;},IntPtr.Zero);return a.ToArray();}
 public static string Text(IntPtr h){var b=new StringBuilder(2048);GetWindowText(h,b,b.Capacity);return b.ToString();}
}
"@
$suiteRoot=(Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
$out=Join-Path $PSScriptRoot 'artifacts/launcher-operational'
New-Item -ItemType Directory -Force -Path $out | Out-Null
$testData=Join-Path $out 'gui dati isolati'
$launcher=Join-Path $suiteRoot 'app/launcher.ps1'
$child=Start-Process powershell.exe -ArgumentList "-NoProfile -STA -ExecutionPolicy Bypass -File `"$launcher`" -NoBrowser -Port 18201 -DataDirectory `"$testData`"" -WindowStyle Normal -PassThru -RedirectStandardOutput (Join-Path $out 'launcher.log') -RedirectStandardError (Join-Path $out 'launcher-error.log')
$buildTimes=@{};Get-ChildItem -LiteralPath (Join-Path $suiteRoot 'sviluppo/target/release') -Filter '*.exe' -File | ForEach-Object {$buildTimes[$_.FullName]=$_.LastWriteTimeUtc.Ticks}
$ownedServer=$null
try {
    $condition=New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::ProcessIdProperty,$child.Id)
    $nameCondition=New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty,'NL Video Suite')
    $condition=New-Object System.Windows.Automation.AndCondition($condition,$nameCondition)
    $window=$null
    for($i=0;$i -lt 300;$i++){$window=[System.Windows.Automation.AutomationElement]::RootElement.FindFirst([System.Windows.Automation.TreeScope]::Children,$condition);if($window){break};Start-Sleep -Milliseconds 100}
    if(!$window){$child.Refresh();throw ('Launcher GUI non trovato; process exited='+$child.HasExited+'; interactive='+[Environment]::UserInteractive)}
    $handle=[IntPtr]$window.Current.NativeWindowHandle
    $names=@();$previewHandle=[IntPtr]::Zero
    for($i=0;$i -lt 150;$i++){
        $names=@();foreach($h in [NLWindowProbe]::Children($handle)){$text=[NLWindowProbe]::Text($h);$names+=$text;if($text -like 'Anteprima aggiornata*'){$previewHandle=$h}}
        if($previewHandle -ne [IntPtr]::Zero){break};Start-Sleep -Milliseconds 150
    }
    if($previewHandle -eq [IntPtr]::Zero){throw ('Controlli nativi mancanti, handle='+$handle+'; '+($names -join '|'))}
    if(@($names | Where-Object {$_ -like 'Stabile*'}).Count -ne 1){throw 'Scelta Stabile non trovata'}
    $rect=$window.Current.BoundingRectangle
    $bitmap=New-Object Drawing.Bitmap([int]$rect.Width,[int]$rect.Height);$graphics=[Drawing.Graphics]::FromImage($bitmap);$dc=$graphics.GetHdc()
    [NLWindowProbe]::PrintWindow($handle,$dc,0) | Out-Null;$graphics.ReleaseHdc($dc);$bitmap.Save((Join-Path $out 'launcher-choice.png'));$graphics.Dispose();$bitmap.Dispose()
    [NLWindowProbe]::SendMessage($previewHandle,0x00F5,[IntPtr]::Zero,[IntPtr]::Zero) | Out-Null
    $health=$null
    for($i=0;$i -lt 80;$i++){try{$health=Invoke-RestMethod http://127.0.0.1:18201/api/health -TimeoutSec 2;break}catch{Start-Sleep -Milliseconds 150}}
    if(!$health -or ([string]$health.data_dir).Replace('\\?\','').TrimEnd('\') -ne $testData.TrimEnd('\')){throw 'Avvio o isolamento anteprima non verificato'}
    $listener=Get-NetTCPConnection -LocalPort 18201 -State Listen | Select-Object -First 1
    $process=Get-CimInstance Win32_Process -Filter ('ProcessId='+$listener.OwningProcess)
    if(!$process.CommandLine.Contains($testData)){throw 'Identita processo diversa dal test'}
    $ownedServer=$process.ProcessId
    if((Get-Item -LiteralPath $process.ExecutablePath).LastWriteTimeUtc.Ticks -ne $buildTimes[$process.ExecutablePath]){throw 'La build risulta modificata durante avvio'}
    $result=[ordered]@{passed=$true;gui_controls=$names;preview_executable=$process.ExecutablePath;isolated_data=$health.data_dir;compile_at_launch=$false;stable_launched=$false;shared_services_modified=$false}
    # Exercise missing-valid-build behavior without moving/deleting any EXE.
    function Get-FileHash {param($LiteralPath,$Algorithm);[pscustomobject]@{Hash='no-verified-build'}}
    try { & (Join-Path $suiteRoot 'sviluppo/tools/Avvia_Anteprima_Creativa.ps1') -NoBrowser -Port 18202 -DataDirectory (Join-Path $out 'must-not-start');throw 'La build non verificata non doveva avviarsi' }
    catch {if($_.Exception.Message -notlike '*manca una build con verifica GUI*'){throw};$result.no_valid_build_message=$_.Exception.Message}
    Remove-Item Function:Get-FileHash
    $fixture=Join-Path $out 'stable-routing/app';New-Item -ItemType Directory -Force -Path $fixture | Out-Null
    Copy-Item -LiteralPath $launcher -Destination (Join-Path $fixture 'launcher.ps1')
    'param([int]$Port=8088,[switch]$NoBrowser); @{Port=$Port;NoBrowser=[bool]$NoBrowser}|ConvertTo-Json|Set-Content (Join-Path $PSScriptRoot "routing.json")' | Set-Content (Join-Path $fixture 'start.ps1') -Encoding UTF8
    & powershell.exe -NoProfile -ExecutionPolicy Bypass -File (Join-Path $fixture 'launcher.ps1') -Version Stable -NoBrowser -Port 18203
    $routing=Get-Content (Join-Path $fixture 'routing.json') -Raw | ConvertFrom-Json
    if($routing.Port -ne 18203 -or !$routing.NoBrowser){throw 'Routing Stabile non corretto'}
    $result.stable_delegate_simulated=$true
    $result | ConvertTo-Json -Depth 3 | Set-Content -LiteralPath (Join-Path $out 'report.json') -Encoding UTF8
    'PASS launcher GUI, isolated preview, validated candidate, missing-build error'
} finally {
    if($ownedServer){Stop-Process -Id $ownedServer -ErrorAction SilentlyContinue}
    if(!$child.HasExited){Stop-Process -Id $child.Id -ErrorAction SilentlyContinue}
}
