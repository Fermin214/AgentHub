# Shared host/VM reporting and validation. Windows PowerShell 5.1 compatible.
$ErrorActionPreference = 'Stop'
function Invoke-AcceptanceProcess([string]$Executable, [string[]]$Arguments, [string]$Log) {
    if (-not (Get-Command $Executable -ErrorAction SilentlyContinue)) { throw "Executable unavailable: $Executable" }
    $previousAction=$ErrorActionPreference
    try {
        # Windows PowerShell maps native stderr to ErrorRecord. Inspect the actual
        # exit code after redirection, including deliberately failing self-tests.
        $ErrorActionPreference='Continue'
        & $Executable @Arguments *> $Log
        return $LASTEXITCODE
    } finally { $ErrorActionPreference=$previousAction }
}
function Write-AcceptanceJson($Value, [string]$Path) {
    [IO.File]::WriteAllText($Path, ($Value | ConvertTo-Json -Depth 80), (New-Object Text.UTF8Encoding($false)))
}
function Assert-AcceptancePath([string]$Path, [string]$Parent) {
    $full = [IO.Path]::GetFullPath($Path)
    $boundary = [IO.Path]::GetFullPath($Parent).TrimEnd('\', '/')
    if ($full -ne $boundary -and -not $full.StartsWith($boundary + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw "Path escapes owned directory: $full" }
    $cursor = $full
    while ($cursor) {
        if ((Test-Path -LiteralPath $cursor) -and ((Get-Item -LiteralPath $cursor -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw "Linked path is not allowed: $cursor" }
        $cursor = Split-Path $cursor -Parent
    }
    return $full
}
function Get-CandidateProof([string]$Directory) {
    $root = Assert-AcceptancePath $Directory $Directory
    $manifestPath = Assert-AcceptancePath (Join-Path $root 'build-manifest.json') $root
    $manifest = [IO.File]::ReadAllText($manifestPath) | ConvertFrom-Json
    if ($manifest.product -ne 'AgentHub' -or $manifest.sourceCommit -notmatch '^[a-f0-9]{40}$' -or $manifest.version -notmatch '^\d+\.\d+\.\d+$') { throw 'Invalid candidate provenance' }
    $expectedNames = @("AgentHub_$($manifest.version)_x64-setup.exe", "AgentHub-$($manifest.version)-windows-x64.zip")
    if (@($manifest.files).Count -ne 2 -or @($manifest.files.name | Select-Object -Unique).Count -ne 2) { throw 'Expected two distinct distribution entries' }
    $hashes = @()
    foreach ($name in @($expectedNames + 'build-manifest.json' + 'SHA256SUMS.txt')) {
        $path = Assert-AcceptancePath (Join-Path $root $name) $root
        $file = Get-Item -LiteralPath $path
        $hashes += [ordered]@{name=$name; bytes=$file.Length; sha256=(Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()}
    }
    foreach ($entry in $manifest.files) {
        if ($entry.name -cnotin $expectedNames) { throw 'Unexpected candidate entry' }
        $actual = @($hashes | Where-Object { $_.name -ceq $entry.name })
        if ($actual.Count -ne 1 -or $actual[0].sha256 -ne $entry.sha256 -or $actual[0].bytes -ne $entry.bytes) { throw "Candidate mismatch: $($entry.name)" }
    }
    $lines = @([IO.File]::ReadAllLines((Join-Path $root 'SHA256SUMS.txt')) | Where-Object { $_.Trim() })
    if ($lines.Count -ne 3) { throw 'Expected three checksum entries' }
    $seen = @{}
    foreach ($line in $lines) {
        if ($line -notmatch '^([a-fA-F0-9]{64})  ([^/\\]+)$') { throw 'Invalid checksum line' }
        $hash = $Matches[1]; $name = $Matches[2]
        if ($seen.ContainsKey($name) -or $name -eq 'SHA256SUMS.txt') { throw 'Duplicate or unexpected checksum entry' }
        $seen[$name] = $true
        $actual = @($hashes | Where-Object { $_.name -ceq $name })
        if ($actual.Count -ne 1 -or $actual[0].sha256 -ne $hash) { throw "Checksum mismatch: $name" }
    }
    if (@($seen.Keys | Where-Object { $_ -in @($expectedNames + 'build-manifest.json') }).Count -ne 3) { throw 'Incomplete candidate checksum coverage' }
    return [ordered]@{sourceCommit=$manifest.sourceCommit; version=$manifest.version; ciRunId=$manifest.ciRunId; repository=$manifest.repository; files=$hashes}
}
function Get-AcceptanceExitCode($Scenarios) {
    if (@($Scenarios).Count -eq 0) { return 2 }
    if (@($Scenarios | Where-Object { $_.status -eq 'failed' }).Count) { return 1 }
    if (@($Scenarios | Where-Object { $_.status -ne 'passed' }).Count) { return 2 }
    return 0
}
# Host-only (PowerShell 7). Own the child tree and bound its lifetime; a timeout
# is a failure even if the child happened to exit successfully during termination.
function Invoke-AcceptanceTimedProcess([string]$Executable, [string[]]$Arguments, [string]$Log, [int]$TimeoutSeconds=900) {
    $info = [Diagnostics.ProcessStartInfo]::new((Get-Command $Executable -ErrorAction Stop).Source)
    $info.UseShellExecute=$false; $info.CreateNoWindow=$true
    $info.RedirectStandardOutput=$true; $info.RedirectStandardError=$true
    $info.StandardOutputEncoding=[Text.Encoding]::UTF8; $info.StandardErrorEncoding=[Text.Encoding]::UTF8
    foreach ($argument in $Arguments) { $info.ArgumentList.Add($argument) }
    $process = [Diagnostics.Process]::new(); $process.StartInfo=$info
    try {
        if (-not $process.Start()) { throw 'Child did not start' }
        $stdout=$process.StandardOutput.ReadToEndAsync(); $stderr=$process.StandardError.ReadToEndAsync()
        $timedOut=-not $process.WaitForExit($TimeoutSeconds*1000)
        if ($timedOut) { $process.Kill($true); $process.WaitForExit() }
        [IO.File]::WriteAllText($Log, $stdout.GetAwaiter().GetResult()+$stderr.GetAwaiter().GetResult())
        if ($timedOut) { [IO.File]::AppendAllText($Log,"`nACCEPTANCE_TIMEOUT after $TimeoutSeconds seconds"); throw "ACCEPTANCE_TIMEOUT: $Log" }
        if ($process.ExitCode -ne 0) { throw "Command exit code $($process.ExitCode); see $Log" }
    } finally { $process.Dispose() }
}
function Assert-NativeAcceptanceResult($Record, [int]$ExitCode, [string]$Case, [switch]$ExpectedFailure) {
    if ($Record.case -ne $Case) { throw 'Native evidence case ID mismatch' }
    if ($ExpectedFailure) {
        if ($ExitCode -eq 0 -or $Record.status -ne 'failed' -or $Record.error -ne 'ACCEPTANCE_EXPECTED_FAILURE_AFTER_INSTALL' -or $Record.cleanup.status -ne 'passed') { throw 'Expected controlled failure with successful finally cleanup' }
    } elseif ($ExitCode -ne 0 -or $Record.status -ne 'passed') { throw "Native scenario did not pass (exit $ExitCode)" }
}
function New-AcceptanceResult([string]$Id, [string]$Status, [string]$Reason, [string]$StartedAt, $Evidence=@()) {
    if ($Status -notin @('passed','failed','not-run','environment-blocked')) { throw 'Invalid scenario status' }
    return [ordered]@{id=$Id; status=$Status; reason=$Reason; startedAt=$StartedAt; finishedAt=[DateTime]::UtcNow.ToString('o'); evidence=@($Evidence)}
}
function Write-AcceptanceReport($Report, [string]$Directory) {
    $Report.finishedAt = [DateTime]::UtcNow.ToString('o')
    $Report.exitCode = Get-AcceptanceExitCode $Report.scenarios
    if ($Report.exitCode -eq 0 -and @($Report.requiredScenarios | Where-Object { $_ -notin @($Report.scenarios.id) }).Count) { $Report.exitCode=2 }
    $Report.status = if ($Report.exitCode -eq 0) {'passed'} elseif ($Report.exitCode -eq 1) {'failed'} else {'incomplete'}
    Write-AcceptanceJson $Report (Join-Path $Directory 'acceptance.json')
    $lines = @('# AgentHub acceptance', '', "Profile: $($Report.profile)", "Status: **$($Report.status)** (exit $($Report.exitCode))", "Harness commit: $($Report.sourceCommit)", "Working tree clean: $($Report.workingTreeClean)", "Started: $($Report.startedAt)", "Finished: $($Report.finishedAt)", '', '| Scenario | Status | Reason |', '| --- | --- | --- |')
    foreach ($row in $Report.scenarios) { $lines += "| $($row.id) | $($row.status) | $(($row.reason -replace '\|','/' -replace '[\r\n]+',' ')) |" }
    $layer = switch ($Report.profile) {
        'PullRequest' { 'Rust / component / simulated IPC browser. Not native desktop or release-package acceptance.' }
        'DevelopmentDesktop' { 'Development Tauri / real WebView2 / real Rust persistence. Explicit IPC faults are not physical disk failures. Not release-package acceptance.' }
        'Release' { 'Immutable packaged candidate; manual release scenarios remain required.' }
    }
    $lines += @('', "Layer: $layer", "Candidate source: $($Report.candidateSourceCommit)", '', '## Follow-up')
    $pending=@($Report.scenarios | Where-Object status -ne 'passed')
    $missing=@($Report.requiredScenarios | Where-Object { $_ -notin @($Report.scenarios.id) })
    if ($Report.exitCode -eq 0) { $lines+='No automated blockers in this selected profile.' }
    if (-not @($Report.scenarios).Count) { $lines+='No scenarios executed; acceptance is incomplete.' }
    foreach($id in $missing){$lines+="- ${id}: not-run — required result missing"}
    foreach ($row in $pending) { $lines+="- $($row.id): $($row.status) — $($row.reason -replace '[\r\n]+',' ')" }
    $lines+='Product review: inspect the bilingual minimum-window screenshots for legibility and preferred spacing. Automated geometry does not decide visual preference.'
    $screens=@(Get-ChildItem -LiteralPath (Join-Path $Directory 'screenshots') -Filter '*.png' -Recurse -ErrorAction SilentlyContinue | Sort-Object @{Expression={if($_.Name -match 'preview.*860|skills-en-860|favorite-pending|restart|detail-en-860'){0}else{1}}},Name | Select-Object -First 6)
    if ($screens.Count) { $lines+=@('', 'Key screenshots:'); foreach($shot in $screens){$relative=$shot.FullName.Substring($Directory.Length+1).Replace('\','/');$lines+="- [$($shot.Name)]($relative)"} }
    $lines += @('', 'See acceptance.json for complete timestamps and evidence. Unrun/blocked scenarios are not passed. This command never publishes a release.')
    [IO.File]::WriteAllLines((Join-Path $Directory 'acceptance.md'), [string[]]$lines, (New-Object Text.UTF8Encoding($false)))
}
