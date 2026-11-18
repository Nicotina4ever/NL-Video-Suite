$ErrorActionPreference='Stop'
Add-Type -TypeDefinition @"
using System;using System.Runtime.InteropServices;public static class NLUpdatePrompt {[DllImport("user32.dll",CharSet=CharSet.Unicode)]public static extern IntPtr FindWindow(string c,string title);[DllImport("user32.dll")]public static extern uint GetWindowThreadProcessId(IntPtr h,out uint pid);[DllImport("user32.dll")]public static extern IntPtr SendMessage(IntPtr h,int m,IntPtr w,IntPtr l);}
"@
$suite=(Resolve-Path "$PSScriptRoot/../..").Path
$out=Join-Path $PSScriptRoot 'artifacts/launcher-update'
$data=Join-Path $out 'isolated data'
New-Item -ItemType Directory -Force $data|Out-Null
'{"comfy_url":"http://127.0.0.1:9","ollama_url":"http://127.0.0.1:9","legacy_sources":{}}'|Set-Content (Join-Path $data settings.json)
$socket=New-Object Net.Sockets.TcpListener([Net.IPAddress]::Loopback,0);$socket.Start();$port=$socket.LocalEndpoint.Port;$socket.Stop()
$oldExe=Join-Path $suite 'sviluppo/target/release/nl-video-suite-gui.exe'
$old=Start-Process $oldExe -ArgumentList "--port $port --data-dir `"$data`"" -WorkingDirectory $suite -WindowStyle Hidden -PassThru
$child=$null;$newPid=$null;$report=@{passed=$false;real_native_update=$true;shared_services_modified=$false}
try {
 for($i=0;$i -lt 60;$i++){try{$health=Invoke-RestMethod "http://127.0.0.1:$port/api/health" -TimeoutSec 8;break}catch{Start-Sleep -Milliseconds 100}}
 if(!$health){throw 'Private preview did not start'}
 $helper=Join-Path $suite 'sviluppo/tools/Avvia_Anteprima_Creativa.ps1'
 # Admission is tested with an explicitly simulated busy queue; the process is real and private.
 function Invoke-RestMethod {param($Uri,$TimeoutSec) if($Uri -like '*/api/jobs'){return @(@{status='running'})};Microsoft.PowerShell.Utility\Invoke-RestMethod -Uri $Uri -TimeoutSec $TimeoutSec}
 $blocked=$false
 try{& $helper -Port $port -DataDirectory $data -NoBrowser -OfferUpdate}catch{if($_.Exception.Message -like '*ancora al lavoro*'){$blocked=$true}else{throw}}finally{Remove-Item Function:Invoke-RestMethod}
 if(!$blocked -or $old.HasExited){throw 'Simulated active queue did not preserve private preview'}
 $report.busy_queue_simulated=$true
 $child=Start-Process powershell.exe -ArgumentList "-NoProfile -STA -ExecutionPolicy Bypass -File `"$helper`" -Port $port -DataDirectory `"$data`" -NoBrowser -OfferUpdate" -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $out launch.log) -RedirectStandardError (Join-Path $out error.log)
 $handle=[IntPtr]::Zero
 for($i=0;$i -lt 900;$i++){$h=[NLUpdatePrompt]::FindWindow('#32770','Aggiorna Anteprima');$windowPid=[uint32]0;if($h -ne [IntPtr]::Zero){[NLUpdatePrompt]::GetWindowThreadProcessId($h,[ref]$windowPid)|Out-Null;if($windowPid -eq $child.Id){$handle=$h;break}};if($child.HasExited){throw (Get-Content (Join-Path $out error.log) -Raw)};Start-Sleep -Milliseconds 100}
 if($handle -eq [IntPtr]::Zero){throw 'Update confirmation not shown'}
 [NLUpdatePrompt]::SendMessage($handle,0x111,[IntPtr]6,[IntPtr]::Zero)|Out-Null
 if(!$child.WaitForExit(15000)){throw 'Update did not finish'}
 if($null -ne $child.ExitCode -and $child.ExitCode -ne 0){throw (Get-Content (Join-Path $out error.log) -Raw)}
 $old.Refresh();if(!$old.HasExited){throw 'Private old preview remained active'}
 $health=Invoke-RestMethod "http://127.0.0.1:$port/api/health" -TimeoutSec 8
 if(([string]$health.data_dir).Replace('\\?\','').TrimEnd('\') -ne $data.TrimEnd('\')){throw 'Private data changed'}
 $listener=Get-NetTCPConnection -LocalPort $port -State Listen|Select-Object -First 1
 $newPid=$listener.OwningProcess;$new=Get-Process -Id $newPid
 if($new.Path -notlike '*nl-video-suite-writing.exe'){throw 'Latest verified writing candidate not used'}
 $report.old_id=$old.Id;$report.new_id=$newPid;$report.data=$data;$report.executable=$new.Path;$report.passed=$true
} finally {
 if(!$newPid){$receipt=Join-Path $data 'server.pid';if(Test-Path $receipt){$possible=[int](Get-Content $receipt);$record=Get-CimInstance Win32_Process -Filter ("ProcessId="+$possible);if($record.CommandLine -and $record.CommandLine.Contains($data)){$newPid=$possible}}}
 if($newPid){$p=Get-Process -Id $newPid -ErrorAction SilentlyContinue;if($p -and $p.Path -like "$suite\sviluppo\target\release\nl-video-suite*"){Stop-Process -Id $newPid}}
 if(!$old.HasExited){Stop-Process -Id $old.Id}
 if($child -and !$child.HasExited){Stop-Process -Id $child.Id}
 $report|ConvertTo-Json|Set-Content (Join-Path $out report.json)
}
$report|ConvertTo-Json -Compress
