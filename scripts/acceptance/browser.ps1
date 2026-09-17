param([Parameter(Mandatory=$true)][string]$CandidatePath,[Parameter(Mandatory=$true)][string]$UrlFile,[Parameter(Mandatory=$true)][string]$Case)
# Real browser download. Security prompts remain enabled and are recorded.
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'vm-safety.ps1')
Assert-TestDesktop
Assert-CleanTestInstallation
if($Case -notmatch '^[a-z0-9-]+$'){throw 'Invalid case'}
$candidate=Get-CandidateProof (Assert-AcceptancePath $CandidatePath 'C:\AgentHub-VM-Test')
$urlPath=Assert-AcceptancePath $UrlFile 'C:\AgentHub-VM-Test'
$url=[IO.File]::ReadAllText($urlPath).Trim()
$uri=[uri]$url
if($uri.Scheme -ne 'https' -or $uri.Host -notmatch '(^|\.)(githubusercontent\.com|blob\.core\.windows\.net)$'){throw 'Expected GitHub artifact download URL'}
$root=Assert-AcceptancePath (Join-Path 'C:\AgentHub-VM-Test\cases' $Case) 'C:\AgentHub-VM-Test\cases'
if(Test-Path $root){throw 'Preserve existing browser evidence'}
New-Item -ItemType Directory -Path "$root/profile/Default","$root/downloads","$root/screenshots"|Out-Null
$before=Get-TestMachineState;$edge=$null;$app=$null;$launcher=$null;$ownsInstall=$false
$installRoot=Join-Path $env:LOCALAPPDATA 'AgentHub'
$oldProfile=$env:WEBVIEW2_USER_DATA_FOLDER
$oldArguments=$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS
$report=[ordered]@{case=$Case;status='running';startedAt=[DateTime]::UtcNow.ToString('o');sourceCommit=$candidate.sourceCommit;ciRunId=$candidate.ciRunId;method='Real Edge download and Windows Shell extraction/launch; no security controls disabled';prompts=@();cleanup=@{status='failed'}}
Add-Type -AssemblyName UIAutomationClient,UIAutomationTypes,System.Drawing
Add-Type @'
using System;using System.Runtime.InteropServices;
public class BrowserCapture {
 [StructLayout(LayoutKind.Sequential)]public struct Rect{public int Left,Top,Right,Bottom;}
 [DllImport("user32.dll")]public static extern bool GetWindowRect(IntPtr h,out Rect r);
 [DllImport("user32.dll")]public static extern bool PrintWindow(IntPtr h,IntPtr dc,uint f);
 [DllImport("user32.dll")]public static extern bool PostMessage(IntPtr h,uint m,IntPtr w,IntPtr l);
}
'@
function Elements($Window){@($Window.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.Condition]::TrueCondition))}
function Capture($Window,[string]$Label){
 $h=[IntPtr]$Window.Current.NativeWindowHandle;$r=New-Object BrowserCapture+Rect
 if(-not [BrowserCapture]::GetWindowRect($h,[ref]$r)){throw 'No window bounds'}
 $b=New-Object Drawing.Bitmap(($r.Right-$r.Left),($r.Bottom-$r.Top));$g=[Drawing.Graphics]::FromImage($b);$dc=$g.GetHdc()
 try{$ok=[BrowserCapture]::PrintWindow($h,$dc,2)}finally{$g.ReleaseHdc($dc);$g.Dispose()}
 try{if(-not $ok){throw 'Screenshot failed'};$b.Save("$root/screenshots/$Label.png")}finally{$b.Dispose()}
}
function Zone([string]$Path){@(Get-Content -LiteralPath $Path -Stream Zone.Identifier -ErrorAction SilentlyContinue|Where-Object {$_ -match '^ZoneId='}|ForEach-Object {[string]$_})}
function Wait-InternetZone([string]$Path){
 $deadline=[DateTime]::UtcNow.AddSeconds(30)
 do{if(@(Zone $Path) -contains 'ZoneId=3'){return};Start-Sleep -Milliseconds 250}while([DateTime]::UtcNow -lt $deadline)
 throw 'Internet download marking not complete; do not execute an unmarked file as security-prompt evidence'
}
function Extract-Shell([string]$Zip,[string]$Destination,[string]$LastFile){
 $Zip=[IO.Path]::GetFullPath($Zip);$Destination=[IO.Path]::GetFullPath($Destination)
 New-Item -ItemType Directory -Path $Destination|Out-Null
 $shell=New-Object -ComObject Shell.Application
 $source=$shell.NameSpace($Zip);$target=$shell.NameSpace($Destination)
 if(-not $source -or -not $target){throw 'Shell compressed-folder support unavailable'}
 $target.CopyHere($source.Items(),0)
 Add-Type -AssemblyName System.IO.Compression.FileSystem
 $archive=[IO.Compression.ZipFile]::OpenRead($Zip)
 try{$expected=@($archive.Entries|Where-Object {$_.Name}|ForEach-Object {@{path=(Assert-AcceptancePath (Join-Path $Destination $_.FullName) $Destination);length=$_.Length}})}finally{$archive.Dispose()}
 $deadline=[DateTime]::UtcNow.AddSeconds(60)
 do{
  $pending=@($expected|Where-Object {-not(Test-Path -LiteralPath $_.path) -or (Get-Item -LiteralPath $_.path).Length -ne $_.length})
  if(-not $pending.Count -and (Test-Path (Join-Path $Destination $LastFile))){return}
  Start-Sleep -Milliseconds 250
 }while([DateTime]::UtcNow -lt $deadline)
 throw 'Shell extraction incomplete'
}
function Start-Known([string]$Exe,[string]$Hash,[string]$Arguments,[string]$Label){
 $Exe=[IO.Path]::GetFullPath($Exe)
 if((Get-FileHash -LiteralPath $Exe).Hash -ne $Hash){throw 'Unknown downloaded executable'}
 $job="$root/$Label-launch.json";$exitFile="$root/$Label-exit.json"
 Write-AcceptanceJson @{exe=$Exe;arguments=$Arguments;exitFile=$exitFile} $job
 # Shell launch can block on the publisher prompt; keep the observer responsive.
 $code='$ErrorActionPreference="Stop";$j=[IO.File]::ReadAllText('''+$job+''')|ConvertFrom-Json;try{$p=Start-Process -FilePath $j.exe -ArgumentList $j.arguments -WorkingDirectory (Split-Path $j.exe -Parent) -PassThru;$p.WaitForExit();$code=$p.ExitCode}catch{$code=1};[IO.File]::WriteAllText($j.exitFile,($code|ConvertTo-Json));exit $code'
 $encoded=[Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($code))
 $script:launcher=Start-Process powershell.exe -ArgumentList @('-NoProfile','-WindowStyle','Hidden','-EncodedCommand',$encoded) -WindowStyle Hidden -PassThru
 $deadline=[DateTime]::UtcNow.AddSeconds(90)
 do{
  $windows=@([Windows.Automation.AutomationElement]::RootElement.FindAll([Windows.Automation.TreeScope]::Children,[Windows.Automation.Condition]::TrueCondition))
  foreach($w in $windows){
   # This launcher only starts the verified executable. Windows may abbreviate
   # the displayed path, so require the exact owned process and native dialog.
   if($w.Current.ProcessId -ne $script:launcher.Id -or $w.Current.ClassName -ne '#32770'){continue}
   $els=Elements $w;$names=@($els|ForEach-Object {$_.Current.Name})
   $script:report.observedPublisher=@{processId=$w.Current.ProcessId;launcherId=$script:launcher.Id;text=$names;controls=@($els|ForEach-Object {@{id=$_.Current.AutomationId;name=$_.Current.Name}})}
   if(-not (($names -join ' ').Contains([IO.Path]::GetFileName($Exe)))){continue}
   $buttons=@($els|Where-Object {$_.Current.AutomationId -eq '4426' -and $_.Current.IsEnabled})
   if($buttons.Count -ne 1){continue}
   if((Get-FileHash -LiteralPath $Exe).Hash -ne $Hash){throw 'File changed before publisher confirmation'}
   Capture $w "$Label-publisher"
   $script:report.prompts+=@{stage=$Label;text=$names;file=$Exe;sha256=$Hash;launcherId=$script:launcher.Id;windowProcessId=$w.Current.ProcessId;action='Approved observed Run control owned by this exact-file launcher; zone retained'}
   if(-not [BrowserCapture]::PostMessage([IntPtr]$buttons[0].Current.NativeWindowHandle,245,[IntPtr]::Zero,[IntPtr]::Zero)){throw 'Cannot click observed Run control'}
  }
  if(Test-Path $exitFile){if([int]([IO.File]::ReadAllText($exitFile)) -ne 0){throw 'Downloaded launch failed or was cancelled'};if($Label -eq 'installer'){return}}
  if($Label -eq 'portable'){
   $owned=@(Get-CimInstance Win32_Process -Filter "Name='AgentHub.exe'"|Where-Object {$_.ExecutablePath -eq $Exe})
   if($owned.Count -eq 1){return Get-Process -Id $owned[0].ProcessId}
  }
  Start-Sleep -Milliseconds 300
 }while([DateTime]::UtcNow -lt $deadline)
 throw 'Downloaded launch blocked or timed out; security settings unchanged'
}
function Assert-Rendered($Process,[string]$Label){
 $deadline=[DateTime]::UtcNow.AddSeconds(40)
 do{
  $Process.Refresh();if($Process.HasExited){throw 'App exited before rendering'}
  if($Process.MainWindowHandle -ne [IntPtr]::Zero){
   $window=[Windows.Automation.AutomationElement]::FromHandle($Process.MainWindowHandle)
   $names=@(Elements $window|ForEach-Object {$_.Current.Name})
   if($names -contains 'Prompts'){Capture $window $Label;return}
  }
  Start-Sleep -Milliseconds 300
 }while([DateTime]::UtcNow -lt $deadline)
 throw 'Downloaded app did not render navigation'
}
function Close-App {
 if($script:app -and -not $script:app.HasExited){
  try{[void]$script:app.CloseMainWindow()}catch{if(-not $script:app.HasExited){throw}}
  if(-not $script:app.WaitForExit(15000)){throw 'Owned browser-test app did not close'}
 }
 $script:app=$null
}
function Close-Browser {
 # Edge can retain a background process after its window closes. Only this
 # fresh profile and descendants of its verified Edge roots belong to the run.
 $profile=[IO.Path]::GetFullPath("$root/profile")
 $all=@(Get-CimInstance Win32_Process -Filter "Name='msedge.exe'")
 $owned=@($all|Where-Object {$_.ExecutablePath -eq $edgePath -and $_.CommandLine -and $_.CommandLine.Contains('--user-data-dir="'+$profile+'"')})
 $ids=@($owned.ProcessId)
 do{$children=@($all|Where-Object {$_.ParentProcessId -in $ids -and $_.ProcessId -notin $ids});$owned+=$children;$ids+=@($children.ProcessId)}while($children.Count)
 foreach($item in $owned){
  $p=Get-Process -Id $item.ProcessId -ErrorAction SilentlyContinue
  if($p){
   try{if(-not $p.HasExited -and $p.MainWindowHandle -ne [IntPtr]::Zero){[void]$p.CloseMainWindow()}}
   catch{if(-not $p.HasExited){throw}}
  }
 }
 $deadline=[DateTime]::UtcNow.AddSeconds(8)
 do{$left=@($owned|Where-Object {Get-Process -Id $_.ProcessId -ErrorAction SilentlyContinue});if(-not $left.Count){break};Start-Sleep -Milliseconds 250}while([DateTime]::UtcNow -lt $deadline)
 $closed=@()
 foreach($item in $left){
  $current=Get-CimInstance Win32_Process -Filter "ProcessId=$($item.ProcessId)"
  if(-not $current){continue}
  if($current.CreationDate -ne $item.CreationDate -or $current.Name -ne 'msedge.exe' -or $current.ExecutablePath -ne $edgePath){throw 'Browser process ownership changed'}
  Stop-Process -Id $item.ProcessId -Force;$closed+=$item.ProcessId
 }
 $script:report.closedOwnedBackgroundBrowser=$closed
 if(@(Get-CimInstance Win32_Process -Filter "Name='msedge.exe'"|Where-Object {$_.CommandLine -and $_.CommandLine.Contains($profile)}).Count){throw 'Owned browser profile still active'}
}
try{
 Write-AcceptanceJson @{download=@{default_directory="$root\downloads";prompt_for_download=$false}} "$root/profile/Default/Preferences"
 $edgePath='C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe'
 $report.browserVersion=(Get-Item $edgePath).VersionInfo.ProductVersion
 $edge=Start-Process $edgePath -ArgumentList @('--no-first-run',('--user-data-dir="'+$root+'\profile"'),'--new-window',$url) -WindowStyle Normal -PassThru
 $deadline=[DateTime]::UtcNow.AddSeconds(120);$zip=@()
 do{$zip=@(Get-ChildItem "$root/downloads" -Filter *.zip);if($zip.Count -eq 1 -and -not(Get-ChildItem "$root/downloads" -Filter *.crdownload)){break};Start-Sleep -Milliseconds 500}while([DateTime]::UtcNow -lt $deadline)
 if($zip.Count -ne 1){throw 'Browser download not completed; review Edge download/security UI'}
 Wait-InternetZone $zip[0].FullName
 $report.download=@{sha256=(Get-FileHash $zip[0].FullName).Hash;zone=@(Zone $zip[0].FullName);source='GitHub Actions artifact HTTPS URL (signed query omitted)'}
 # Show only the browser downloads UI; signed URLs are never written to reports.
 $shell=New-Object -ComObject WScript.Shell
 if($shell.AppActivate($edge.Id)){
  $shell.SendKeys('^l');$shell.SendKeys('edge://downloads/all');$shell.SendKeys('{ENTER}')
  $deadline=[DateTime]::UtcNow.AddSeconds(15)
  do{
   $edge.Refresh();$window=[Windows.Automation.AutomationElement]::FromHandle($edge.MainWindowHandle)
   $names=@(Elements $window|ForEach-Object {$_.Current.Name})
   if(($names -join ' ').Contains($zip[0].Name)){Capture $window 'edge-downloads';break}
   Start-Sleep -Milliseconds 300
  }while([DateTime]::UtcNow -lt $deadline)
  if(-not ($names -join ' ').Contains($zip[0].Name)){throw 'Completed download was not visible in Edge downloads UI'}
 }else{throw 'Cannot focus the owned browser'}
 $downloaded="$root/candidate-shell";Extract-Shell $zip[0].FullName $downloaded 'SHA256SUMS.txt'
 $actual=Get-CandidateProof $downloaded
 if(($actual|ConvertTo-Json -Depth 20 -Compress) -ne ($candidate|ConvertTo-Json -Depth 20 -Compress)){throw 'Browser bytes differ from frozen candidate'}
 $report.files=@($actual.files|ForEach-Object {@{name=$_.name;sha256=$_.sha256;zone=@(Zone (Join-Path $downloaded $_.name))}})
 $env:WEBVIEW2_USER_DATA_FOLDER="$root/webview";$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS='--force-renderer-accessibility'
 $installer=@($actual.files|Where-Object {$_.name -like '*setup.exe'})[0]
 Wait-InternetZone (Join-Path $downloaded $installer.name)
 Write-AcceptanceJson @{installRoot=$installRoot;caseRoot=$root} "$root/installation-owner.json";$ownsInstall=$true
 Start-Known (Join-Path $downloaded $installer.name) $installer.sha256 '/S' 'installer'
 $app=Start-Process "$installRoot/AgentHub.exe" -WorkingDirectory $installRoot -WindowStyle Normal -PassThru
 Assert-Rendered $app 'downloaded-installed';Close-App
 Remove-OwnedTestInstallation $installRoot $root;$ownsInstall=$false
 $portable=@($actual.files|Where-Object {$_.name -like '*.zip'})[0]
 Extract-Shell (Join-Path $downloaded $portable.name) "$root/portable-shell" 'AgentHub.exe'
 $manifest=[IO.File]::ReadAllText("$downloaded/build-manifest.json")|ConvertFrom-Json
 Wait-InternetZone "$root/portable-shell/AgentHub.exe"
 $report.portableExeZone=@(Zone "$root/portable-shell/AgentHub.exe")
 $app=Start-Known "$root/portable-shell/AgentHub.exe" $manifest.desktop.sha256 ' ' 'portable'
 Assert-Rendered $app 'downloaded-portable';Close-App
 $report.status='passed'
}catch{$report.status='failed';$report.error=$_.ToString();$report.stack=$_.ScriptStackTrace}
finally{
 $errors=@();try{Close-App}catch{$errors+=$_.ToString()}
 if($ownsInstall){try{Remove-OwnedTestInstallation $installRoot $root}catch{$errors+=$_.ToString()}}
 if($launcher -and -not $launcher.HasExited){if(-not $launcher.WaitForExit(5000)){$errors+='Shell launcher remains; preserve for recovery'}}
 try{if($edge){Close-Browser}}catch{$errors+=$_.ToString()}
 $env:WEBVIEW2_USER_DATA_FOLDER=$oldProfile;$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=$oldArguments
 $after=Get-TestMachineState
 if(($before|ConvertTo-Json -Depth 30 -Compress) -ne ($after|ConvertTo-Json -Depth 30 -Compress)){$errors+='VM baseline changed'}
 $report.cleanup=@{status=$(if($errors.Count){'failed'}else{'passed'});errors=$errors}
 if($errors.Count){$report.status='failed'}
 $report.finishedAt=[DateTime]::UtcNow.ToString('o')
 Write-AcceptanceJson $report "$root/report.json"
 Write-AcceptanceJson $report (Join-Path 'C:\AgentHub-VM-Test\evidence' ($Case+'.json'))
}
if($report.status -ne 'passed'){exit 1}
