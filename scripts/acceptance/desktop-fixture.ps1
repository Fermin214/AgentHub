param([string]$Cli,[string]$CaseRoot)
$ErrorActionPreference='Stop'
[Console]::OutputEncoding=[Text.UTF8Encoding]::new($false)
$OutputEncoding=[Console]::OutputEncoding
. (Join-Path $PSScriptRoot 'common.ps1')
$caseRoot=Assert-AcceptancePath $CaseRoot $CaseRoot
if (Test-Path -LiteralPath $caseRoot) { throw 'Fixture root must be new' }
New-Item -ItemType Directory -Path $caseRoot | Out-Null
$dataRoot=Join-Path $caseRoot 'data'
function Write-Json($Value,[string]$Path){Write-AcceptanceJson $Value $Path}
function Call-Core([string]$Method, $Arguments = @{}) {
  $request = Join-Path $caseRoot 'request.json'
  Write-Json $Arguments $request
  $raw = & $cli --data-dir $dataRoot call $Method --args-file $request --apply
  if ($LASTEXITCODE -ne 0) { throw "Core call failed: $Method $raw" }
  ($raw -join "`n") | ConvertFrom-Json
}
function Assert-Owned([string]$Path) {
  $full = [IO.Path]::GetFullPath($Path)
  if (-not $full.StartsWith($caseRoot + '\', [StringComparison]::OrdinalIgnoreCase)) { throw "Path outside fixture: $full" }
  return $full
}
Call-Core 'settings.save' @{settings=@{scanRoots=@();language='zh'}} | Out-Null
Call-Core 'maintenance.save' @{automaticChecks=$false;retainUpdateBackup=$true} | Out-Null
foreach ($target in (Call-Core 'targets.list').targets) {
  $target.globalPath = Join-Path $caseRoot ('agents/' + $target.id + '/skills')
  $target.enabled = ($target.id -eq 'codex')
  Call-Core 'targets.save' @{target=$target} | Out-Null
}
$titles = @(
  '短标题：整理会议记录',
  ('这是自动验收用的中文长标题请检查标题与收藏编辑删除按钮是否同排且换行不会遮挡按钮' * 3),
  'Summarize the incident timeline and every unresolved question for each stakeholder while preserving the original wording and making the next action clear',
  ('UnbrokenLongTitleForWrapping' * 12)
)
foreach ($title in $titles) {
  $saved=Call-Core 'prompts.save' @{prompt=@{title=$title;body="自动验收正文。`nSecond line for preview and keyboard tests.";purpose='自动验收 / Native acceptance';category='Acceptance';tags=@('test');favorite=$false}}
  if(-not $saved.id -or $saved.title -ne $title -or -not $saved.updatedAt -or $saved.PSObject.Properties.Name -contains 'prompts'){throw 'prompts.save must return the saved Prompt, not a snapshot'}
}
$skills = @()
foreach ($name in @('acceptance-writer','acceptance-second')) {
  $source = Join-Path $caseRoot ('sources/' + $name)
  New-Item -ItemType Directory -Path (Join-Path $source 'references') -Force | Out-Null
  $body = "---`nname: $name`ndescription: Fictional manual acceptance Skill`n---`n# LIBRARY_COPY — 库副本正文`n`n" + ((1..160 | ForEach-Object { "第 $_ 行：这是库中的虚构长正文，用于独立滚动和 PageDown/PageUp 验收。" }) -join "`n")
  Set-Content -LiteralPath (Join-Path $source 'SKILL.md') -Value $body -Encoding utf8
  1..24 | ForEach-Object { Set-Content -LiteralPath (Join-Path $source "references/note-$_.md") -Value "# Reference $_`nFictional reference content." -Encoding utf8 }
  $inspection = Call-Core 'sources.inspect' @{source=@{kind='local';locator=$source}}
  $added = Call-Core 'skills.add' @{inspectionId=$inspection.inspectionId;subpath=$inspection.candidates[0].subpath}
  $skills += $added.skill
}
$plan = Call-Core 'skills.install.preview' @{skillId=$skills[0].id;targetId='codex'}
if (-not $plan.canExecute) { throw "Fixture installation blocked: $($plan.blockedReason)" }
$installed = Call-Core 'skills.install' @{planId=$plan.id;confirmed=$true}
if ($installed.status -ne 'succeeded') { throw 'Fixture install failed' }
$snapshot = Call-Core 'snapshot'
$deployment = @($snapshot.deployments | Where-Object { $_.skillId -eq $skills[0].id -and $_.agent -eq 'codex' })[0]
if (-not $deployment) { throw 'Fixture deployment missing' }
$installedFile = Assert-Owned (Join-Path $deployment.path 'SKILL.md')
Set-Content -LiteralPath $installedFile -Value "---`nname: acceptance-writer`ndescription: Fictional installed copy`n---`n# INSTALLED_COPY — 安装副本正文`nThis text must not appear in the library viewer." -Encoding utf8
foreach ($skill in $skills) { Call-Core 'skills.check' @{skillId=$skill.id} | Out-Null }
$snapshot = Call-Core 'snapshot'
foreach ($target in (Call-Core 'targets.list').targets) { Assert-Owned $target.globalPath | Out-Null }
$libraryRead = Call-Core 'skills.read' @{skillId=$skills[0].id;path='SKILL.md'}
$installedRead = Call-Core 'skills.read' @{skillId=$skills[0].id;deploymentId=$deployment.id;path='SKILL.md'}
if ($libraryRead.content -notmatch 'LIBRARY_COPY' -or $installedRead.content -notmatch 'INSTALLED_COPY') { throw 'Different-copy evidence was not seeded correctly' }

# Check real dispatch output, independently of the browser IPC fixture.
$first=$skills[0].id
$tagged=Call-Core 'skills.metadata.save' @{ids=@($first);tags=@(' kept ','kept','')}
if (@($tagged.skills).Count -ne 1 -or $tagged.skills[0].id -ne $first -or ($tagged.skills[0].tags -join ',') -ne 'kept') { throw 'Metadata subset/normalization contract' }
$favorited=Call-Core 'skills.metadata.save' @{ids=@($first);favorite=$true}
if (@($favorited.skills).Count -ne 1 -or -not $favorited.skills[0].favorite -or ($favorited.skills[0].tags -join ',') -ne 'kept') { throw 'Favorite-only patch lost tags' }
$retagged=Call-Core 'skills.metadata.save' @{ids=@($first);tags=@('updated')}
if (-not $retagged.skills[0].favorite) { throw 'Tags-only patch lost favorite' }
Call-Core 'skills.metadata.save' @{ids=@($first);tags=@();favorite=$false} | Out-Null
$final=Call-Core 'snapshot'
if (@($final.skills).Count -ne 2 -or $final.skills[1].favorite) { throw 'Unrelated row changed' }
if (@($final.prompts).Count -ne 4 -or -not $final.prompts[0].id -or -not $final.prompts[0].updatedAt) { throw 'Prompt save contract' }
Write-Json @{dataDir=$dataRoot;root=$caseRoot;skills=$skills;contract='real Rust dispatch: subset response, field patches, normalization, Prompt persistence'} (Join-Path $caseRoot 'session.json')
