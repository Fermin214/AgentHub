param([Parameter(Mandatory=$true)][string]$Installer,[Parameter(Mandatory=$true)][string]$PortableZip,[Parameter(Mandatory=$true)][string]$Case,[switch]$AuthorizedRuntimeIsolation)
# Opt-in only. The operator must authorize reversible isolation on TEST/Try.
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'vm-safety.ps1')
if(-not $AuthorizedRuntimeIsolation){throw 'Explicit runtime isolation authorization required'}
Assert-TestDesktop
Assert-CleanTestInstallation
if($Case -notmatch '^[a-z0-9-]+$'){throw 'Invalid case name'}
$root=Assert-AcceptancePath (Join-Path 'C:\AgentHub-VM-Test\cases' $Case) 'C:\AgentHub-VM-Test\cases'
if(Test-Path -LiteralPath $root){throw 'Preserve previous case'}
New-Item -ItemType Directory -Path $root | Out-Null
$report=[ordered]@{case=$Case;status='running';startedAt=[DateTime]::UtcNow.ToString('o');method='Operator-authorized reversible shared runtime isolation in TEST VM; unreachable VM-user proxy is network fault injection, not physical disconnection';missingPassed=$false;recoveryPassed=$false;cleanup=@{status='failed'};steps=@()}
$before=Get-TestMachineState
Write-AcceptanceJson $before "$root/before.json"
$runtimeParent='C:\Program Files (x86)\Microsoft\EdgeWebView\Application'
$runtimes=@(Get-ChildItem -LiteralPath $runtimeParent -Filter msedgewebview2.exe -Recurse)
if($runtimes.Count -ne 1){throw 'Exactly one machine runtime required; preserve other installations'}
$runtime=Assert-AcceptancePath $runtimes[0].DirectoryName $runtimeParent
if((Split-Path $runtime -Leaf) -notmatch '^\d+\.\d+\.\d+\.\d+$'){throw 'Unexpected runtime directory'}
$backup=Assert-AcceptancePath ($runtime+'.'+$Case) $runtimeParent
if(Test-Path -LiteralPath $backup){throw 'Runtime backup exists'}
$clientId='{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}'
$clients=@(@{native="HKLM\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\$clientId";provider="HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\$clientId"},@{native="HKCU\Software\Microsoft\EdgeUpdate\Clients\$clientId";provider="HKCU:\Software\Microsoft\EdgeUpdate\Clients\$clientId"})
$proxyKey='HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet Settings'
$proxySaved=@();$proxyChanged=$false;$registrySaved=@();$moved=$false;$searchPath=$null;$installerProcess=$null;$app=$null;$ownsInstall=$false
$installRoot=Join-Path $env:LOCALAPPDATA 'AgentHub'
$oldArguments=$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS
$oldProfile=$env:WEBVIEW2_USER_DATA_FOLDER
function Save { Write-AcceptanceJson $report "$root/report.json" }
function Step([string]$Text){$report.steps+=$Text;Save}
function Close-Owned($Process){
 if(-not $Process -or $Process.HasExited){return}
 $Process.Refresh()
 $identity=Get-CimInstance Win32_Process -Filter "ProcessId=$($Process.Id)"
 # Tauri's missing-runtime task dialog has no working close-box. Dismiss only
 # its observed acknowledgement, in this run's verified portable executable.
 if($identity.ExecutablePath -eq (Join-Path $root 'portable\AgentHub.exe') -and $Process.MainWindowHandle -ne [IntPtr]::Zero){
  $window=[Windows.Automation.AutomationElement]::FromHandle($Process.MainWindowHandle)
  $elements=@($window.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.Condition]::TrueCondition))
  if(($elements|ForEach-Object {$_.Current.Name}) -match 'Could not find the WebView2 Runtime'){
   $button=@($elements|Where-Object {$_.Current.ControlType -eq [Windows.Automation.ControlType]::Button -and $_.Current.IsEnabled -and $_.Current.Name -in @('OK',([string][char]0x786e+[char]0x5b9a))})
   if($button.Count -ne 1){throw 'Missing-runtime acknowledgement is ambiguous'}
   $button[0].GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern).Invoke()
  }else{[void]$Process.CloseMainWindow()}
 }else{[void]$Process.CloseMainWindow()}
 if(-not $Process.WaitForExit(10000)){throw "Owned process did not close: $($Process.Id)"}
}
function Restore-Proxy {
 if(-not $script:proxyChanged){return}
 foreach($entry in $proxySaved){if($entry.exists){New-ItemProperty -LiteralPath $proxyKey -Name $entry.name -Value $entry.value -PropertyType $entry.kind -Force|Out-Null}else{Remove-ItemProperty -LiteralPath $proxyKey -Name $entry.name -ErrorAction SilentlyContinue}}
 $script:proxyChanged=$false
}
function Stop-RuntimeSearch {
 $processes=@(Get-CimInstance Win32_Process -Filter "Name='msedgewebview2.exe'" | Where-Object {$_.ExecutablePath -and $_.ExecutablePath.StartsWith($runtimeParent+'\',[StringComparison]::OrdinalIgnoreCase)})
 $roots=@($processes|Where-Object {$processes.ProcessId -notcontains $_.ParentProcessId})
 foreach($process in $roots){
  $owner=Get-CimInstance Win32_Process -Filter "ProcessId=$($process.ParentProcessId)"
  if($owner.Name -ne 'SearchHost.exe' -or $owner.ExecutablePath -ne 'C:\WINDOWS\SystemApps\MicrosoftWindows.Client.CBS_cw5n1h2txyewy\SearchHost.exe'){throw 'Runtime used by an unrelated application; preserve it'}
 }
 foreach($id in @($roots.ParentProcessId|Select-Object -Unique)){
  $script:searchPath='C:\WINDOWS\SystemApps\MicrosoftWindows.Client.CBS_cw5n1h2txyewy\SearchHost.exe'
  & taskkill.exe /PID $id /T /F | Out-Null
  if($LASTEXITCODE -ne 0 -and (Get-Process -Id $id -ErrorAction SilentlyContinue)){throw 'Cannot close authorized VM SearchHost tree'}
 }
}
Add-Type -AssemblyName UIAutomationClient,UIAutomationTypes,System.Drawing
Add-Type @'
using System;using System.Runtime.InteropServices;
public class RuntimeCapture {
 [StructLayout(LayoutKind.Sequential)] public struct Rect{public int Left,Top,Right,Bottom;}
 [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h,out Rect r);
 [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h,IntPtr dc,uint f);
}
'@
function Capture($Process,[string]$Name){
 $Process.Refresh();if($Process.HasExited -or $Process.MainWindowHandle -eq [IntPtr]::Zero){return @()}
 $window=[Windows.Automation.AutomationElement]::FromHandle($Process.MainWindowHandle)
 $names=@($window.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.Condition]::TrueCondition)|ForEach-Object {$_.Current.Name})
 Write-AcceptanceJson $names "$root/$Name.json"
 $rect=New-Object RuntimeCapture+Rect
 if(-not [RuntimeCapture]::GetWindowRect($Process.MainWindowHandle,[ref]$rect)){throw 'Window rectangle unavailable'}
 $bitmap=New-Object Drawing.Bitmap(($rect.Right-$rect.Left),($rect.Bottom-$rect.Top));$graphics=[Drawing.Graphics]::FromImage($bitmap);$dc=$graphics.GetHdc()
 try{$ok=[RuntimeCapture]::PrintWindow($Process.MainWindowHandle,$dc,2)}finally{$graphics.ReleaseHdc($dc);$graphics.Dispose()}
 try{if(-not $ok){throw 'Screenshot failed'};$bitmap.Save("$root/$Name.png",[Drawing.Imaging.ImageFormat]::Png)}finally{$bitmap.Dispose()}
 return $names
}
try {
 $report.installerSha256=(Get-FileHash -LiteralPath $Installer).Hash;$report.portableSha256=(Get-FileHash -LiteralPath $PortableZip).Hash
 # Export every original key before any mutation. The recovery journal never contains passwords.
 foreach($client in $clients){
  $entry=@{native=$client.native;provider=$client.provider;exists=(Test-Path -LiteralPath $client.provider);file="$root/client-$($registrySaved.Count).reg"}
  if($entry.exists){& reg.exe export $entry.native $entry.file /y|Out-Null;if($LASTEXITCODE -ne 0){throw 'Registry export failed'}}
  $registrySaved+=$entry
 }
 Write-AcceptanceJson @{runtime=$runtime;backup=$backup;clients=$registrySaved;candidate=$report.installerSha256} "$root/recovery-journal.json"
 Stop-RuntimeSearch
 # Process exit can precede release of runtime image handles. Retry briefly;
 # never remove a file, change ACLs, or stop an unrecognized owner to force it.
 $moveDeadline=[DateTime]::UtcNow.AddSeconds(5)
 do{
  try{Move-Item -LiteralPath $runtime -Destination $backup;$moved=$true;break}
  catch{if([DateTime]::UtcNow -ge $moveDeadline){throw};Start-Sleep -Milliseconds 250;Stop-RuntimeSearch}
 }while(-not $moved)
 foreach($entry in $registrySaved){if($entry.exists){Remove-Item -LiteralPath $entry.provider -Recurse}}
 if((Test-Path -LiteralPath $runtime) -or @($clients|Where-Object {Test-Path -LiteralPath $_.provider}).Count){throw 'Runtime isolation incomplete'}
 $portable=Join-Path $root 'portable';Expand-Archive -LiteralPath $PortableZip -DestinationPath $portable
 $env:WEBVIEW2_USER_DATA_FOLDER=Join-Path $root 'webview'
 $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS='--force-renderer-accessibility'
 $app=Start-Process -FilePath "$portable/AgentHub.exe" -WorkingDirectory $portable -WindowStyle Hidden -PassThru
 [void]$app.WaitForExit(7000)
 $report.portableMissing=@{exited=$app.HasExited;exitCode=$(if($app.HasExited){$app.ExitCode}else{$null});ui=@(Capture $app 'portable-missing')}
 if($report.portableMissing.ui -contains 'Prompts'){throw 'Portable unexpectedly rendered without the isolated runtime'}
 if(-not $report.portableMissing.exited -and -not $report.portableMissing.ui.Count){throw 'Portable missing-runtime behavior was not observable'}
 Close-Owned $app;$app=$null
 Step 'Actual runtime path and detection registration isolated; recorded portable startup outcome'
 $key=Get-Item -LiteralPath $proxyKey
 foreach($name in @('ProxyEnable','ProxyServer','ProxyOverride','AutoConfigURL','AutoDetect')){$exists=$key.GetValueNames() -contains $name;$proxySaved+=@{name=$name;exists=$exists;value=$(if($exists){$key.GetValue($name)}else{$null});kind=$(if($exists){$key.GetValueKind($name).ToString()}else{$null})}}
 Write-AcceptanceJson $proxySaved "$root/proxy-recovery.json"
 if(Get-NetTCPConnection -LocalPort 9 -State Listen -ErrorAction SilentlyContinue){throw 'Fault injection proxy port occupied'}
 $proxyChanged=$true
 New-ItemProperty -LiteralPath $proxyKey -Name ProxyEnable -Value 1 -PropertyType DWord -Force|Out-Null
 New-ItemProperty -LiteralPath $proxyKey -Name ProxyServer -Value '127.0.0.1:9' -PropertyType String -Force|Out-Null
 New-ItemProperty -LiteralPath $proxyKey -Name ProxyOverride -Value '' -PropertyType String -Force|Out-Null
 New-ItemProperty -LiteralPath $proxyKey -Name AutoDetect -Value 0 -PropertyType DWord -Force|Out-Null
 Remove-ItemProperty -LiteralPath $proxyKey -Name AutoConfigURL -ErrorAction SilentlyContinue
 Write-AcceptanceJson @{installRoot=$installRoot;caseRoot=$root} "$root/installation-owner.json";$ownsInstall=$true
 $installerProcess=Start-Process -FilePath $Installer -ArgumentList '/LANG=1033' -WindowStyle Normal -PassThru
 $deadline=[DateTime]::UtcNow.AddSeconds(120);$failure=$false
 do {
  Start-Sleep -Milliseconds 500;$installerProcess.Refresh()
  if($installerProcess.HasExited){break};if($installerProcess.MainWindowHandle -eq [IntPtr]::Zero){continue}
  $window=[Windows.Automation.AutomationElement]::FromHandle($installerProcess.MainWindowHandle)
  $elements=@($window.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.Condition]::TrueCondition))
  $text=($elements|ForEach-Object {$_.Current.Name}) -join ' '
  if($text -match '(?i)download.*(failed|error)|error.*download|installation aborted'){$failure=$true;break}
  foreach($button in @($elements|Where-Object {$_.Current.ControlType -eq [Windows.Automation.ControlType]::Button -and $_.Current.IsEnabled -and $_.Current.Name -match '^(Next|Install|I Agree|&Next|&Install)'})){$button.GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern).Invoke();break}
 }while([DateTime]::UtcNow -lt $deadline)
 $report.downloadFailureUi=@(Capture $installerProcess 'download-failure')
 if(-not $failure){throw 'Did not capture explicit runtime download failure'}
 if(Test-Path 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\AgentHub'){throw 'Offline installation incorrectly registered success'}
 $report.missingPassed=$true
 Close-Owned $installerProcess;$installerProcess=$null
 Restore-Proxy
 Step 'Unreachable VM-user proxy produced explicit download failure; original proxy restored before online retry'
 $installerProcess=Start-Process -FilePath $Installer -ArgumentList '/S' -WindowStyle Hidden -PassThru
 if(-not $installerProcess.WaitForExit(240000) -or $installerProcess.ExitCode -ne 0){throw 'Online installer retry failed or timed out'}
 $installerProcess=$null
 $registration=Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\AgentHub'
 if($registration.InstallLocation.Trim('"') -ne $installRoot){throw 'Unexpected installation path'}
 $app=Start-Process -FilePath "$installRoot/AgentHub.exe" -WorkingDirectory $installRoot -WindowStyle Hidden -PassThru
 $deadline=[DateTime]::UtcNow.AddSeconds(40);$names=@()
 do{Start-Sleep -Milliseconds 500;$names=@(Capture $app 'online-startup');if($names -contains 'Prompts'){break}}while([DateTime]::UtcNow -lt $deadline)
 if($names -notcontains 'Prompts'){throw 'App did not render after online recovery'}
 $report.recoveryPassed=$true;$report.status='passed'
}catch{$report.status='failed';$report.error=$_.ToString();$report.stack=$_.ScriptStackTrace}
finally {
 $errors=@()
 try{Restore-Proxy}catch{$errors+=$_.ToString()}
 foreach($p in @($app,$installerProcess)){try{Close-Owned $p}catch{$errors+=$_.ToString()}}
 if($ownsInstall){try{Remove-OwnedTestInstallation $installRoot $root}catch{$errors+=$_.ToString()}}
 if($moved){
  try{
   Stop-RuntimeSearch
   # Retain downloaded runtime bytes as owned evidence; never delete a shared runtime.
   $created=@(Get-ChildItem -LiteralPath $runtimeParent -Directory|Where-Object {$_.Name -match '^\d+\.\d+\.\d+\.\d+$'})
   foreach($item in $created){$source=Assert-AcceptancePath $item.FullName $runtimeParent;$dest=Assert-AcceptancePath (Join-Path $root ('downloaded-runtime-'+$item.Name)) $root;if(Test-Path $dest){throw 'Recovery evidence exists'};Move-Item -LiteralPath $source -Destination $dest}
   Move-Item -LiteralPath (Assert-AcceptancePath $backup $runtimeParent) -Destination (Assert-AcceptancePath $runtime $runtimeParent)
   foreach($entry in $registrySaved){if(Test-Path $entry.provider){Remove-Item -LiteralPath $entry.provider -Recurse};if($entry.exists){& reg.exe import $entry.file|Out-Null;if($LASTEXITCODE -ne 0){throw 'Original registration restore failed'}}}
  }catch{$errors+=$_.ToString()}
 }
 if($searchPath){try{if(-not(Get-Process SearchHost -ErrorAction SilentlyContinue)){Start-Process -FilePath $searchPath -WindowStyle Hidden|Out-Null}}catch{$errors+=$_.ToString()}}
 $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=$oldArguments;$env:WEBVIEW2_USER_DATA_FOLDER=$oldProfile
 try{$after=Get-TestMachineState;Write-AcceptanceJson $after "$root/after.json";if(($before|ConvertTo-Json -Depth 30 -Compress) -ne ($after|ConvertTo-Json -Depth 30 -Compress)){throw 'Critical VM state differs after runtime restoration'}}catch{$errors+=$_.ToString()}
 $report.cleanup=@{status=$(if($errors.Count){'failed'}else{'passed'});errors=$errors}
 if($errors.Count){$report.status='failed'}
 $report.finishedAt=[DateTime]::UtcNow.ToString('o');Save
 Write-AcceptanceJson $report (Join-Path 'C:\AgentHub-VM-Test\evidence' ($Case+'.json'))
}
if($report.status -ne 'passed'){exit 1}
