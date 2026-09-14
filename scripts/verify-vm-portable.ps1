param(
    [Parameter(Mandatory=$true)][string]$PortableZip,
    [Parameter(Mandatory=$true)][string]$Cli,
    [string]$Root = 'C:\AgentHub-VM-Test',
    [string]$Case = 'portable-relocation'
)
# Windows PowerShell 5.1; run only in the authorized disposable VM desktop.
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$utf8 = New-Object Text.UTF8Encoding($false)
[Console]::OutputEncoding = $utf8
$OutputEncoding = $utf8
$caseRoot = Join-Path $Root ('cases/' + $Case)
$reportPath = Join-Path $Root ('evidence/' + $Case + '.json')
$report = [ordered]@{case=$Case;status='running';startedAt=(Get-Date).ToUniversalTime().ToString('o');steps=@();screens=@()}
$reportReady=$false
$app=$null
function Write-Json($Value,[string]$Path) { [IO.File]::WriteAllText($Path,($Value | ConvertTo-Json -Depth 80),$utf8) }
function Read-Text([string]$Path) { [IO.File]::ReadAllText($Path,$utf8) }
function Save-Report { if($script:reportReady){Write-Json $report $reportPath} }
function Step([string]$Text) { $report.steps += $Text; Save-Report }
function Assert-FixturePath([string]$Path) {
    $resolved=[IO.Path]::GetFullPath($Path)
    if(-not $resolved.StartsWith($caseRoot+'\',[StringComparison]::OrdinalIgnoreCase)){throw "Path escaped case directory: $Path"}
    $cursor=$resolved
    while($cursor -and $cursor.StartsWith($Root,[StringComparison]::OrdinalIgnoreCase)){
        if(Test-Path -LiteralPath $cursor){
            if(((Get-Item -LiteralPath $cursor -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0){throw "Reparse point in fixture: $cursor"}
        }
        $cursor=Split-Path -Parent $cursor
    }
    $resolved
}
function Files([string]$Directory) {
    $base=Assert-FixturePath $Directory
    if(-not (Test-Path -LiteralPath $base -PathType Container)){throw "Fixture directory missing: $base"}
    $items=@(Get-ChildItem -LiteralPath $base -Force -Recurse)
    foreach($item in $items){if(($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0){throw "Linked fixture entry: $($item.FullName)"}}
    @($items | Where-Object {-not $_.PSIsContainer} | Sort-Object FullName | ForEach-Object {
        [ordered]@{path=$_.FullName.Substring($base.Length+1);sha256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash}
    })
}
function Same($Actual,$Expected,[string]$Label) {
    if(($Actual | ConvertTo-Json -Depth 80 -Compress) -ne ($Expected | ConvertTo-Json -Depth 80 -Compress)){throw "Mismatch: $Label"}
}
function Call-Core([string]$Method,$Arguments=@{}) {
    $request=Join-Path $caseRoot 'request.json'
    Write-Json $Arguments $request
    $raw=& $Cli --data-dir $dataRoot call $Method --args-file $request --apply
    if($LASTEXITCODE -ne 0){throw "Core call failed: $Method $raw"}
    $raw | ConvertFrom-Json
}
function Require-Success($Result,[string]$Label) {
    if($Result.status -ne 'succeeded'){throw "Operation did not succeed: $Label"}
}
function Close-Desktop {
    if($app -and -not $app.HasExited){
        [void]$app.CloseMainWindow()
        if(-not $app.WaitForExit(15000)){throw 'Desktop did not close cleanly'}
    }
    # WebView2 may still be deleting its lockfile after the desktop process exits.
    # Wait only for children using this fixture's data directory before hashing/moving it.
    $webviewRemaining=@()
    for($attempt=0;$attempt -lt 60;$attempt++){
        $webviewRemaining=@(Get-CimInstance Win32_Process -Filter "Name='msedgewebview2.exe'" | Where-Object {$_.CommandLine -and $_.CommandLine.IndexOf($dataRoot,[StringComparison]::OrdinalIgnoreCase) -ge 0})
        if($webviewRemaining.Count -eq 0){break}
        Start-Sleep -Milliseconds 250
    }
    if($webviewRemaining.Count -ne 0){throw 'WebView2 still owns the fixture data directory after desktop exit'}
    $script:app=$null
}
function Elements { $script:window.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.Condition]::TrueCondition) }
function Save-Screen([string]$Name) {
    $rect=New-Object VmPortableCapture+Rect
    if(-not [VmPortableCapture]::GetWindowRect($app.MainWindowHandle,[ref]$rect)){throw 'Window bounds unavailable'}
    $bitmap=New-Object Drawing.Bitmap(($rect.Right-$rect.Left),($rect.Bottom-$rect.Top))
    $graphics=[Drawing.Graphics]::FromImage($bitmap)
    $dc=$graphics.GetHdc()
    try{$ok=[VmPortableCapture]::PrintWindow($app.MainWindowHandle,$dc,2)}finally{$graphics.ReleaseHdc($dc);$graphics.Dispose()}
    $path=Join-Path $caseRoot ($Name+'.png')
    try{
        if(-not $ok -or $bitmap.GetPixel([int]($bitmap.Width/2),[int]($bitmap.Height/2)).GetBrightness() -lt 0.1){throw 'Window did not paint'}
        $bitmap.Save($path,[Drawing.Imaging.ImageFormat]::Png)
    }finally{$bitmap.Dispose()}
    $report.screens += $path
}
function Probe-Desktop([string]$Stage) {
    if($env:AGENTHUB_DATA_DIR){throw 'Portable discovery must not use AGENTHUB_DATA_DIR'}
    # Deliberately use a working directory outside the package.
    $script:app=Start-Process -FilePath $exe -WorkingDirectory $caseRoot -WindowStyle Hidden -PassThru
    $ready=$false
    for($attempt=0;$attempt -lt 90;$attempt++){
        $app.Refresh()
        if($app.HasExited){throw "Desktop exited: $Stage"}
        if($app.MainWindowHandle -ne [IntPtr]::Zero){
            $script:window=[Windows.Automation.AutomationElement]::FromHandle($app.MainWindowHandle)
            $names=@(Elements | ForEach-Object {$_.Current.Name})
            if($names -contains 'Prompts' -and $names -contains 'VM Portable Prompt'){$ready=$true;break}
        }
        Start-Sleep -Milliseconds 500
    }
    if(-not $ready){throw "Native Prompt navigation/content unavailable: $Stage"}
    Start-Sleep -Milliseconds 1500
    [IO.File]::WriteAllLines((Join-Path $caseRoot ($Stage+'-prompts-ui.txt')),[string[]]$names,$utf8)
    Save-Screen ($Stage+'-prompts')
    foreach($page in @(@{key='skills';pattern='^Skills?\s';expected='vm-portable'},@{key='projects';pattern='^(Projects|\u9879\u76ee)\s';expected='VM Portable Project'},@{key='settings';pattern='^(Settings|\u8bbe\u7f6e)\s';expected=$null})){
        $buttons=@(Elements | Where-Object {$_.Current.ControlType -eq [Windows.Automation.ControlType]::Button -and $_.Current.Name -match $page.pattern})
        if($buttons.Count -ne 1){throw "Ambiguous navigation: $($page.key)"}
        $buttons[0].GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern).Invoke()
        Start-Sleep -Milliseconds 1000
        $names=@(Elements | ForEach-Object {$_.Current.Name})
        if($page.expected -and $names -notcontains $page.expected){throw "Persisted $($page.key) missing: $Stage"}
        [IO.File]::WriteAllLines((Join-Path $caseRoot ($Stage+'-'+$page.key+'-ui.txt')),[string[]]$names,$utf8)
        Save-Screen ($Stage+'-'+$page.key)
    }
    $about=@(Elements | Where-Object {$_.Current.ControlType -eq [Windows.Automation.ControlType]::TabItem -and $_.Current.Name -eq 'About'})
    if($about.Count -ne 1){throw 'About tab missing'}
    $about[0].GetCurrentPattern([Windows.Automation.SelectionItemPattern]::Pattern).Select()
    Start-Sleep -Milliseconds 500
    $names=@(Elements | ForEach-Object {$_.Current.Name})
    if($names -notcontains ('AgentHub '+$version)){throw 'Rendered portable version differs from executable'}
    [IO.File]::WriteAllLines((Join-Path $caseRoot ($Stage+'-about-ui.txt')),[string[]]$names,$utf8)
    Save-Screen ($Stage+'-about')
    Close-Desktop
    if(-not (Test-Path -LiteralPath (Join-Path $dataRoot 'agenthub.sqlite3'))){throw 'Portable database missing'}
    Step "Native desktop $Stage rendered stored Prompt, Skill, project and settings without data-dir override"
}
try{
    if($env:COMPUTERNAME -ne 'TEST' -or $env:USERNAME -ne 'Try' -or [Diagnostics.Process]::GetCurrentProcess().SessionId -eq 0){throw 'Requires authorized TEST/Try interactive VM session'}
    if([IO.Path]::GetFullPath($Root) -ne 'C:\AgentHub-VM-Test' -or $Case -notmatch '^[a-z0-9-]+$'){throw 'Invalid VM fixture root or case'}
    if($env:AGENTHUB_DATA_DIR){throw 'Remove AGENTHUB_DATA_DIR before this test'}
    if(Test-Path -LiteralPath $caseRoot){throw 'Case already exists; preserve evidence and use a new case name'}
    [void](Assert-FixturePath (Join-Path $caseRoot 'A'))
    New-Item -ItemType Directory -Path $caseRoot,(Join-Path $Root 'evidence') -Force | Out-Null
    $reportReady=$true
    $a=Assert-FixturePath (Join-Path $caseRoot 'A')
    $b=Assert-FixturePath (Join-Path $caseRoot 'B')
    $external=Assert-FixturePath (Join-Path $caseRoot 'external')
    $source=Assert-FixturePath (Join-Path $external 'source')
    $projectPath=Assert-FixturePath (Join-Path $external 'project')
    New-Item -ItemType Directory -Path $a,$source,$projectPath | Out-Null
    $os=Get-CimInstance Win32_OperatingSystem
    $report.environment=@{computer=$env:COMPUTERNAME;user=$env:USERNAME;os=$os.Caption;version=$os.Version;session=[Diagnostics.Process]::GetCurrentProcess().SessionId}
    $report.input=@{zip=$PortableZip;zipSha256=(Get-FileHash -LiteralPath $PortableZip -Algorithm SHA256).Hash;cli=$Cli;cliSha256=(Get-FileHash -LiteralPath $Cli -Algorithm SHA256).Hash}
    Add-Type -AssemblyName System.IO.Compression.FileSystem,UIAutomationClient,UIAutomationTypes,System.Drawing
    $archive=[IO.Compression.ZipFile]::OpenRead($PortableZip)
    try{
        foreach($entry in $archive.Entries){
            $destination=[IO.Path]::GetFullPath((Join-Path $a $entry.FullName))
            if(-not $destination.StartsWith($a+'\',[StringComparison]::OrdinalIgnoreCase)){throw 'ZIP entry escapes portable directory'}
        }
    }finally{$archive.Dispose()}
    Expand-Archive -LiteralPath $PortableZip -DestinationPath $a
    $manifest=@(Files $a)
    Same @($manifest | ForEach-Object {$_.path.Replace('\','/')} | Sort-Object) @('AgentHub.exe','data/runtime/LICENSE','data/runtime/THIRD-PARTY-NOTICES.md','data/runtime/agenthub-layout.json','data/runtime/DEPENDENCY-NOTICES.md','data/runtime/NSIS-COPYING.txt','data/runtime/inventory.json','data/runtime/RUST-STDLIB-NOTICES.html' | Sort-Object) 'desktop-only portable package'
    $layout=(Read-Text (Join-Path $a 'data/runtime/agenthub-layout.json')) | ConvertFrom-Json
    if($layout.schemaVersion -ne 1 -or $layout.dataDirectory -ne 'data' -or $layout.libraryDirectory -ne 'data/library/skills'){throw 'Portable layout marker invalid'}
    $exe=Join-Path $a 'AgentHub.exe'
    $version=(Get-Item -LiteralPath $exe).VersionInfo.FileVersion
    $cliVersion=(& $Cli --version) -join ''
    if($LASTEXITCODE -ne 0 -or $version -notmatch '^\d+\.\d+\.\d+$' -or $cliVersion -ne ('agenthub-dev '+$version)){throw 'Portable executable and CLI versions must match'}
    if((Get-Item -LiteralPath $exe).VersionInfo.ProductName -ne 'AgentHub'){throw 'Requires an AgentHub portable package'}
    $report.input.version=$version;$report.input.cliVersion=$cliVersion;$report.packageManifest=$manifest
    $dataRoot=Join-Path $a 'data'
    Call-Core 'settings.save' @{settings=@{scanRoots=@();language='en'}} | Out-Null
    Call-Core 'maintenance.save' @{automaticChecks=$false;retainUpdateBackup=$true} | Out-Null
    foreach($target in (Call-Core 'targets.list').targets){
        $target.globalPath=Join-Path $external ('agent-'+$target.id)
        $target.enabled=($target.id -eq 'codex')
        Call-Core 'targets.save' @{target=$target} | Out-Null
    }
    $prompt=Call-Core 'prompts.save' @{prompt=@{title='VM Portable Prompt';body="Preserved portable content`nSecond line";purpose='Portable relocation';category='VM';tags=@('portable');favorite=$true}}
    [IO.File]::WriteAllText((Join-Path $projectPath 'keep.txt'),'Project content must not move or change',$utf8)
    $project=Call-Core 'projects.save' @{project=@{name='VM Portable Project';path=$projectPath}}
    if($project.gitTrusted){throw 'New project unexpectedly trusted'}
    $v1="---`nname: vm-portable`ndescription: VM portable fixture`n---`nVersion one`n"
    $v2=$v1+"Version two`n"
    $sourceFile=Join-Path $source 'SKILL.md'
    [IO.File]::WriteAllText($sourceFile,$v1,$utf8)
    $inspection=Call-Core 'sources.inspect' @{source=@{kind='local';locator=$source}}
    $added=Call-Core 'skills.add' @{inspectionId=$inspection.inspectionId;subpath=$inspection.candidates[0].subpath}
    $skillId=$added.skill.id
    $plan=Call-Core 'skills.install.preview' @{skillId=$skillId;targetId='codex'}
    Require-Success (Call-Core 'skills.install' @{planId=$plan.id;confirmed=$true}) 'initial external install'
    [IO.File]::WriteAllText($sourceFile,$v2,$utf8)
    $check=Call-Core 'skills.check' @{skillId=$skillId}
    $plan=Call-Core 'skills.update.preview' @{skillId=$skillId;checkId=$check.checkId;locationIds=@('library');retainBackup=$true}
    $update=Call-Core 'skills.update' @{planId=$plan.id;confirmed=$true}
    Require-Success $update 'library update'
    if(-not $update.backupId){throw 'Update did not create a real backup'}
    $backupId=$update.backupId
    $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS='--force-renderer-accessibility'
    Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class VmPortableCapture {
 [StructLayout(LayoutKind.Sequential)] public struct Rect {public int Left,Top,Right,Bottom;}
 [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h,out Rect r);
 [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h,IntPtr dc,uint f);
}
'@
    Probe-Desktop 'A-seeded'
    $before=Call-Core 'snapshot'
    $backupsBefore=Call-Core 'backups.list'
    $externalBefore=@(Files $external)
    $packageBefore=@(Files $a)
    Write-Json @{snapshot=$before;backups=$backupsBefore;externalFiles=$externalBefore;packageFiles=$packageBefore} (Join-Path $caseRoot 'before-move.json')
    if($app){throw 'Desktop still active before move'}
    [void](Assert-FixturePath $a);[void](Assert-FixturePath $b)
    if(Test-Path -LiteralPath $b){throw 'Destination already exists'}
    Move-Item -LiteralPath $a -Destination $b
    if(Test-Path -LiteralPath $a){throw 'Old portable root survived move'}
    Same @(Files $b) $packageBefore 'complete package bytes immediately after move'
    Same @(Files $external) $externalBefore 'external files immediately after move'
    $dataRoot=Join-Path $b 'data';$exe=Join-Path $b 'AgentHub.exe'
    $report.moved=@{from=$a;to=$b;firstConsumer='native desktop';time=(Get-Date).ToUniversalTime().ToString('o')}
    Save-Report
    # No CLI dispatch may occur between Move-Item and this native first start.
    Probe-Desktop 'B-first-start'
    if(Test-Path -LiteralPath $a){throw 'Native restart recreated old portable root'}
    $after=Call-Core 'snapshot'
    if([IO.Path]::GetFullPath($after.dataDir) -ne [IO.Path]::GetFullPath($dataRoot)){throw 'Snapshot does not use B/data'}
    Same $after.prompts $before.prompts 'complete Prompt records'
    Same $after.projects $before.projects 'untrusted project and external path'
    Same $after.deployments $before.deployments 'external deployment records'
    $movedSkill=@($after.skills | Where-Object {$_.id -eq $skillId})
    if($movedSkill.Count -ne 1){throw 'Moved library Skill missing'}
    $libraryFile=Join-Path $movedSkill[0].path 'SKILL.md'
    if(-not ([IO.Path]::GetFullPath($libraryFile)).StartsWith($dataRoot+'\library\skills\',[StringComparison]::OrdinalIgnoreCase)){throw 'Library still points outside B/data'}
    Same $movedSkill[0].source $added.skill.source 'local source association'
    Same (Read-Text $libraryFile) $v2 'updated library content after move'
    Same @(Files $external) $externalBefore 'external files after native first start'
    Write-Json @{snapshot=$after;backups=(Call-Core 'backups.list');externalFiles=@(Files $external)} (Join-Path $caseRoot 'after-native-move.json')
    Step 'Whole A moved to B; native first start found data, all user records and external paths were retained'
    $plan=Call-Core 'skills.install.preview' @{skillId=$skillId;targetId='codex'}
    Require-Success (Call-Core 'skills.install' @{planId=$plan.id;confirmed=$true}) 'install from relocated library'
    $checked=Call-Core 'skills.check' @{skillId=$skillId}
    if(-not $checked.checkId -or $checked.checkId -eq $check.checkId){throw 'Fresh source check did not complete after relocation'}
    $deployed=@((Call-Core 'snapshot').deployments | Where-Object {$_.skillId -eq $skillId -and $_.agent -eq 'codex'})
    if($deployed.Count -ne 1){throw 'Expected one external Codex deployment'}
    $deployedFile=Join-Path $deployed[0].path 'SKILL.md'
    Same (Read-Text $deployedFile) $v2 'new install reads relocated library'
    $externalBeforeRestore=@(Files $external)
    Require-Success (Call-Core 'backups.restore' @{id=$backupId;confirmed=$true}) 'restore pre-move library backup'
    Same (Read-Text $libraryFile) $v1 'original library content restored from pre-move backup'
    Same @(Files $external) $externalBeforeRestore 'restore leaves source, project and external deployed copy unchanged'
    $final=Call-Core 'snapshot'
    Same $final.projects $before.projects 'project path and trust after restore'
    Same $final.prompts $before.prompts 'Prompt after restore'
    if(Test-Path -LiteralPath $a){throw 'Follow-up operations recreated old root'}
    Write-Json @{snapshot=$final;check=$checked;backupId=$backupId;externalFiles=@(Files $external);librarySha256=(Get-FileHash -LiteralPath $libraryFile -Algorithm SHA256).Hash} (Join-Path $caseRoot 'after-restore.json')
    Step 'Relocated library installed and checked successfully; pre-move backup restored version one without changing external files'
    $report.retainedData=$b
    $report.status='passed'
}catch{
    $report.status='failed';$report.error=$_.ToString();$report.stack=$_.ScriptStackTrace
    if($app -and -not $app.HasExited){try{Save-Screen 'failure'}catch{$report.captureError=$_.ToString()}}
}finally{
    if($app -and -not $app.HasExited){try{Close-Desktop}catch{$report.status='failed';$report.closeError=$_.ToString()}}
    $report.finishedAt=(Get-Date).ToUniversalTime().ToString('o')
    Save-Report
}
if($report.status -ne 'passed'){throw ($report.error+' '+$report.closeError)}
