param(
    [Parameter(Mandatory=$true)][string]$Case,
    [Parameter(Mandatory=$true)][string]$Installer,
    [Parameter(Mandatory=$true)][string]$Cli,
    [string]$Root = 'C:\AgentHub-VM-Test'
)
# Run in the logged-in desktop of the disposable Windows VM, never on the host.
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$utf8 = New-Object Text.UTF8Encoding($false)
[Console]::OutputEncoding = $utf8
$OutputEncoding = $utf8
$caseRoot = Join-Path $Root ('cases/' + $Case)
$reportPath = Join-Path $Root ('evidence/' + $Case + '.json')
$report = [ordered]@{case=$Case;status='running';startedAt=(Get-Date).ToUniversalTime().ToString('o');steps=@();screens=@()}
$rememberedKey = 'HKCU:\Software\agenthub\AgentHub'
$app = $null
function Write-Json($Value, [string]$Path) { [IO.File]::WriteAllText($Path, ($Value | ConvertTo-Json -Depth 80), $utf8) }
function Save-Report { Write-Json $report $reportPath }
function Step([string]$Text) { $report.steps += $Text; Save-Report }
function Registrations {
    @(Get-ChildItem 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall' | Get-ItemProperty | Where-Object { $_.PSChildName -eq 'AgentHub' -or $_.DisplayName -eq 'AgentHub' })
}
function Run-Installer([string]$Path) {
    $p = Start-Process -FilePath $Path -ArgumentList '/S' -WindowStyle Hidden -PassThru
    if(-not $p.WaitForExit(120000)){throw 'Installer did not finish within 120 seconds'}
    if($p.ExitCode -ne 0){throw "Installer failed with $($p.ExitCode)"}
    $entries = @(Registrations)
    if($entries.Count -ne 1){throw "Expected one uninstall registration, found $($entries.Count)"}
    $script:installRoot = $entries[0].InstallLocation.Trim('"')
    $script:dataRoot = Join-Path $installRoot 'data'
    $script:exe = Join-Path $installRoot 'AgentHub.exe'
    if(-not (Test-Path -LiteralPath $exe)){throw 'Installed executable missing'}
    $installerVersion=(Get-Item -LiteralPath $Path).VersionInfo.ProductVersion
    $exeVersion=(Get-Item -LiteralPath $exe).VersionInfo.FileVersion
    if($entries[0].DisplayVersion -ne $installerVersion -or $exeVersion -ne $installerVersion){throw 'Installed executable or registration version differs from installer'}
    if(-not $report.Contains('installations')){$report.installations=@()}
    $report.installations+=@{time=(Get-Date).ToUniversalTime().ToString('o');installer=$Path;installerVersion=$installerVersion;exeVersion=$exeVersion;exeSha256=(Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash;registrationVersion=$entries[0].DisplayVersion;registrationKey=$entries[0].PSChildName}
    if((Get-Item -LiteralPath $rememberedKey).GetValue('') -ne $installRoot){throw 'Remembered path differs from installation'}
    $report.registration = $entries | Select-Object PSChildName,DisplayName,DisplayVersion,InstallLocation,UninstallString
    $report.installRoot = $installRoot
    $shell=New-Object -ComObject WScript.Shell
    $shortcuts=@()
    foreach($folder in @([Environment]::GetFolderPath('Desktop'),[Environment]::GetFolderPath('Programs'))){
        foreach($name in @('AgentHub.lnk')){
            $link=Join-Path $folder $name
            if(Test-Path -LiteralPath $link){
                $target=$shell.CreateShortcut($link).TargetPath
                if($target -ne $exe){throw 'Application shortcut points to the wrong executable'}
                if($name -ne ($entries[0].DisplayName+'.lnk')){throw 'Application shortcut name differs from the product'}
                $shortcuts+=@{path=$link;target=$target}
            }
        }
    }
    if($shortcuts.Count -ne 2){throw 'Expected both desktop and Start Menu shortcuts'}
    $report.shortcuts=$shortcuts
}
function Call-Core([string]$Method, $Arguments=@{}) {
    $request = Join-Path $caseRoot 'request.json'
    Write-Json $Arguments $request
    $raw = & $script:activeCli --data-dir $dataRoot call $Method --args-file $request --apply
    if($LASTEXITCODE -ne 0){throw "Core fixture call failed: $Method $raw"}
    $raw | ConvertFrom-Json
}
function Close-Desktop {
    if($app -and -not $app.HasExited){
        [void]$app.CloseMainWindow()
        if(-not $app.WaitForExit(15000)){throw 'Desktop did not close cleanly'}
    }
    $script:app = $null
}
function Save-Screen([string]$Name) {
    $rect = New-Object VmWindowCapture+Rect
    if(-not [VmWindowCapture]::GetWindowRect($app.MainWindowHandle,[ref]$rect)){throw 'Window bounds unavailable'}
    $bitmap = New-Object Drawing.Bitmap(($rect.Right-$rect.Left),($rect.Bottom-$rect.Top))
    $graphics = [Drawing.Graphics]::FromImage($bitmap)
    $dc = $graphics.GetHdc()
    try { $ok=[VmWindowCapture]::PrintWindow($app.MainWindowHandle,$dc,2) }
    finally {$graphics.ReleaseHdc($dc);$graphics.Dispose()}
    $path = Join-Path $caseRoot ($Name+'.png')
    try {
        if(-not $ok -or $bitmap.GetPixel([int]($bitmap.Width/2),[int]($bitmap.Height/2)).GetBrightness() -lt 0.1){throw 'Window did not paint'}
        $bitmap.Save($path,[Drawing.Imaging.ImageFormat]::Png)
    } finally {$bitmap.Dispose()}
    $report.screens += $path
}
function Elements { $script:window.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.Condition]::TrueCondition) }
function Probe-Desktop([string]$Stage,[bool]$Seeded=$false) {
    if($env:AGENTHUB_DATA_DIR){throw 'Default data discovery must not use AGENTHUB_DATA_DIR'}
    $script:app = Start-Process -FilePath $exe -WorkingDirectory $installRoot -WindowStyle Hidden -PassThru
    $ready=$false
    for($attempt=0;$attempt -lt 90;$attempt++){
        $app.Refresh()
        if($app.HasExited){throw "Desktop exited at $Stage"}
        if($app.MainWindowHandle -ne [IntPtr]::Zero){
            $script:window=[Windows.Automation.AutomationElement]::FromHandle($app.MainWindowHandle)
            $names=@(Elements | ForEach-Object {$_.Current.Name})
            if($names -contains 'Prompts'){$ready=$true;break}
        }
        Start-Sleep -Milliseconds 500
    }
    if(-not $ready){throw "Desktop navigation unavailable: $Stage"}
    Start-Sleep -Milliseconds 1500
    $names=@(Elements | ForEach-Object {$_.Current.Name})
    if($Seeded -and $names -notcontains ('VM Prompt '+$Case)){throw "Persisted Prompt missing: $Stage"}
    [IO.File]::WriteAllLines((Join-Path $caseRoot ($Stage+'-prompts-ui.txt')),[string[]]$names,$utf8)
    Save-Screen ($Stage+'-prompts')
    foreach($page in @(@{key='skills';pattern='^Skills?\s'},@{key='projects';pattern='^(Projects|\u9879\u76ee)\s'},@{key='settings';pattern='^(Settings|\u8bbe\u7f6e)\s'})){
        $buttons=@(Elements | Where-Object {$_.Current.ControlType -eq [Windows.Automation.ControlType]::Button -and $_.Current.Name -match $page.pattern})
        if($buttons.Count -ne 1){throw "Ambiguous navigation $($page.key): $($buttons.Count)"}
        $buttons[0].GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern).Invoke()
        Start-Sleep -Milliseconds 800
        $names=@(Elements | ForEach-Object {$_.Current.Name})
        if($Seeded -and $page.key -eq 'skills' -and $names -notcontains 'vm-fixture'){throw 'Persisted Skill missing in desktop'}
        [IO.File]::WriteAllLines((Join-Path $caseRoot ($Stage+'-'+$page.key+'-ui.txt')),[string[]]$names,$utf8)
        Save-Screen ($Stage+'-'+$page.key)
    }
    $about=@(Elements | Where-Object {$_.Current.ControlType -eq [Windows.Automation.ControlType]::TabItem -and $_.Current.Name -match '^(About|\u5173\u4e8e)$'})
    if($about.Count -ne 1){throw 'About tab missing'}
    $about[0].GetCurrentPattern([Windows.Automation.SelectionItemPattern]::Pattern).Select()
    Start-Sleep -Milliseconds 500
    $names=@(Elements | ForEach-Object {$_.Current.Name})
    if($names -notcontains ('AgentHub '+$report.registration.DisplayVersion)){throw 'Rendered app version differs from installer'}
    [IO.File]::WriteAllLines((Join-Path $caseRoot ($Stage+'-about-ui.txt')),[string[]]$names,$utf8)
    Save-Screen ($Stage+'-about')
    Close-Desktop
    if(-not (Test-Path -LiteralPath (Join-Path $dataRoot 'agenthub.sqlite3'))){throw 'Default data root was not initialized'}
    Step "Native desktop $Stage rendered four pages and matching About version, then closed cleanly without a data-dir override"
}
function Data-Proof {
    if(-not (Test-Path -LiteralPath (Join-Path $dataRoot 'agenthub.sqlite3'))){throw 'Database missing before the read; do not recreate it with the CLI'}
    $snap=Call-Core 'snapshot'
    $backups=Call-Core 'backups.list'
    $files=@()
    foreach($directory in @((Join-Path $dataRoot 'library'),(Join-Path $dataRoot 'backups'),(Join-Path $caseRoot 'external'))){
        foreach($file in @(Get-ChildItem -LiteralPath $directory -File -Recurse -ErrorAction SilentlyContinue)){
            $files += [ordered]@{path=$file.FullName;sha256=(Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash}
        }
    }
    [ordered]@{prompts=$snap.prompts;skills=@($snap.skills | Select-Object id,path,source);deployments=$snap.deployments;backups=$backups.backups;files=@($files | Sort-Object {$_.path});userFile=(Get-FileHash -LiteralPath (Join-Path $installRoot 'user-note.txt') -Algorithm SHA256).Hash}
}
function Assert-Data([string]$Stage) {
    $proof=Data-Proof
    Write-Json $proof (Join-Path $caseRoot ($Stage+'-data.json'))
    if(($proof | ConvertTo-Json -Depth 80 -Compress) -ne ($script:baseline | ConvertTo-Json -Depth 80 -Compress)){throw "Data changed during $Stage"}
    Step "Prompt, Skill, source, deployment, backup, external files and user file preserved: $Stage"
}
function Uninstall {
    $database=Join-Path $dataRoot 'agenthub.sqlite3'
    $beforeDatabaseHash=(Get-FileHash -LiteralPath $database -Algorithm SHA256).Hash
    $uninstaller=Join-Path $installRoot 'uninstall.exe'
    $p=Start-Process -FilePath $uninstaller -ArgumentList "/S _?=$installRoot" -WindowStyle Hidden -PassThru
    if(-not $p.WaitForExit(90000) -or $p.ExitCode -ne 0){throw 'Uninstall failed'}
    if(Test-Path -LiteralPath $exe){throw 'Uninstall left application executable'}
    if(-not (Test-Path -LiteralPath $database) -or (Get-FileHash -LiteralPath $database -Algorithm SHA256).Hash -ne $beforeDatabaseHash){throw 'Uninstall removed or changed the database before any CLI read'}
    if(@(Registrations).Count -ne 0){throw 'Uninstall left registered application'}
    foreach($folder in @([Environment]::GetFolderPath('Desktop'),[Environment]::GetFolderPath('Programs'))){
        foreach($name in @('AgentHub.lnk')){
            if(Test-Path -LiteralPath (Join-Path $folder $name)){throw 'Uninstall left shortcut'}
        }
    }
    Step 'Ordinary uninstall removed executable, registration and shortcuts; database hash was unchanged before any CLI read'
}
try {
    if($env:COMPUTERNAME -ne 'TEST' -or $env:USERNAME -ne 'Try' -or [Diagnostics.Process]::GetCurrentProcess().SessionId -eq 0){throw 'Requires the authorized TEST/Try interactive VM session'}
    if([IO.Path]::GetFullPath($Root) -ne 'C:\AgentHub-VM-Test' -or $Case -notmatch '^[a-z0-9-]+$'){throw 'Invalid VM fixture root or case'}
    if(Test-Path -LiteralPath $caseRoot){throw 'Case already exists; preserve evidence and use a new case name'}
    if(@(Registrations).Count -gt 0){throw 'Existing installation must be handled before this independent case'}
    if(Test-Path -LiteralPath $rememberedKey){throw 'Initial remembered installation state must be empty'}
    New-Item -ItemType Directory -Force -Path $caseRoot,(Join-Path $Root 'evidence') | Out-Null
    $os=Get-CimInstance Win32_OperatingSystem
    $report.environment=@{computer=$env:COMPUTERNAME;user=$env:USERNAME;os=$os.Caption;version=$os.Version;session=[Diagnostics.Process]::GetCurrentProcess().SessionId;webview=@(Get-ChildItem 'C:\Program Files (x86)\Microsoft\EdgeWebView\Application' -Directory | Select-Object -ExpandProperty Name)}
    $report.installer=@{path=$Installer;sha256=(Get-FileHash -LiteralPath $Installer -Algorithm SHA256).Hash;signature=(Get-AuthenticodeSignature -LiteralPath $Installer).Status.ToString()}
    $script:activeCli=$Cli
    $cliVersion=(& $activeCli --version) -join ''
    if($LASTEXITCODE -ne 0 -or $cliVersion -ne ('agenthub-dev '+(Get-Item -LiteralPath $Installer).VersionInfo.ProductVersion)){throw 'CLI does not match installer version'}
    $report.initialCli=@{version=$cliVersion;sha256=(Get-FileHash -LiteralPath $Cli -Algorithm SHA256).Hash}
    $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS='--force-renderer-accessibility'
    Add-Type -AssemblyName UIAutomationClient,UIAutomationTypes,System.Drawing
    Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class VmWindowCapture {
 [StructLayout(LayoutKind.Sequential)] public struct Rect {public int Left,Top,Right,Bottom;}
 [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h,out Rect r);
 [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h,IntPtr dc,uint f);
}
'@
    Run-Installer $Installer
    $initialRoot=$installRoot
    $expectedRoot=Join-Path $env:LOCALAPPDATA $report.registration.DisplayName
    if($installRoot -ne $expectedRoot){throw "Fresh default directory differs: $installRoot versus $expectedRoot"}
    $report.initialExe=@{version=(Get-Item -LiteralPath $exe).VersionInfo.FileVersion;sha256=(Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash}
    Step 'Fresh silent install chose the product default directory with exactly one registration'
    Probe-Desktop 'fresh'
    $snap=Call-Core 'snapshot'
    $snap.settings.scanRoots=@();$snap.settings.language='en'
    Call-Core 'settings.save' @{settings=$snap.settings} | Out-Null
    foreach($target in (Call-Core 'targets.list').targets){
        $target.globalPath=Join-Path $caseRoot ('external/'+$target.id)
        $target.enabled=($target.id -eq 'codex')
        Call-Core 'targets.save' @{target=$target} | Out-Null
    }
    Call-Core 'prompts.save' @{prompt=@{id='';title=('VM Prompt '+$Case);body="VM retained content`nSecond line";purpose='Installer lifecycle';category='VM';tags=@();favorite=$false;createdAt='';updatedAt=''}} | Out-Null
    $source=Join-Path $caseRoot 'source';New-Item -ItemType Directory -Path $source | Out-Null
    $sourceFile=Join-Path $source 'SKILL.md'
    [IO.File]::WriteAllText($sourceFile,"---`nname: vm-fixture`ndescription: VM lifecycle fixture`n---`nOriginal content",$utf8)
    $inspection=Call-Core 'sources.inspect' @{source=@{kind='local';locator=$source}}
    $added=Call-Core 'skills.add' @{inspectionId=$inspection.inspectionId;subpath=$inspection.candidates[0].subpath}
    $plan=Call-Core 'skills.install.preview' @{skillId=$added.skill.id;targetId='codex'}
    Call-Core 'skills.install' @{planId=$plan.id;confirmed=$true} | Out-Null
    [IO.File]::AppendAllText($sourceFile,"`nSecond version",$utf8)
    $check=Call-Core 'skills.check' @{skillId=$added.skill.id}
    $plan=Call-Core 'skills.update.preview' @{skillId=$added.skill.id;checkId=$check.checkId;locationIds=@('library');retainBackup=$true}
    $updated=Call-Core 'skills.update' @{planId=$plan.id;confirmed=$true}
    if(-not $updated.backupId){throw 'Fixture did not create a real backup'}
    [IO.File]::WriteAllText((Join-Path $installRoot 'user-note.txt'),'Unowned file must survive',$utf8)
    $script:baseline=Data-Proof
    Write-Json $baseline (Join-Path $caseRoot 'baseline-data.json')
    Probe-Desktop 'seeded' $true
    Probe-Desktop 'relaunch' $true
    Run-Installer $Installer
    if($installRoot -ne $initialRoot){throw 'Same-version reinstall changed directory'}
    Assert-Data 'same-version-reinstall'
    Uninstall
    Assert-Data 'after-uninstall'
    Run-Installer $Installer
    if($installRoot -ne $initialRoot){throw 'Reinstall after uninstall lost remembered directory'}
    Probe-Desktop 'reinstall-after-uninstall' $true
    Assert-Data 'reinstall-after-uninstall'
    Uninstall
    Assert-Data 'final-uninstall'
    # Only the test-created remembered value is removed; all retained fixture data stays as evidence.
    if((Get-Item -LiteralPath $rememberedKey).GetValue('') -ne $initialRoot){throw 'Unexpected remembered state during cleanup'}
    $key=[Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Software\agenthub\AgentHub',$true)
    try{$key.DeleteValue('', $false)}finally{$key.Dispose()}
    $remaining=Get-Item -LiteralPath $rememberedKey
    if($remaining.ValueCount -eq 0 -and $remaining.SubKeyCount -eq 0){Remove-Item -LiteralPath $rememberedKey}
    $retainedDestination=Join-Path $caseRoot 'retained-installation'
    $resolvedInstall=(Resolve-Path -LiteralPath $initialRoot).Path
    if($resolvedInstall -ne $expectedRoot -or -not $retainedDestination.StartsWith($caseRoot+'\') -or (Test-Path -LiteralPath $retainedDestination)){throw 'Unsafe fixture evidence move'}
    Move-Item -LiteralPath $resolvedInstall -Destination $retainedDestination
    $report.retainedData=$retainedDestination
    $report.status='passed'
}catch{
    $report.status='failed';$report.error=$_.ToString();$report.stack=$_.ScriptStackTrace
}finally{
    if($app -and -not $app.HasExited){try{Close-Desktop}catch{$report.closeError=$_.ToString()}}
    $report.finishedAt=(Get-Date).ToUniversalTime().ToString('o')
    Save-Report
}
if($report.status -ne 'passed'){throw $report.error}
