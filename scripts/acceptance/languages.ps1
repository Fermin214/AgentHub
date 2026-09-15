param([Parameter(Mandatory=$true)][string]$Installer,[Parameter(Mandatory=$true)][string]$Case)
$ErrorActionPreference='Stop'
if($env:COMPUTERNAME -ne 'TEST' -or $env:USERNAME -ne 'Try' -or [Diagnostics.Process]::GetCurrentProcess().SessionId -eq 0){throw 'Requires TEST/Try interactive test desktop'}
. (Join-Path $PSScriptRoot 'vm-safety.ps1')
Assert-TestDesktop
Assert-CleanTestInstallation
if($Case -notmatch '^[a-z0-9-]+$'){throw 'Invalid case name'}
$root=Assert-AcceptancePath (Join-Path 'C:\AgentHub-VM-Test\cases' $Case) 'C:\AgentHub-VM-Test\cases'
$candidateVersion=(Get-Item -LiteralPath $Installer).VersionInfo.ProductVersion
$parsed=[version]$candidateVersion
$olderVersion='0.0.0'
$newerVersion=([version]::new($parsed.Major+1,0,0)).ToString()
$reg='HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\AgentHub'
$remembered='HKCU:\Software\agenthub\AgentHub'
if((Test-Path $root) -or (Test-Path $reg) -or (Test-Path $remembered) -or (Test-Path -LiteralPath (Join-Path 'C:\AgentHub-VM-Test\evidence' ($Case+'.json')))){throw 'Expected clean language fixture and installation state'}
New-Item -ItemType Directory -Path $root | Out-Null
$report=[ordered]@{case=$Case;status='running';startedAt=[DateTime]::UtcNow.ToString('o');installerSha256=(Get-FileHash $Installer -Algorithm SHA256).Hash;environment=@{os=[Environment]::OSVersion.VersionString;uiCulture=(Get-UICulture).Name};cases=@()}
$utf8=New-Object Text.UTF8Encoding($false)
function Save { [IO.File]::WriteAllText("$root/report.json",($report|ConvertTo-Json -Depth 20),$utf8) }
Add-Type -AssemblyName UIAutomationClient,UIAutomationTypes,System.Drawing
Add-Type @'
using System; using System.Runtime.InteropServices;
public static class LanguageCapture {
 [StructLayout(LayoutKind.Sequential)] public struct Rect {public int Left,Top,Right,Bottom;}
 [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h,out Rect r);
 [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h,IntPtr dc,uint f);
}
'@
function Windows {
 @([Windows.Automation.AutomationElement]::RootElement.FindAll([Windows.Automation.TreeScope]::Children,[Windows.Automation.Condition]::TrueCondition) | Where-Object {$_.Current.Name -match '^AgentHub\s+(Setup|安装)'})
}
function Elements($Window) { @($Window.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.Condition]::TrueCondition)) }
function Screenshot($Window,[string]$Path) {
 $r=New-Object LanguageCapture+Rect
 $h=[IntPtr]$Window.Current.NativeWindowHandle
 if(-not [LanguageCapture]::GetWindowRect($h,[ref]$r)){throw 'Missing window bounds'}
 $b=New-Object Drawing.Bitmap(($r.Right-$r.Left),($r.Bottom-$r.Top))
 $g=[Drawing.Graphics]::FromImage($b);$dc=$g.GetHdc()
 try{$ok=[LanguageCapture]::PrintWindow($h,$dc,2)}finally{$g.ReleaseHdc($dc);$g.Dispose()}
 try{if(-not $ok){throw 'Window capture failed'};$b.Save($Path,[Drawing.Imaging.ImageFormat]::Png)}finally{$b.Dispose()}
}
$appRoot=Join-Path $root 'app';$exe=Join-Path $appRoot 'AgentHub.exe';$app=$null;$setup=$null
Write-AcceptanceJson @{installRoot=$appRoot;caseRoot=$root} (Join-Path $root 'installation-owner.json')
try {
 $p=Start-Process $Installer -ArgumentList "/S /LANG=1033 /D=$appRoot" -WindowStyle Hidden -PassThru
 if(-not $p.WaitForExit(120000) -or $p.ExitCode -ne 0){throw 'Fixture install failed'}
 [IO.File]::WriteAllText((Join-Path $appRoot 'user-note.txt'),'preserve language fixture',$utf8)
 $sentinel=(Get-FileHash (Join-Path $appRoot 'user-note.txt')).Hash
 $exeHash=(Get-FileHash $exe).Hash
 foreach($lang in @(1033,2052)) {
  foreach($mode in @('repair','upgrade','downgrade','running')) {
   $version=switch($mode){'upgrade'{$olderVersion} 'downgrade'{$newerVersion} default{$candidateVersion}}
   Set-ItemProperty $reg DisplayVersion $version
   if($mode -eq 'running') {
    $app=Start-Process $exe -WorkingDirectory $appRoot -WindowStyle Hidden -PassThru
    for($i=0;$i -lt 40;$i++){Start-Sleep -Milliseconds 250;$app.Refresh();if($app.MainWindowHandle -ne [IntPtr]::Zero){break}}
    if($app.HasExited){throw 'App not running for the running-process check'}
   }
   $expected=if($lang -eq 1033){switch($mode){'repair'{'Repair installation'} 'upgrade'{'Upgrade and keep your data'} 'downgrade'{("A newer version ($newerVersion) is installed.")} 'running'{'Close AgentHub before retrying setup.'}}}else{switch($mode){'repair'{'修复安装'} 'upgrade'{'升级到新版本，保留数据'} 'downgrade'{("已安装较新版本 $newerVersion")} 'running'{'请先关闭 AgentHub'}}}
   $setup=Start-Process $Installer -ArgumentList "/LANG=$lang" -WindowStyle Normal -PassThru
   $found=$false;$names=@();$current=@()
   for($i=0;$i -lt 60;$i++) {
    Start-Sleep -Milliseconds 400
    $current=@(Windows);$all=@($current|ForEach-Object {Elements $_});$names=@($all|ForEach-Object {$_.Current.Name})
    if(($names -join "`n").Contains($expected)){$found=$true;break}
    $next=@($all|Where-Object {$_.Current.ControlType -eq [Windows.Automation.ControlType]::Button -and $_.Current.IsEnabled -and $_.Current.Name -match '^(Next|Install|下一步|安装\()'})
    if($next.Count -eq 1){$next[0].GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern).Invoke();Start-Sleep -Milliseconds 400}
   }
   if(-not $found){throw "Missing $lang/$mode message: $($names -join ' | ')"}
   $report.lastUi=$names;Save
   $contentNames=@($all | Where-Object {$_.Current.AutomationId -match '^\d+$' -and $_.Current.ControlType -eq [Windows.Automation.ControlType]::Text} | ForEach-Object {$_.Current.Name})
   if($lang -eq 1033 -and ($contentNames -join ' ') -match '[\u4e00-\u9fff]'){throw 'Chinese text on English maintenance UI'}
   if($names -match '\$ATb|\$\{VERSION\}|\$\{PRODUCTNAME\}') {throw 'Unexpanded interpolation in installer text'}
   $caseName="$lang-$mode"
   [IO.File]::WriteAllLines("$root/$caseName.txt",[string[]]$names,$utf8)
   for($i=0;$i -lt $current.Count;$i++){Screenshot $current[$i] "$root/$caseName-$i.png"}
   $cancel=@($all|Where-Object {$_.Current.ControlType -eq [Windows.Automation.ControlType]::Button -and $_.Current.IsEnabled -and $_.Current.Name -match '^(Cancel|取消)'})
   if($cancel.Count -eq 1){$cancel[0].GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern).Invoke()}else{
    $ok=@($all|Where-Object {$_.Current.ControlType -eq [Windows.Automation.ControlType]::Button -and $_.Current.Name -match '^(OK|确定)'})
    if($ok.Count -ne 1){throw 'No cancel/acknowledgement button'}
    $ok[0].GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern).Invoke()
   }
   if(-not $setup.WaitForExit(15000)){throw 'Setup did not exit after cancel'}
   $setup=$null
   if($mode -eq 'downgrade') {
    $silent=Start-Process $Installer -ArgumentList "/S /LANG=$lang" -WindowStyle Hidden -PassThru
    if(-not $silent.WaitForExit(15000) -or $silent.ExitCode -ne 2){throw 'Silent downgrade was not rejected with exit 2'}
   }
   if($app){[void]$app.CloseMainWindow();if(-not $app.WaitForExit(15000)){throw 'App did not exit'};$app=$null}
   if((Get-FileHash (Join-Path $appRoot 'user-note.txt')).Hash -ne $sentinel -or (Get-FileHash $exe).Hash -ne $exeHash){throw 'Maintenance cancellation changed data or executable'}
   $report.cases+=@{language=$lang;scenario=$mode;status='passed';expected=$expected;text=$names;dataPreserved=$true};Save
  }
 }
 $report.status='passed'
}catch{$report.status='failed';$report.error=$_.ToString();$report.stack=$_.ScriptStackTrace}
finally {
 $errors=@()
 try {
  if($setup -and -not $setup.HasExited){[void]$setup.CloseMainWindow();if(-not $setup.WaitForExit(5000)){$setup.Kill();$setup.WaitForExit()}}
  if($app -and -not $app.HasExited){[void]$app.CloseMainWindow();if(-not $app.WaitForExit(15000)){$app.Kill();$app.WaitForExit();throw 'App required forced cleanup'}}
 } catch {$errors+=$_.ToString()}
 try {
  if(Test-Path -LiteralPath $reg){
   if((Get-ItemProperty -LiteralPath $reg).InstallLocation.Trim('"') -ne $appRoot){throw 'Registration ownership changed'}
   Set-ItemProperty -LiteralPath $reg -Name DisplayVersion -Value $candidateVersion
  }
  Remove-OwnedTestInstallation $appRoot $root
 } catch {$errors+=$_.ToString()}
 $report.cleanup=@{status=$(if($errors.Count){'failed'}else{'passed'});errors=$errors}
 if($errors.Count){$report.status='failed'}
 $report.finishedAt=[DateTime]::UtcNow.ToString('o')
 Save
 Write-AcceptanceJson $report (Join-Path 'C:\AgentHub-VM-Test\evidence' ($Case+'.json'))
}
if($report.status -ne 'passed'){throw ($report.error+' '+($report.cleanup.errors -join '; '))}
