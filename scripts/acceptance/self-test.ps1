# No VM, registry changes, or third-party test framework required.
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'common.ps1')
$project=Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$root=Join-Path $project ('output/acceptance-self-test/'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path "$root/candidate" | Out-Null
$checks=@()
function Check([string]$Name,[scriptblock]$Action) {
    & $Action
    $script:checks+=@{name=$Name;status='passed'}
    Write-Host "PASS $Name"
}
function Reject([scriptblock]$Action) { $rejected=$false;try{& $Action | Out-Null}catch{$rejected=$true};if(-not $rejected){throw 'Expected rejection'} }
try {
    Check 'Windows PowerShell 5.1 parses the native loading chain under Western ANSI' {
        $code=Invoke-AcceptanceProcess 'powershell.exe' @('-NoProfile','-File',"$project/scripts/acceptance/ps51-compatibility.ps1") "$root/ps51-compatibility.log"
        if($code -ne 0){throw "Windows PowerShell 5.1 encoding regression; see $root/ps51-compatibility.log"}
    }
    Check 'Runtime isolation requires explicit authorization before reading inputs' {
        $code=Invoke-AcceptanceProcess 'powershell.exe' @('-NoProfile','-File',"$project/scripts/acceptance/runtime.ps1",'-Installer','missing.exe','-PortableZip','missing.zip','-Case','unauthorized-self-test') "$root/runtime-refusal.log"
        if($code -ne 1 -or -not ([IO.File]::ReadAllText("$root/runtime-refusal.log")).Contains('Explicit runtime isolation authorization required')){throw 'Runtime isolation did not refuse unauthorized invocation'}
        $code=Invoke-AcceptanceProcess 'powershell.exe' @('-NoProfile','-File',"$project/scripts/acceptance/runtime.ps1",'-Installer','missing.exe','-PortableZip','missing.zip','-Case','wrong-host-self-test','-AuthorizedRuntimeIsolation') "$root/runtime-host-refusal.log"
        if($code -ne 1 -or -not ([IO.File]::ReadAllText("$root/runtime-host-refusal.log")).Contains('Requires TEST/Try interactive disposable VM desktop')){throw 'Runtime isolation bypassed the test-host guard'}
    }
    Check 'Containment rejects siblings and traversal' {
        Reject { Assert-AcceptancePath "$root/../outside" $root }
        Reject { Assert-AcceptancePath ($root+'-other/file') $root }
    }
    Check 'Linked ancestors are rejected' {
        New-Item -ItemType Directory -Path "$root/target" | Out-Null
        New-Item -ItemType Junction -Path "$root/link" -Target "$root/target" | Out-Null
        try { Reject { Assert-AcceptancePath "$root/link/file" $root } }
        finally { Remove-Item -LiteralPath "$root/link" }
    }
    Check 'Failure outranks incomplete and passed' {
        if ((Get-AcceptanceExitCode @(@{status='passed'},@{status='not-run'},@{status='failed'})) -ne 1) { throw 'Failure exit code' }
        if ((Get-AcceptanceExitCode @(@{status='environment-blocked'})) -ne 2) { throw 'Blocked exit code' }
        if ((Get-AcceptanceExitCode @(@{status='passed'})) -ne 0) { throw 'Success exit code' }
        Reject { New-AcceptanceResult 'invalid' 'skipped' '' '' }
    }
    Check 'Windows PowerShell stderr cannot bypass exit-code inspection' {
        $code=Invoke-AcceptanceProcess 'powershell.exe' @('-NoProfile','-Command',"throw 'expected-child-failure'") "$root/native-failure.log"
        if($code -ne 1 -or -not ([IO.File]::ReadAllText("$root/native-failure.log")).Contains('expected-child-failure')){throw 'Lost native failure evidence'}
        if($ErrorActionPreference -ne 'Stop'){throw 'Error policy leaked'}
    }
    Check 'Expected failure requires the exact error, cleanup success and matching case' {
        $record=@{case='fixture';status='failed';error='ACCEPTANCE_EXPECTED_FAILURE_AFTER_INSTALL';cleanup=@{status='passed'}}
        Assert-NativeAcceptanceResult $record 1 'fixture' -ExpectedFailure
        Reject { Assert-NativeAcceptanceResult $record 0 'fixture' -ExpectedFailure }
        Reject { Assert-NativeAcceptanceResult $record 1 'another-run' -ExpectedFailure }
        $record.cleanup.status='failed'
        Reject { Assert-NativeAcceptanceResult $record 1 'fixture' -ExpectedFailure }
        $record.cleanup.status='passed';$record.error='unexpected failure'
        Reject { Assert-NativeAcceptanceResult $record 1 'fixture' -ExpectedFailure }
        Reject { Assert-NativeAcceptanceResult $record 1 'fixture' }
    }
    if($env:COMPUTERNAME -ne 'TEST' -or $env:USERNAME -ne 'Try') {
        Check 'Native release entry refuses the development host before opening inputs' {
            $code=Invoke-AcceptanceProcess 'powershell.exe' @('-NoProfile','-File',"$project/scripts/verify-windows-vm.ps1",'-Case','self-test-refusal','-Installer','does-not-exist.exe','-Cli','does-not-exist.exe') "$root/host-refusal.log"
            if($code -ne 1 -or -not ([IO.File]::ReadAllText("$root/host-refusal.log")).Contains('Requires TEST/Try interactive disposable VM desktop')){throw 'Host safety guard was not reached before input use'}
        }
    }
    $files=@()
    foreach($name in @('AgentHub_0.1.1_x64-setup.exe','AgentHub-0.1.1-windows-x64.zip')) {
        [IO.File]::WriteAllText("$root/candidate/$name",'fictional fixture bytes')
        $files+=@{name=$name;bytes=(Get-Item "$root/candidate/$name").Length;sha256=(Get-FileHash "$root/candidate/$name").Hash.ToLowerInvariant()}
    }
    $manifest=@{product='AgentHub';version='0.1.1';sourceCommit=('a'*40);files=$files}
    Write-AcceptanceJson $manifest "$root/candidate/build-manifest.json"
    $sum=@($files|ForEach-Object{"$($_.sha256)  $($_.name)"})
    $sum+="$((Get-FileHash "$root/candidate/build-manifest.json").Hash.ToLowerInvariant())  build-manifest.json"
    [IO.File]::WriteAllLines("$root/candidate/SHA256SUMS.txt",[string[]]$sum)
    Check 'Valid candidate then tamper detection' {
        if ((Get-CandidateProof "$root/candidate").files.Count -ne 4) { throw 'Incomplete proof' }
        [IO.File]::AppendAllText("$root/candidate/AgentHub_0.1.1_x64-setup.exe",'changed')
        Reject { Get-CandidateProof "$root/candidate" }
    }
    Check 'Real entry point returns nonzero and complete evidence on candidate failure' {
        & pwsh -NoProfile -File "$project/scripts/acceptance.ps1" -Profile Release -CandidatePath "$root/candidate" -OutputRoot "$root/failed" *> "$root/failure.log"
        if ($LASTEXITCODE -ne 1) { throw "Expected exit 1, got $LASTEXITCODE" }
        $run=(Get-ChildItem "$root/failed" -Directory | Select-Object -First 1).FullName
        foreach($name in @('acceptance.json','acceptance.md','environment.json','candidate-hashes.json','cleanup.json','logs','screenshots')) { if(-not(Test-Path -LiteralPath (Join-Path $run $name))){throw "Missing $name"} }
        $report=[IO.File]::ReadAllText("$run/acceptance.json")|ConvertFrom-Json
        if ($report.status -ne 'failed' -or @($report.scenarios|Where-Object status -eq 'passed').Count) { throw 'False pass on invalid candidate' }
    }
    Check 'Dry run lists every scenario without executing tests and returns incomplete' {
        & pwsh -NoProfile -File "$project/scripts/acceptance.ps1" -Profile PullRequest -DryRun -OutputRoot "$root/dry" *> "$root/dry.log"
        if($LASTEXITCODE -ne 2){throw 'Dry run must not claim acceptance passed'}
        $run=(Get-ChildItem "$root/dry" -Directory|Select-Object -First 1).FullName
        $report=[IO.File]::ReadAllText("$run/acceptance.json")|ConvertFrom-Json
        if($report.scenarios.Count -ne 4 -or @($report.scenarios|Where-Object status -ne 'not-run').Count){throw 'Dry run executed a scenario'}
    }
    Check 'Empty results and omitted required scenarios cannot become passed' {
        if((Get-AcceptanceExitCode @()) -ne 2){throw 'Empty result was accepted'}
        $record=[ordered]@{profile='PullRequest';sourceCommit=('b'*40);workingTreeClean=$true;startedAt='test';finishedAt=$null;requiredScenarios=@('executed','missing');scenarios=@(New-AcceptanceResult 'executed' 'passed' 'fixture' 'test')}
        $dir=Join-Path $root 'missing';New-Item -ItemType Directory -Path $dir | Out-Null
        Write-AcceptanceReport $record $dir
        if($record.exitCode -ne 2){throw 'Missing required scenario was accepted'}
        $written=Get-Content "$dir/acceptance.json" -Raw|ConvertFrom-Json
        if($written.status -ne 'incomplete' -or $written.requiredScenarios -notcontains 'missing'){throw 'Missing requirement was lost in report'}
        if(-not([IO.File]::ReadAllText("$dir/acceptance.md")).Contains('missing: not-run')){throw 'Summary concealed the missing scenario'}
    }
    Check 'Timed-out process is failed and terminated, never successful' {
        Reject { Invoke-AcceptanceTimedProcess 'pwsh' @('-NoProfile','-Command','Start-Sleep -Seconds 30') "$root/timeout.log" 1 }
        if(-not([IO.File]::ReadAllText("$root/timeout.log")).Contains('ACCEPTANCE_TIMEOUT')){throw 'Timeout not identified'}
        Invoke-AcceptanceTimedProcess 'pwsh' @('-NoProfile','-Command','exit 0') "$root/success.log" 10
        Reject { Invoke-AcceptanceTimedProcess 'pwsh' @('-NoProfile','-Command','exit 1') "$root/exit-one.log" 10 }
    }
    Check 'Development desktop dry run cannot claim native validation' {
        & pwsh -NoProfile -File "$project/scripts/acceptance.ps1" -Profile DevelopmentDesktop -DryRun -OutputRoot "$root/native-dry" *> "$root/native-dry.log"
        if($LASTEXITCODE -ne 2){throw 'Native dry run false pass'}
        $dir=(Get-ChildItem "$root/native-dry" -Directory|Select-Object -First 1).FullName
        $record=Get-Content "$dir/acceptance.json" -Raw|ConvertFrom-Json
        if($record.scenarios.Count -ne 6 -or @($record.scenarios|Where-Object status -ne 'not-run').Count){throw 'Native dry run executed work'}
    }

    Write-AcceptanceJson @{status='passed';checks=$checks;finishedAt=[DateTime]::UtcNow.ToString('o')} "$root/self-test.json"
    Write-Host "Self-test evidence: $root"
} catch {
    Write-AcceptanceJson @{status='failed';checks=$checks;error=$_.ToString();stack=$_.ScriptStackTrace} "$root/self-test.json"
    throw
}
