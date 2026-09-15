param(
    [ValidateSet('PullRequest','Release')][string]$Profile='PullRequest',
    [string]$CandidatePath,
    [string]$BaseVersion,
    [string]$VmConfig,
    [string]$OutputRoot='output/acceptance',
    [switch]$DryRun,
    [switch]$ListScenarios
)
$ErrorActionPreference = 'Stop'
$project = Split-Path $PSScriptRoot -Parent
. (Join-Path $PSScriptRoot 'acceptance/common.ps1')
$catalog = @([IO.File]::ReadAllText((Join-Path $PSScriptRoot 'acceptance/scenarios.json')) | ConvertFrom-Json | Where-Object { $_.profile -eq $Profile })
if ($ListScenarios) { $catalog | Format-Table id,coverage -Wrap; exit 0 }
$run = 'a-' + [DateTime]::UtcNow.ToString('yyyyMMdd-HHmmss') + '-' + [guid]::NewGuid().ToString('N').Substring(0,8)
if (-not [IO.Path]::IsPathRooted($OutputRoot)) { $OutputRoot = Join-Path $project $OutputRoot }
$directory = Assert-AcceptancePath (Join-Path $OutputRoot $run) $OutputRoot
if (Test-Path -LiteralPath $directory) { throw 'Evidence directory already exists' }
New-Item -ItemType Directory -Path $directory,(Join-Path $directory 'logs'),(Join-Path $directory 'screenshots') | Out-Null
$report = [ordered]@{schemaVersion=1; runId=$run; profile=$Profile; sourceCommit=$null; workingTreeClean=$null; startedAt=[DateTime]::UtcNow.ToString('o'); finishedAt=$null; scenarios=@(); dryRun=[bool]$DryRun}
$cleanup = [ordered]@{status='passed'; notes=@('No host system settings changed'); errors=@()}
$before = $null
$oldEvidence = $env:AGENTHUB_ACCEPTANCE_OUTPUT
$uiBaseline=$null
$uiPortWasBusy=$false
Write-AcceptanceJson @{before=$null; after=$null; unchanged=$null} (Join-Path $directory 'candidate-hashes.json')
Write-AcceptanceJson $cleanup (Join-Path $directory 'cleanup.json')
Write-AcceptanceJson @{computer=$env:COMPUTERNAME; user=$env:USERNAME; os=[Environment]::OSVersion.VersionString; powershell=$PSVersionTable.PSVersion.ToString(); kind='host'; buildRoot=$env:AGENTHUB_BUILD_ROOT; rustupToolchain=$env:RUSTUP_TOOLCHAIN; node=$(if(Get-Command node -ErrorAction SilentlyContinue){(& node --version) -join ''}else{'unavailable'}); edge=$(if(Test-Path -LiteralPath 'C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe'){(Get-Item -LiteralPath 'C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe').VersionInfo.ProductVersion}else{'not detected at standard path'})} (Join-Path $directory 'environment.json')
function Run-Check([string]$Id, [scriptblock]$Command) {
    $start = [DateTime]::UtcNow.ToString('o')
    $log = Join-Path $directory "logs/$Id.log"
    Write-Host "[$Id] starting"
    try {
        & $Command *> $log
        if ($LASTEXITCODE -ne 0) { throw "Command exit code $LASTEXITCODE; see logs/$Id.log" }
        $report.scenarios += New-AcceptanceResult $Id 'passed' 'Completed' $start @("logs/$Id.log")
    } catch { $report.scenarios += New-AcceptanceResult $Id 'failed' $_.ToString() $start @("logs/$Id.log") }
    Write-Host "[$Id] $($report.scenarios[-1].status)"
    Write-AcceptanceReport $report $directory
}
Push-Location $project
try {
    $report.sourceCommit = (& git rev-parse HEAD).Trim()
    if ($LASTEXITCODE -ne 0) { throw 'Cannot identify harness source commit' }
    $status = @(& git status --porcelain)
    if ($LASTEXITCODE -ne 0) { throw 'Cannot read working tree status' }
    $report.workingTreeClean = $status.Count -eq 0
    [IO.File]::WriteAllLines((Join-Path $directory 'logs/working-tree.txt'), [string[]]$status)
    if ($DryRun) {
        foreach ($scenario in $catalog) { $report.scenarios += New-AcceptanceResult $scenario.id 'not-run' ('Dry run: '+$scenario.coverage) $report.startedAt }
    } elseif ($Profile -eq 'PullRequest') {
        Run-Check 'rust' { & pwsh -NoProfile -File scripts/test.ps1 -Stage Rust }
        Run-Check 'frontend' { & pwsh -NoProfile -File scripts/test.ps1 -Stage Frontend }
        Run-Check 'production-build' { & pwsh -NoProfile -File scripts/test.ps1 -Stage Build }
        $env:AGENTHUB_ACCEPTANCE_OUTPUT = $directory
        $uiBaseline=@(Get-CimInstance Win32_Process -Filter "Name='msedge.exe'" | Where-Object { $_.CommandLine -like '*playwright_chromiumdev_profile*' } | Select-Object -ExpandProperty ProcessId)
        $uiPortWasBusy=[bool](Get-NetTCPConnection -LocalPort 1437 -State Listen -ErrorAction SilentlyContinue)
        Run-Check 'ui-smoke' { & node node_modules/@playwright/test/cli.js test }
    } else {
        if (-not $CandidatePath) { throw 'Release requires CandidatePath' }
        $CandidatePath = [IO.Path]::GetFullPath($CandidatePath)
        $before = Get-CandidateProof $CandidatePath
        $report.candidateSourceCommit = $before.sourceCommit
        $report.candidateVersion = $before.version
        Write-AcceptanceJson @{before=$before; after=$null; unchanged=$null} (Join-Path $directory 'candidate-hashes.json')
        $report.scenarios += New-AcceptanceResult 'candidate-integrity' 'passed' 'Manifest and checksums match; CI success and public provenance remain separately reviewable' $report.startedAt @('candidate-hashes.json')
        if ($VmConfig) {
            . (Join-Path $PSScriptRoot 'acceptance/transport.ps1')
            $config = [IO.File]::ReadAllText([IO.Path]::GetFullPath($VmConfig)) | ConvertFrom-Json
            Invoke-AcceptanceVm $config $directory $run $CandidatePath $BaseVersion
            $vmReport = [IO.File]::ReadAllText((Join-Path $directory 'vm/result.json')) | ConvertFrom-Json
            $report.scenarios += @($vmReport.scenarios)
            $cleanup.vm = $vmReport.cleanup
            if ($vmReport.cleanup.status -ne 'passed') { $cleanup.status='failed' }
        } else {
            foreach ($id in @('cleanup-failure-probe','installer-lifecycle','portable-relocation','upgrade','installer-languages','cleanup')) { $report.scenarios += New-AcceptanceResult $id 'environment-blocked' 'Supply VmConfig for the authorized TEST/Try VM; no installer runs on this host' $report.startedAt }
        }
    }
} catch {
    $report.scenarios += New-AcceptanceResult 'runner' 'failed' ($_.ToString()+' '+$_.ScriptStackTrace) $report.startedAt
} finally {
    $env:AGENTHUB_ACCEPTANCE_OUTPUT = $oldEvidence
    if ($null -ne $uiBaseline) {
        try {
            $remaining=@(Get-CimInstance Win32_Process -Filter "Name='msedge.exe'" | Where-Object { $_.CommandLine -like '*playwright_chromiumdev_profile*' -and $_.ProcessId -notin $uiBaseline } | Select-Object ProcessId,ExecutablePath)
            $serverLeft=(-not $uiPortWasBusy) -and [bool](Get-NetTCPConnection -LocalPort 1437 -State Listen -ErrorAction SilentlyContinue)
            $cleanup.ui=@{remainingNewPlaywrightProcesses=$remaining;newServerLeft=$serverLeft}
            if ($remaining.Count -or $serverLeft) { throw 'UI processes or Vite listener remain; preserve unrelated processes and inspect cleanup.json' }
        } catch { $cleanup.status='failed';$cleanup.errors+=$_.ToString();$report.scenarios+=New-AcceptanceResult 'ui-cleanup' 'failed' $_.ToString() $report.startedAt @('cleanup.json') }
    }
    if ($before) {
        try {
            $after = Get-CandidateProof $CandidatePath
            $same = ($before | ConvertTo-Json -Depth 30 -Compress) -eq ($after | ConvertTo-Json -Depth 30 -Compress)
            Write-AcceptanceJson @{before=$before; after=$after; unchanged=$same} (Join-Path $directory 'candidate-hashes.json')
            if (-not $same) { throw 'Candidate bytes changed during acceptance' }
            $report.scenarios += New-AcceptanceResult 'candidate-unchanged' 'passed' 'All four candidate files unchanged' $report.startedAt @('candidate-hashes.json')
        } catch { $report.scenarios += New-AcceptanceResult 'candidate-unchanged' 'failed' $_.ToString() $report.startedAt }
    }
    foreach ($scenario in $catalog) {
        if ($scenario.id -notin @($report.scenarios.id)) {
            $reason = switch ($scenario.id) {
                'webview-missing' { 'Requires operator-prepared runtime-free VM snapshot; this runner never removes a shared system runtime' }
                'webview-download-failure-recovery' { 'Requires dedicated runtime-free VM and operator-controlled network failure/recovery; see manual runbook' }
                'browser-download-startup' { 'Requires real browser download and operator handling of publisher/reputation prompts; see manual runbook' }
                'native-ui-details' { 'Requires packaged WebView2 interaction review; fixture smoke does not establish native GUI acceptance' }
                default { 'Prerequisite or earlier setup failed; scenario was not executed' }
            }
            $report.scenarios += New-AcceptanceResult $scenario.id 'not-run' $reason $report.startedAt
        }
    }
    if (Test-Path -LiteralPath (Join-Path $directory 'transport-cleanup.json')) {
        $cleanup.transport=[IO.File]::ReadAllText((Join-Path $directory 'transport-cleanup.json')) | ConvertFrom-Json
        if ($cleanup.transport.status -ne 'passed') { $cleanup.status='failed'; $report.scenarios+=New-AcceptanceResult 'transport-cleanup' 'failed' $cleanup.transport.error $report.startedAt @('transport-cleanup.json') }
    }
    Write-AcceptanceJson $cleanup (Join-Path $directory 'cleanup.json')
    Write-AcceptanceReport $report $directory
    Pop-Location
}
Write-Host "Acceptance: $directory ($($report.status), exit $($report.exitCode))"
exit $report.exitCode
