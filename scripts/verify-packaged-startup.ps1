param([Parameter(Mandatory=$true)][string]$Exe,[Parameter(Mandatory=$true)][string]$Cli,[string]$DataDir,[ValidateSet('zh','en')][string]$Language='zh',[switch]$RecoveryFixture,[switch]$PortableMove)
$ErrorActionPreference='Stop'
$projectRoot=[IO.Path]::GetFullPath((Split-Path $PSScriptRoot -Parent))
$version=(Get-Content (Join-Path $projectRoot 'package.json') -Raw | ConvertFrom-Json).version
$suffix=if($Language -eq 'en'){'-en'}else{''}
if($RecoveryFixture){$suffix+='-recovery'}
if($PortableMove){
 if($RecoveryFixture -or $DataDir){throw 'PortableMove requires its own fresh fixture'}
 $suffix+='-portable-move'
}
$labels=if($Language -eq 'en'){@{prompt='Prompts';skill='Skills';projects='Projects';settings='Settings';locations='Skill install locations'}}else{@{prompt='Prompts';skill='Skill';projects='项目';settings='设置';locations='Skill 安装位置'}}
$fixture=[IO.Path]::GetFullPath((Join-Path $projectRoot ('.test-data/packaged-startup-'+$version+'-'+[guid]::NewGuid().ToString('N').Substring(0,8))))
if(-not $fixture.StartsWith((Join-Path $projectRoot '.test-data')+'\',[StringComparison]::OrdinalIgnoreCase)){throw 'Invalid fixture root'}
if(Test-Path -LiteralPath $fixture){throw 'Use a fresh startup fixture'}
if($DataDir){
 $resolvedData=[IO.Path]::GetFullPath($DataDir)
 $allowedRoots=@((Join-Path $projectRoot '.test-data'),(Join-Path $projectRoot 'output/installer-tests'))
 if(-not @($allowedRoots | Where-Object {$resolvedData.StartsWith($_+'\',[StringComparison]::OrdinalIgnoreCase)}).Count){throw 'DataDir must be inside an isolated verification directory'}
 if(Test-Path -LiteralPath (Join-Path $resolvedData 'agenthub.sqlite3')){throw 'Use a fresh startup database'}
}
$reportDir=Join-Path $projectRoot ('output/playwright/release-'+$version)
New-Item -ItemType Directory -Force -Path $fixture,$reportDir | Out-Null
$exePath=(Resolve-Path -LiteralPath $Exe).Path
$cliPath=(Resolve-Path -LiteralPath $Cli).Path
if($PortableMove){
 $portableA=Join-Path $fixture 'A';$portableB=Join-Path $fixture 'B'
 New-Item -ItemType Directory -Path (Join-Path $portableA 'data/runtime') -Force | Out-Null
 Copy-Item -LiteralPath $exePath -Destination (Join-Path $portableA 'AgentHub.exe')
 foreach($runtimeFile in @('agenthub-layout.json','LICENSE','THIRD-PARTY-NOTICES.md')){Copy-Item -LiteralPath (Join-Path (Split-Path $exePath -Parent) ('data/runtime/'+$runtimeFile)) -Destination (Join-Path $portableA 'data/runtime')}
}
$variables=@{AGENTHUB_DATA_DIR=$(if($PortableMove){Join-Path $portableA 'data'}elseif($DataDir){[IO.Path]::GetFullPath($DataDir)}else{Join-Path $fixture 'data'});WEBVIEW2_USER_DATA_FOLDER=(Join-Path $fixture 'webview');HOME=(Join-Path $fixture 'home');USERPROFILE=(Join-Path $fixture 'home');APPDATA=(Join-Path $fixture 'appdata');LOCALAPPDATA=(Join-Path $fixture 'local');CODEX_HOME=(Join-Path $fixture 'home/.codex');HERMES_HOME=(Join-Path $fixture 'home/.hermes');XDG_CONFIG_HOME=(Join-Path $fixture 'home/.config');XDG_CACHE_HOME=(Join-Path $fixture 'home/.cache');WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS='--force-renderer-accessibility';WEBVIEW2_BROWSER_EXECUTABLE_FOLDER=$null}
$previous=@{};foreach($key in $variables.Keys){$previous[$key]=[Environment]::GetEnvironmentVariable($key);[Environment]::SetEnvironmentVariable($key,$variables[$key]);if($key -ne 'WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS' -and $variables[$key]){New-Item -ItemType Directory -Force -Path $variables[$key] | Out-Null}}
$report=[ordered]@{version=$version;language=$Language;status='running';exe=$exePath;exeSha256=(Get-FileHash -LiteralPath $exePath -Algorithm SHA256).Hash;data=$variables.AGENTHUB_DATA_DIR;reportDir=$reportDir;userDirectoriesUsed=$false;viteRequired=$false}
$app=$null
function Invoke-Fixture([string]$Method,$Arguments=@{}) {
 $request=Join-Path $fixture 'request.json';$Arguments | ConvertTo-Json -Depth 30 | Set-Content -LiteralPath $request -Encoding utf8
 $raw=& $cliPath --data-dir $variables.AGENTHUB_DATA_DIR call $Method --args-file $request --apply
 if($LASTEXITCODE -ne 0){throw "Fixture call failed: $Method"};return $raw | ConvertFrom-Json -Depth 60
}
try {
 $snapshot=Invoke-Fixture 'snapshot'
 $targets=Invoke-Fixture 'targets.list'
 foreach($target in $targets.targets){$target.globalPath=Join-Path $fixture ('targets/'+$target.id);$target.enabled=$false;Invoke-Fixture 'targets.save' @{target=$target} | Out-Null}
 $snapshot.settings.scanRoots=@();$snapshot.settings.language=$Language;Invoke-Fixture 'settings.save' @{settings=$snapshot.settings} | Out-Null
 Invoke-Fixture 'prompts.save' @{prompt=@{id='';title='桌面验收 Prompt';body="真实桌面读取本地核心数据`n`n完整保留正文换行";purpose='确认浮窗阅读和复制正文';category='验收';tags=@();favorite=$false;createdAt='';updatedAt=''}} | Out-Null
 $source=Join-Path $fixture 'skill-source';New-Item -ItemType Directory -Path $source | Out-Null
 [IO.File]::WriteAllText((Join-Path $source 'SKILL.md'), "---`nname: desktop-fixture`ndescription: 桌面验收 Skill`n---`n真实本地内容")
 $inspection=Invoke-Fixture 'sources.inspect' @{source=@{kind='local';locator=$source}}
 $fixtureSkill=Invoke-Fixture 'skills.add' @{inspectionId=$inspection.inspectionId;subpath=$inspection.candidates[0].subpath}
 Invoke-Fixture 'skills.check' @{skillId=$fixtureSkill.skill.id} | Out-Null
 if($PortableMove){
  $portableTarget=@((Invoke-Fixture 'targets.list').targets | Where-Object id -eq 'codex')[0]
  $portableTarget.enabled=$true;Invoke-Fixture 'targets.save' @{target=$portableTarget} | Out-Null
  $portableInstall=Invoke-Fixture 'skills.install.preview' @{skillId=$fixtureSkill.skill.id;targetId='codex'}
  Invoke-Fixture 'skills.install' @{planId=$portableInstall.id;confirmed=$true} | Out-Null
  $portableOriginal=[IO.File]::ReadAllText((Join-Path $source 'SKILL.md'))
  [IO.File]::AppendAllText((Join-Path $source 'SKILL.md'),"`n第二版便携验收内容")
  $portableCheck=Invoke-Fixture 'skills.check' @{skillId=$fixtureSkill.skill.id}
  $portableUpdate=Invoke-Fixture 'skills.update.preview' @{skillId=$fixtureSkill.skill.id;checkId=$portableCheck.checkId;locationIds=@('library');retainBackup=$true}
  $portableUpdated=Invoke-Fixture 'skills.update' @{planId=$portableUpdate.id;confirmed=$true}
 }
 function Invoke-FixtureGit([string]$Directory,[string[]]$GitArguments) {
  & git -c core.hooksPath=NUL -c core.autocrlf=false -c user.name=Fixture -c user.email=fixture@example.invalid -C $Directory @GitArguments 2>&1 | Out-Null
  if($LASTEXITCODE -ne 0){throw 'Fixture Git command failed'}
 }
 $upstreamProject=Join-Path $fixture 'upstream';New-Item -ItemType Directory -Path $upstreamProject | Out-Null
 Invoke-FixtureGit $upstreamProject @('init','--initial-branch=main')
 [IO.File]::WriteAllText((Join-Path $upstreamProject 'README.md'),'initial')
 Invoke-FixtureGit $upstreamProject @('add','.')
 Invoke-FixtureGit $upstreamProject @('commit','-m','共同起点')
 $localProject=Join-Path $fixture 'project'
 Invoke-FixtureGit $fixture @('clone',$upstreamProject,$localProject)
 foreach($change in @('first change','latest change')) {
  [IO.File]::WriteAllText((Join-Path $upstreamProject 'README.md'),$change)
  Invoke-FixtureGit $upstreamProject @('add','.')
  Invoke-FixtureGit $upstreamProject @('commit','-m',$change)
 }
 $project=Invoke-Fixture 'projects.save' @{project=@{name='桌面验收项目';path=$localProject}}
 Invoke-Fixture 'projects.trust' @{projectId=$project.id;path=$localProject;trusted=$true;confirmed=$true} | Out-Null
 Invoke-Fixture 'projects.check' @{projectId=$project.id} | Out-Null
 $secondProjectPath=Join-Path $fixture 'second-project'
 Invoke-FixtureGit $fixture @('clone',$upstreamProject,$secondProjectPath)
 $secondProject=Invoke-Fixture 'projects.save' @{project=@{name='第二个验收项目';path=$secondProjectPath}}
 Invoke-Fixture 'projects.trust' @{projectId=$secondProject.id;path=$secondProjectPath;trusted=$true;confirmed=$true} | Out-Null
 Invoke-Fixture 'projects.check' @{projectId=$secondProject.id} | Out-Null
 Invoke-Fixture 'bookmarks.save' @{bookmark=@{name='桌面验收收藏';url='https://example.invalid/desktop-fixture';projectId=$project.id;status='interested'}} | Out-Null
 if($PortableMove){
  $portableBefore=Invoke-Fixture 'snapshot'
  $portableSource=(Resolve-Path -LiteralPath $portableA).Path
  $portableDestination=[IO.Path]::GetFullPath($portableB)
  if(-not $portableSource.StartsWith($fixture+'\',[StringComparison]::OrdinalIgnoreCase) -or -not $portableDestination.StartsWith($fixture+'\',[StringComparison]::OrdinalIgnoreCase) -or (Test-Path -LiteralPath $portableDestination)){throw 'Unsafe portable fixture move'}
  if(Get-ChildItem -LiteralPath $portableSource -Recurse -Force | Where-Object {($_.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0}){throw 'Portable fixture contains a link'}
  Move-Item -LiteralPath $portableSource -Destination $portableDestination
  $variables.AGENTHUB_DATA_DIR=Join-Path $portableB 'data'
  $exePath=Join-Path $portableB 'AgentHub.exe'
  $report.exe=$exePath;$report.data=$variables.AGENTHUB_DATA_DIR;$report.portableMove=$true
  # Native startup must discover B/data from its adjacent layout marker.
  Remove-Item Env:AGENTHUB_DATA_DIR
 }
 if($RecoveryFixture){
  & node (Join-Path $projectRoot 'scripts/seed-recovery-fixture.mjs') $variables.AGENTHUB_DATA_DIR
  if($LASTEXITCODE -ne 0){throw 'Recovery fixture injection failed'}
  $restricted=Invoke-Fixture 'snapshot'
  if($restricted.recovery.status -ne 'restricted'){throw 'Core did not isolate the damaged recovery journal'}
  $exported=Invoke-Fixture 'prompts.export' @{format='json'}
  if($exported.content -notlike '*桌面验收 Prompt*'){throw 'Prompt export failed during recovery'}
  Invoke-Fixture 'backups.list' | Out-Null
 }

 Add-Type -AssemblyName UIAutomationClient,UIAutomationTypes,System.Drawing
 Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class StartupWindowCapture {
 [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left,Top,Right,Bottom; }
 [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd,out Rect rect);
 [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hwnd,IntPtr dc,uint flags);
}
'@
 $timer=[Diagnostics.Stopwatch]::StartNew()
 $app=Start-Process -FilePath $exePath -WorkingDirectory (Split-Path $exePath -Parent) -WindowStyle Hidden -PassThru
 $names=@();$ready=$false
 for($attempt=0;$attempt -lt 90;$attempt++) {
  $app.Refresh();if($app.HasExited){throw 'Packaged desktop exited before rendering'}
  if($app.MainWindowHandle -ne [IntPtr]::Zero){
   $window=[System.Windows.Automation.AutomationElement]::FromHandle($app.MainWindowHandle)
   $elements=$window.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition)
   $names=@($elements | ForEach-Object {$_.Current.Name} | Where-Object {$_})
   if(($names -contains $labels.prompt) -and (@($names | Where-Object {$_.StartsWith($labels.skill+' ')}).Count -gt 0) -and (@($names | Where-Object {$_.StartsWith($labels.projects+' ')}).Count -gt 0) -and (@($names | Where-Object {$_.StartsWith($labels.settings+' ')}).Count -gt 0)){$ready=$true;break}
  }
  Start-Sleep -Milliseconds 500
 }
 $report.renderMs=$timer.ElapsedMilliseconds
 $names | Set-Content -LiteralPath (Join-Path $reportDir "startup$suffix-ui.txt") -Encoding utf8
 # Accessibility controls can precede the first composited WebView frame.
 Start-Sleep -Milliseconds 1500
 $rect=New-Object StartupWindowCapture+Rect
 if(-not [StartupWindowCapture]::GetWindowRect($app.MainWindowHandle,[ref]$rect)){throw 'Cannot read desktop bounds'}
 $bitmap=[Drawing.Bitmap]::new(($rect.Right-$rect.Left),($rect.Bottom-$rect.Top),[Drawing.Imaging.PixelFormat]::Format24bppRgb)
 $graphics=[Drawing.Graphics]::FromImage($bitmap);$dc=$graphics.GetHdc()
 try{$captured=[StartupWindowCapture]::PrintWindow($app.MainWindowHandle,$dc,2)}finally{$graphics.ReleaseHdc($dc);$graphics.Dispose()}
 try{
  if($bitmap.GetPixel([int]($bitmap.Width/2),[int]($bitmap.Height/2)).GetBrightness() -lt 0.1){throw 'Desktop frame is not painted yet'}
  $bitmap.Save((Join-Path $reportDir "startup$suffix.png"),[Drawing.Imaging.ImageFormat]::Png)
  $report.frameCapturedMs=$timer.ElapsedMilliseconds
 }finally{$bitmap.Dispose()}
 if(-not $captured){throw 'Cannot capture packaged desktop'}
 if(-not $ready){throw 'Packaged desktop did not expose the expected product navigation'}
 if($PortableMove){
  $portableAfter=Invoke-Fixture 'snapshot'
  if($portableAfter.recovery.status -ne 'ready'){throw 'Moved data entered restricted recovery unexpectedly'}
  foreach($field in @('prompts','projects','deployments')){
   if(($portableBefore.$field | ConvertTo-Json -Depth 100 -Compress) -ne ($portableAfter.$field | ConvertTo-Json -Depth 100 -Compress)){throw "Portable move changed $field"}
  }
  $movedSkill=@($portableAfter.skills | Where-Object id -eq $fixtureSkill.skill.id)[0]
  if(-not $movedSkill.path.StartsWith((Join-Path $portableB 'data/library/skills')+'\',[StringComparison]::OrdinalIgnoreCase)){throw 'Library path was not relocated'}
  if(($movedSkill.source | ConvertTo-Json -Compress) -ne ($fixtureSkill.skill.source | ConvertTo-Json -Compress)){throw 'External source path changed'}
  $portableInstall=Invoke-Fixture 'skills.install.preview' @{skillId=$fixtureSkill.skill.id;targetId='codex'}
  Invoke-Fixture 'skills.install' @{planId=$portableInstall.id;confirmed=$true} | Out-Null
  Invoke-Fixture 'skills.check' @{skillId=$fixtureSkill.skill.id} | Out-Null
  $portableExternalFile=Join-Path $portableTarget.globalPath 'desktop-fixture/SKILL.md'
  $portableExternalHash=(Get-FileHash -LiteralPath $portableExternalFile -Algorithm SHA256).Hash
  Invoke-Fixture 'backups.restore' @{id=$portableUpdated.backupId;confirmed=$true} | Out-Null
  if([IO.File]::ReadAllText((Join-Path $movedSkill.path 'SKILL.md')) -ne $portableOriginal){throw 'Moved backup did not restore original Skill'}
  if((Get-FileHash -LiteralPath $portableExternalFile -Algorithm SHA256).Hash -ne $portableExternalHash){throw 'Library restore changed the external Agent copy'}
  if(Test-Path -LiteralPath $portableA){throw 'Native startup recreated the old portable directory'}
 }
 if($RecoveryFixture){
  $recoveryLabel=if($Language -eq 'en'){'File recovery required; Skill writes are paused'}else{'部分文件需要恢复，Skill 写入已暂停'}
  if($names -notcontains $recoveryLabel){throw 'Native desktop did not show the restricted recovery state'}
 }
 function Save-SmokeScreen([string]$Name) {
  $shot=[Drawing.Bitmap]::new(($rect.Right-$rect.Left),($rect.Bottom-$rect.Top),[Drawing.Imaging.PixelFormat]::Format24bppRgb)
  $canvas=[Drawing.Graphics]::FromImage($shot);$context=$canvas.GetHdc()
  try{[void][StartupWindowCapture]::PrintWindow($app.MainWindowHandle,$context,2)}finally{$canvas.ReleaseHdc($context);$canvas.Dispose()}
  try{$shot.Save((Join-Path $reportDir ($Name+$suffix+'.png')),[Drawing.Imaging.ImageFormat]::Png)}finally{$shot.Dispose()}
 }
 $previewLabel=if($Language -eq 'en'){'View 桌面验收 Prompt'}else{'查看 桌面验收 Prompt'}
 $copyLabel=if($Language -eq 'en'){'Copy body'}else{'复制正文'}
 $closeLabel=if($Language -eq 'en'){'Close'}else{'关闭'}
 $buttons=$window.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::ControlTypeProperty,[System.Windows.Automation.ControlType]::Button))
 $preview=@($buttons | Where-Object {$_.Current.Name -eq $previewLabel})[0]
 if(-not $preview){throw 'Prompt preview button missing'}
 $preview.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
 Start-Sleep -Milliseconds 500
 $previewElements=$window.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition)
 $previewNames=@($previewElements | ForEach-Object {$_.Current.Name})
 if(($previewNames -notcontains $copyLabel) -or ($previewNames -notcontains '确认浮窗阅读和复制正文')){throw 'Prompt full-text preview or purpose missing'}
 Save-SmokeScreen 'prompt-preview'
 $close=@($previewElements | Where-Object {$_.Current.Name -eq $closeLabel -and $_.Current.ControlType -eq [System.Windows.Automation.ControlType]::Button})[0]
 $close.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
 $report.pages=@($labels.prompt)
 foreach($page in @(@{name=$labels.skill;expected='desktop-fixture';file='skill'},@{name=$labels.projects;expected='桌面验收项目';file='projects'},@{name=$labels.settings;expected=$labels.locations;file='settings'})) {
  $buttons=$window.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::ControlTypeProperty,[System.Windows.Automation.ControlType]::Button))
  $button=@($buttons | Where-Object {$_.Current.Name.StartsWith($page.name)})[0]
  if(-not $button){throw "Navigation button missing: $($page.name)"}
  $invoke=$button.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern);$invoke.Invoke()
  $pageReady=$false
  for($attempt=0;$attempt -lt 30;$attempt++) {
   Start-Sleep -Milliseconds 200
   $elements=$window.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition)
   $pageNames=@($elements | ForEach-Object {$_.Current.Name} | Where-Object {$_})
   if($pageNames -contains $page.expected){$pageReady=$true;break}
  }
  $pageNames | Set-Content -LiteralPath (Join-Path $reportDir ($page.file+$suffix+'-ui.txt')) -Encoding utf8
  if(-not $pageReady){throw "Page did not render persisted content: $($page.name)"}
  $bitmap=[Drawing.Bitmap]::new(($rect.Right-$rect.Left),($rect.Bottom-$rect.Top),[Drawing.Imaging.PixelFormat]::Format24bppRgb)
  $graphics=[Drawing.Graphics]::FromImage($bitmap);$dc=$graphics.GetHdc()
  try{[void][StartupWindowCapture]::PrintWindow($app.MainWindowHandle,$dc,2)}finally{$graphics.ReleaseHdc($dc);$graphics.Dispose()}
  try{$bitmap.Save((Join-Path $reportDir ($page.file+$suffix+'.png')),[Drawing.Imaging.ImageFormat]::Png)}finally{$bitmap.Dispose()}
  if($page.file -eq 'projects' -and -not $RecoveryFixture) {
   $firstCheckLabel=if($Language -eq 'en'){'Check upstream for 桌面验收项目'}else{'检查上游 桌面验收项目'}
   $secondCheckLabel=if($Language -eq 'en'){'Check upstream for 第二个验收项目'}else{'检查上游 第二个验收项目'}
   $firstCheck=@($elements | Where-Object {$_.Current.Name -eq $firstCheckLabel})[0]
   $secondCheck=@($elements | Where-Object {$_.Current.Name -eq $secondCheckLabel})[0]
   if(-not $firstCheck -or -not $secondCheck){throw 'Project check control missing'}
   if([math]::Abs($firstCheck.Current.BoundingRectangle.Left-$secondCheck.Current.BoundingRectangle.Left) -gt 1){throw 'Project check buttons do not align across rows'}

   $detailLabel=if($Language -eq 'en'){'Upstream check details for 桌面验收项目'}else{'上游检查详情 桌面验收项目'}
   $detailsButton=@($elements | Where-Object {$_.Current.Name -eq $detailLabel -and $_.Current.ControlType -eq [System.Windows.Automation.ControlType]::Button})[0]
   if(-not $detailsButton){throw 'Project details button missing'}
   $detailsButton.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
   $detailsReady=$false
   for($attempt=0;$attempt -lt 100;$attempt++) {
    Start-Sleep -Milliseconds 200
    $detailsElements=$window.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition)
    $detailsNames=@($detailsElements | ForEach-Object {$_.Current.Name})
    if(@($detailsNames | Where-Object {$_ -like '*latest change'}).Count -gt 0){$detailsReady=$true;break}
   }
   $detailsNames | Set-Content -LiteralPath (Join-Path $reportDir ('upstream-details'+$suffix+'-ui.txt')) -Encoding utf8
   Save-SmokeScreen 'upstream-details'
   if(-not $detailsReady){throw 'Local upstream details failed to load'}
   $leadingLabel=if($Language -eq 'en'){'Upstream is 2 commits ahead of local'}else{'上游领先本地 2 个提交'}
   if($detailsNames -notcontains $leadingLabel){throw 'Upstream count missing'}
   Save-SmokeScreen 'upstream-details'
   $close=@($detailsElements | Where-Object {$_.Current.Name -eq $closeLabel -and $_.Current.ControlType -eq [System.Windows.Automation.ControlType]::Button})[0]
   $close.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
   $elements=$window.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition)

   $bookmarkTabLabel=if($Language -eq 'en'){'Saved repos'}else{'仓库收藏'}
   $bookmarkSortLabel=if($Language -eq 'en'){'Sort bookmarks'}else{'收藏排序'}
   $tab=@($elements | Where-Object {$_.Current.Name -eq $bookmarkTabLabel -and $_.Current.ControlType -eq [System.Windows.Automation.ControlType]::TabItem})[0]
   if(-not $tab){throw 'Bookmarks tab missing'}
   $tab.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Select()
   $bookmarkReady=$false
   for($attempt=0;$attempt -lt 40;$attempt++) {
    Start-Sleep -Milliseconds 200
    $bookmarkNames=@($window.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition) | ForEach-Object {$_.Current.Name})
    if(($bookmarkNames -contains '桌面验收收藏') -and ($bookmarkNames -contains $bookmarkSortLabel)){$bookmarkReady=$true;break}
   }
   if(-not $bookmarkReady){throw 'Bookmark or sort missing'}
   Save-SmokeScreen 'bookmarks'
  }
  $report.pages+=$page.name
 }
 $aboutLabel=if($Language -eq 'en'){'About'}else{'关于'}
 $languageLabel=if($Language -eq 'en'){'Language'}else{'界面语言'}
 $elements=$window.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition)
 $about=@($elements | Where-Object {$_.Current.Name -eq $aboutLabel -and $_.Current.ControlType -eq [System.Windows.Automation.ControlType]::TabItem})[0]
 if(-not $about){throw 'About tab missing'}
 $about.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern).Select()
 Start-Sleep -Milliseconds 500
 $aboutNames=@($window.FindAll([System.Windows.Automation.TreeScope]::Descendants,[System.Windows.Automation.Condition]::TrueCondition) | ForEach-Object {$_.Current.Name})
 if(($aboutNames -notcontains ('AgentHub '+$version)) -or ($aboutNames -notcontains $languageLabel)){throw 'About version or language setting missing'}
 $bitmap=[Drawing.Bitmap]::new(($rect.Right-$rect.Left),($rect.Bottom-$rect.Top),[Drawing.Imaging.PixelFormat]::Format24bppRgb)
 $graphics=[Drawing.Graphics]::FromImage($bitmap);$dc=$graphics.GetHdc()
 try{[void][StartupWindowCapture]::PrintWindow($app.MainWindowHandle,$dc,2)}finally{$graphics.ReleaseHdc($dc);$graphics.Dispose()}
 try{$bitmap.Save((Join-Path $reportDir ("about"+$suffix+".png")),[Drawing.Imaging.ImageFormat]::Png)}finally{$bitmap.Dispose()}
 $aboutNames | Set-Content -LiteralPath (Join-Path $reportDir ("about"+$suffix+"-ui.txt")) -Encoding utf8
 $report.status='passed';$report.checks=@('Packaged custom-protocol UI loaded without Vite','Four pages navigate and render persisted core data','Version matches packaged release','Fresh isolated data and configured targets only','Prompt preview with purpose and copy control','Bookmark sort and About language controls','Cached upstream count and latest-first details','Project check buttons align across different status rows')
 if($RecoveryFixture){$report.checks=@('Native desktop opens with a damaged recovery journal','Restricted banner remains visible across four pages','Prompt preview and About settings remain available','Core Prompt export and backup listing succeed in restricted mode');$report.recoveryFixture=$true}
 if($PortableMove){$report.checks+=@('Used portable application and data moved together from A to B','Native startup discovers B/data without AGENTHUB_DATA_DIR','Prompts, projects, deployments and external source paths preserved','Existing Skill installs and checks again after native startup','Moved backup restores the library without changing the external Agent copy')}
} catch {$report.status='failed';$report.error=$_.ToString();throw}
finally {
 if($app -and -not $app.HasExited){& taskkill.exe /PID $app.Id /T /F | Out-Null}
 foreach($key in $previous.Keys){[Environment]::SetEnvironmentVariable($key,$previous[$key])}
 $report | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $projectRoot ('output/desktop-verification-'+$version+$suffix+'.json')) -Encoding utf8
 $report | ConvertTo-Json -Depth 8
}
