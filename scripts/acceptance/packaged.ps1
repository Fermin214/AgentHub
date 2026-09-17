param([Parameter(Mandatory=$true)][string]$PortableZip,[Parameter(Mandatory=$true)][string]$Cli,[Parameter(Mandatory=$true)][string]$Node,[Parameter(Mandatory=$true)][string]$Case,[Parameter(Mandatory=$true)][string]$DesktopSha256)
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'vm-safety.ps1')
Assert-TestDesktop
Assert-CleanTestInstallation
if($Case -notmatch '^[a-z0-9-]+$'){throw 'Invalid case name'}
$root=Assert-AcceptancePath (Join-Path 'C:\AgentHub-VM-Test\cases' $Case) 'C:\AgentHub-VM-Test\cases'
if(Test-Path $root){throw 'Preserve existing case'}
New-Item -ItemType Directory -Path "$root/screenshots","$root/logs"|Out-Null
$report=[ordered]@{case=$Case;status='running';startedAt=[DateTime]::UtcNow.ToString('o');phases=@();cleanup=@{status='failed'};layer='exact CI portable executable / native WebView2; IPC fault injection explicitly recorded'}
$baseline=Get-TestMachineState;$app=$null;$old=@{}
foreach($name in @('AGENTHUB_DATA_DIR','WEBVIEW2_USER_DATA_FOLDER','WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS')){$old[$name]=[Environment]::GetEnvironmentVariable($name,'Process')}
function Close-App {
 if($script:app -and -not $script:app.HasExited){[void]$script:app.CloseMainWindow();if(-not $script:app.WaitForExit(15000)){throw 'Owned packaged app did not close'}}
 $script:app=$null
}
function Launch-App {
 $info=New-Object Diagnostics.ProcessStartInfo($exe)
 $info.UseShellExecute=$false;$info.WorkingDirectory="$root/portable"
 foreach($name in @('AGENTHUB_DATA_DIR','WEBVIEW2_USER_DATA_FOLDER','WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS')){$info.EnvironmentVariables[$name]=[Environment]::GetEnvironmentVariable($name,'Process')}
 $script:app=[Diagnostics.Process]::Start($info)
 $deadline=[DateTime]::UtcNow.AddSeconds(40)
 do{
  if($app.HasExited){throw 'Packaged app exited before endpoint ready'}
  try{
   $listener=Get-NetTCPConnection -LocalPort $port -State Listen -ErrorAction Stop|Select-Object -First 1
   if($listener.LocalAddress -notin @('127.0.0.1','::1')){throw 'Endpoint must be loopback-only'}
   $cursor=[int]$listener.OwningProcess;$seen=@()
   while($cursor -ne $app.Id -and $cursor -gt 0 -and $cursor -notin $seen){$seen+=$cursor;$cursor=[int](Get-CimInstance Win32_Process -Filter "ProcessId=$cursor").ParentProcessId}
   if($cursor -ne $app.Id){throw 'Endpoint not owned by candidate process'}
   $null=Invoke-RestMethod "http://127.0.0.1:$port/json/version" -TimeoutSec 2
   return
  }catch{$last=$_.ToString();Start-Sleep -Milliseconds 250}
 }while([DateTime]::UtcNow -lt $deadline)
 Write-AcceptanceJson @(Get-CimInstance Win32_Process|Where-Object {$_.ProcessId -eq $app.Id -or ($_.Name -eq 'msedgewebview2.exe' -and $_.CommandLine -and $_.CommandLine.Contains($root))}|Select-Object Name,ProcessId,ParentProcessId,CommandLine) "$root/endpoint-processes.json"
 throw "Packaged endpoint unavailable: $last"
}
function Run-Phase([string]$Phase){
 $code=Invoke-AcceptanceProcess $Node @("$PSScriptRoot/desktop-check.mjs","http://127.0.0.1:$port",$env:AGENTHUB_DATA_DIR,$root,$Phase,'http://tauri.localhost') "$root/logs/$Phase.log"
 if($code -ne 0){throw "Packaged UI phase failed: $Phase (exit $code)"}
 $report.phases+=@{name=$Phase;status='passed'}
}
try{
 Expand-Archive -LiteralPath $PortableZip -DestinationPath "$root/portable"
 $exe="$root/portable/AgentHub.exe"
 if((Get-FileHash -LiteralPath $exe).Hash -ne $DesktopSha256){throw 'Packaged executable differs from candidate manifest'}
 $report.exeSha256=$DesktopSha256;$report.portableSha256=(Get-FileHash $PortableZip).Hash
 $fixture="$root/fixture"
 $code=Invoke-AcceptanceProcess 'powershell.exe' @('-NoProfile','-File',"$PSScriptRoot/desktop-fixture.ps1",'-Cli',$Cli,'-CaseRoot',$fixture) "$root/logs/fixture.log"
 if($code -ne 0){throw 'Real Rust fixture creation failed'}
 $reservation=New-Object Net.Sockets.TcpListener([Net.IPAddress]::Loopback,0);$reservation.Start();$port=$reservation.LocalEndpoint.Port;$reservation.Stop()
 $env:AGENTHUB_DATA_DIR="$fixture/data";$env:WEBVIEW2_USER_DATA_FOLDER="$fixture/webview"
 $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=$port --remote-debugging-address=127.0.0.1"
 Add-Type @'
using System;using System.Runtime.InteropServices;
public class PackagedWindow {
 [StructLayout(LayoutKind.Sequential)] public struct Rect{public int Left,Top,Right,Bottom;}
 [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr h,out Rect r);
 [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h,out Rect r);
 [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h,IntPtr after,int x,int y,int cx,int cy,uint flags);
}
'@
 Launch-App
 foreach($size in @(@(1280,860),@(860,640))){
  $app.Refresh();$client=New-Object PackagedWindow+Rect;$outer=New-Object PackagedWindow+Rect
  if(-not [PackagedWindow]::GetClientRect($app.MainWindowHandle,[ref]$client) -or -not [PackagedWindow]::GetWindowRect($app.MainWindowHandle,[ref]$outer)){throw 'Window bounds unavailable'}
  if(-not [PackagedWindow]::SetWindowPos($app.MainWindowHandle,[IntPtr]::Zero,0,0,$size[0]+$outer.Right-$outer.Left-$client.Right+$client.Left,$size[1]+$outer.Bottom-$outer.Top-$client.Bottom+$client.Top,6)){throw 'Window resize failed'}
  Run-Phase "layout-$($size[0])x$($size[1])"
 }
 Run-Phase 'favorites'
 Close-App
 Launch-App
 Run-Phase 'restart'
 $report.status='passed'
}catch{$report.status='failed';$report.error=$_.ToString();$report.stack=$_.ScriptStackTrace}
finally{
 $errors=@();try{Close-App}catch{$errors+=$_.ToString()}
 try{
  $deadline=[DateTime]::UtcNow.AddSeconds(10)
  do{$left=@(Get-CimInstance Win32_Process -Filter "Name='msedgewebview2.exe'"|Where-Object {$_.CommandLine -and $_.CommandLine.Contains($root)});if(-not $left.Count){break};Start-Sleep -Milliseconds 250}while([DateTime]::UtcNow -lt $deadline)
  if($left.Count){throw 'Owned packaged WebView processes remain'}
  $after=Get-TestMachineState
  if(($baseline|ConvertTo-Json -Depth 30 -Compress) -ne ($after|ConvertTo-Json -Depth 30 -Compress)){throw 'Packaged UI changed VM baseline'}
 }catch{$errors+=$_.ToString()}
 foreach($name in $old.Keys){[Environment]::SetEnvironmentVariable($name,$old[$name],'Process')}
 $report.cleanup=@{status=$(if($errors.Count){'failed'}else{'passed'});errors=$errors}
 if($errors.Count){$report.status='failed'}
 $report.finishedAt=[DateTime]::UtcNow.ToString('o')
 Write-AcceptanceJson $report "$root/report.json"
 Write-AcceptanceJson $report (Join-Path 'C:\AgentHub-VM-Test\evidence' ($Case+'.json'))
}
if($report.status -ne 'passed'){exit 1}
