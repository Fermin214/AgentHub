param([Parameter(Mandatory=$true)][string]$Executable,[Parameter(Mandatory=$true)][string]$Sha256,[Parameter(Mandatory=$true)][ValidateSet('zh','en')][string]$Language,[Parameter(Mandatory=$true)][string]$Case)
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'vm-safety.ps1')
Assert-TestDesktop
Assert-CleanTestInstallation
if($Case -notmatch '^[a-z0-9-]+$'){throw 'Invalid case'}
$exe=Assert-AcceptancePath $Executable 'C:\AgentHub-VM-Test'
if((Get-FileHash -LiteralPath $exe).Hash -ne $Sha256){throw 'Executable hash mismatch'}
$root=Assert-AcceptancePath (Join-Path 'C:\AgentHub-VM-Test\cases' $Case) 'C:\AgentHub-VM-Test\cases'
if(Test-Path $root){throw 'Preserve previous evidence'}
New-Item -ItemType Directory -Path "$root/empty-runtime"|Out-Null
$report=[ordered]@{case=$Case;status='running';sha256=$Sha256;language=$Language;method='Process-local empty fixed-runtime override; actual native startup error, not physical machine-runtime removal';cleanup=@{status='failed'}}
$before=Get-TestMachineState;$p=$null;$old=@{}
foreach($key in @('WEBVIEW2_BROWSER_EXECUTABLE_FOLDER','WEBVIEW2_USER_DATA_FOLDER','AGENTHUB_DATA_DIR')){$old[$key]=[Environment]::GetEnvironmentVariable($key,'Process')}
Add-Type -AssemblyName UIAutomationClient,UIAutomationTypes,System.Drawing
Add-Type @'
using System;using System.Runtime.InteropServices;
public class StartupDialogCapture {
 [StructLayout(LayoutKind.Sequential)]public struct Rect{public int Left,Top,Right,Bottom;}
 [DllImport("user32.dll")]public static extern bool GetWindowRect(IntPtr h,out Rect r);
 [DllImport("user32.dll")]public static extern bool PrintWindow(IntPtr h,IntPtr dc,uint f);
 [DllImport("user32.dll")]public static extern bool PostMessage(IntPtr h,uint m,IntPtr w,IntPtr l);
}
'@
$handle=[IntPtr]::Zero;$buttonId=0
try{
 $admin=([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
 if($admin){throw 'Run the probe in the ordinary interactive user token; elevated WebView2 ignores overrides'}
 $env:WEBVIEW2_BROWSER_EXECUTABLE_FOLDER="$root/empty-runtime"
 $env:WEBVIEW2_USER_DATA_FOLDER="$root/webview";$env:AGENTHUB_DATA_DIR="$root/data"
 $p=Start-Process -FilePath $exe -WorkingDirectory (Split-Path $exe -Parent) -WindowStyle Normal -PassThru
 $deadline=[DateTime]::UtcNow.AddSeconds(20);$names=@();$elements=@()
 do{
  $p.Refresh();if($p.HasExited){throw 'Exited without an observable dependency dialog'}
  if($p.MainWindowHandle -ne [IntPtr]::Zero){
   $window=[Windows.Automation.AutomationElement]::FromHandle($p.MainWindowHandle)
   $elements=@($window.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.Condition]::TrueCondition))
   $names=@($elements|ForEach-Object {$_.Current.Name})
   if(($names -join ' ') -match 'WebView2'){$handle=$p.MainWindowHandle;break}
  }
  Start-Sleep -Milliseconds 200
 }while([DateTime]::UtcNow -lt $deadline)
 if($handle -eq [IntPtr]::Zero){throw 'Dependency dialog was not observed'}
 $report.ui=$names
 $rect=New-Object StartupDialogCapture+Rect
 if(-not [StartupDialogCapture]::GetWindowRect($handle,[ref]$rect)){throw 'No dialog bounds'}
 $bmp=New-Object Drawing.Bitmap(($rect.Right-$rect.Left),($rect.Bottom-$rect.Top));$graphics=[Drawing.Graphics]::FromImage($bmp);$dc=$graphics.GetHdc()
 try{$captured=[StartupDialogCapture]::PrintWindow($handle,$dc,2)}finally{$graphics.ReleaseHdc($dc);$graphics.Dispose()}
 try{if(-not $captured){throw 'No screenshot'};$bmp.Save("$root/dialog.png")}finally{$bmp.Dispose()}
 foreach($element in $elements){if($element.Current.AutomationId -match '^CommandButton_(1|100)$'){$buttonId=[int]$Matches[1]}}
 $expectedBody=if($Language -eq 'zh'){-join [char[]]@(0x672a,0x627e,0x5230)}else{'The WebView2 Runtime was not found.'}
 $expectedButton=if($Language -eq 'zh'){-join [char[]]@(0x786e,0x5b9a)}else{'OK'}
 if(-not ($names -join ' ').Contains($expectedBody) -or $names -notcontains $expectedButton){throw 'STARTUP_DIALOG_LANGUAGE_MISMATCH'}
 if($buttonId -ne 100){throw 'Expected the explicitly localized acknowledgement'}
 if(-not [StartupDialogCapture]::PostMessage($handle,1126,[IntPtr]$buttonId,[IntPtr]::Zero)){throw 'Cannot acknowledge dialog'}
 if(-not $p.WaitForExit(10000)){throw 'Acknowledgement left a startup process'}
 $report.exitCode=$p.ExitCode
 if($p.ExitCode -ne 1){throw 'Startup failure must exit 1'}
 if(Test-Path "$root/data"){throw 'Dependency error created application data'}
 $report.status='passed'
}catch{$report.status='failed';$report.error=$_.ToString()}
finally{
 $errors=@()
 if($p -and -not $p.HasExited){
  if($handle -ne [IntPtr]::Zero -and $buttonId){[void][StartupDialogCapture]::PostMessage($handle,1126,[IntPtr]$buttonId,[IntPtr]::Zero)}
  if(-not $p.WaitForExit(3000)){
   $identity=Get-CimInstance Win32_Process -Filter "ProcessId=$($p.Id)"
   if($identity.ExecutablePath -eq $exe){Stop-Process -Id $p.Id -Force;$report.forcedCleanup=$true}else{$errors+='Process ownership changed'}
  }
 }
 foreach($key in $old.Keys){[Environment]::SetEnvironmentVariable($key,$old[$key],'Process')}
 $after=Get-TestMachineState
 if(($before|ConvertTo-Json -Depth 30 -Compress) -ne ($after|ConvertTo-Json -Depth 30 -Compress)){$errors+='VM baseline changed'}
 $report.cleanup=@{status=$(if($errors.Count){'failed'}else{'passed'});errors=$errors}
 if($errors.Count){$report.status='failed'}
 Write-AcceptanceJson $report "$root/report.json"
 Write-AcceptanceJson $report (Join-Path 'C:\AgentHub-VM-Test\evidence' ($Case+'.json'))
}
if($report.status -ne 'passed'){exit 1}
