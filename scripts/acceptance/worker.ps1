param([Parameter(Mandatory=$true)][string]$RunDirectory)
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'vm-safety.ps1')
$runRoot=Assert-AcceptancePath $RunDirectory 'C:\AgentHub-VM-Test\runs'
$job=[IO.File]::ReadAllText((Join-Path $runRoot 'job.json')) | ConvertFrom-Json
if ($job.runId -notmatch '^a-[a-z0-9-]+$' -or (Split-Path $runRoot -Leaf) -ne $job.runId -or $job.taskName -ne "AgentHub-Acceptance-$($job.runId)") { throw 'Invalid worker ownership' }
$evidence=Join-Path $runRoot 'evidence'
New-Item -ItemType Directory -Path $evidence,"$evidence/logs","$evidence/screenshots" | Out-Null
$result=[ordered]@{scenarios=@();cleanup=@{status='failed';reason='Worker has not finished restoration'}}
$baseline=$null
$cleanupFailures=New-Object 'System.Collections.Generic.List[string]'
$start=[DateTime]::UtcNow.ToString('o')
function Run-VmScenario([string]$Id,[string]$Script,$Arguments,[switch]$ExpectedFailure,[switch]$Limited) {
    $began=[DateTime]::UtcNow.ToString('o')
    $case=$job.runId+'-'+$Id
    $log="$evidence/logs/$Id.log"
    $record=$null
    try {
        if($Limited){
            # WebView2 ignores environment browser flags in an elevated host.
            # Use the ordinary interactive user token, without changing app policy.
            $childName=$job.taskName+'-'+$Id
            if(Get-ScheduledTask -TaskName $childName -ErrorAction SilentlyContinue){throw 'Preserve existing interactive child task'}
            $childJob="$evidence/$Id-job.json";$childExit="$evidence/$Id-exit.json"
            Write-AcceptanceJson @{script=$Script;arguments=@($Arguments)+@('-Case',$case);log=$log;exit=$childExit} $childJob
            $code=@'
$ErrorActionPreference='Stop'
try {
 $job=[IO.File]::ReadAllText('__JOB__')|ConvertFrom-Json
 & powershell.exe -NoProfile -ExecutionPolicy Bypass -File $job.script @($job.arguments) *> $job.log
 $code=$LASTEXITCODE
 [IO.File]::WriteAllText($job.exit,([ordered]@{exitCode=$code}|ConvertTo-Json))
 exit $code
}catch { $_|Out-String|Write-Error;exit 1 }
'@
            $encoded=[Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($code.Replace('__JOB__',$childJob)))
            $action=New-ScheduledTaskAction -Execute 'powershell.exe' -Argument ('-NoProfile -WindowStyle Hidden -EncodedCommand '+$encoded)
            $principal=New-ScheduledTaskPrincipal -UserId 'TEST\Try' -LogonType Interactive -RunLevel Limited
            $settings=New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([TimeSpan]::Zero) -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries
            Register-ScheduledTask -TaskName $childName -Action $action -Principal $principal -Settings $settings|Out-Null
            Start-ScheduledTask -TaskName $childName
            $deadline=[DateTime]::UtcNow.AddMinutes(10)
            do { Start-Sleep -Milliseconds 500;$child=Get-ScheduledTask -TaskName $childName }while($child.State -eq 'Running' -and [DateTime]::UtcNow -lt $deadline)
            if($child.State -eq 'Running'){throw 'Interactive child timed out; retain task and VM lock for recovery'}
            if($child.Actions.Arguments -ne $action.Arguments){throw 'Interactive task ownership changed'}
            Unregister-ScheduledTask -TaskName $childName -Confirm:$false
            if(-not(Test-Path $childExit)){throw 'Interactive child stopped without an exit record'}
            $scenarioExit=([IO.File]::ReadAllText($childExit)|ConvertFrom-Json).exitCode
        }else{
            $scenarioExit=Invoke-AcceptanceProcess 'powershell.exe' (@('-NoProfile','-ExecutionPolicy','Bypass','-File',$Script)+$Arguments+@('-Case',$case)) $log
        }
        $source="C:\AgentHub-VM-Test\evidence\$case.json"
        $record=[IO.File]::ReadAllText($source) | ConvertFrom-Json
        Assert-NativeAcceptanceResult $record $scenarioExit $case -ExpectedFailure:$ExpectedFailure
        if ($ExpectedFailure) {
            Assert-CleanTestInstallation
        }
        $reason=if($ExpectedFailure){'Injected post-install failure exited nonzero; finally restored the clean installation baseline'}else{'Native fixture completed; see scenario evidence'}
        $result.scenarios+=New-AcceptanceResult $Id 'passed' $reason $began @("vm/$Id.json","vm/logs/$Id.log")
    } catch { $result.scenarios+=New-AcceptanceResult $Id 'failed' $_.ToString() $began @("vm/logs/$Id.log") }
    finally {
        if(-not $record -or $record.cleanup.status -ne 'passed') {
            $cleanupFailures.Add("$Id cleanup failed or was not verified; preserve VM lock for recovery")
        }
        $caseRoot=Assert-AcceptancePath "C:\AgentHub-VM-Test\cases\$case" 'C:\AgentHub-VM-Test\cases'
        $source="C:\AgentHub-VM-Test\evidence\$case.json"
        if (Test-Path -LiteralPath $source) { Copy-Item -LiteralPath $source -Destination "$evidence/$Id.json" }
        if (Test-Path -LiteralPath $caseRoot) {
            foreach($file in Get-ChildItem -LiteralPath $caseRoot -Filter '*.png' -File) { Copy-Item -LiteralPath $file.FullName -Destination "$evidence/screenshots/$Id-$($file.Name)" }
            if(Test-Path "$caseRoot/screenshots"){Copy-Item -LiteralPath "$caseRoot/screenshots" -Destination "$evidence/screenshots/$Id" -Recurse}
            if(Test-Path "$caseRoot/logs"){Copy-Item -LiteralPath "$caseRoot/logs" -Destination "$evidence/logs/$Id-details" -Recurse}
            foreach($file in Get-ChildItem -LiteralPath $caseRoot -Filter '*.json' -File){Copy-Item -LiteralPath $file.FullName -Destination "$evidence/$Id-$($file.Name)"}
        }
    }
    # Do not enter the next system-changing case if cleanup failed.
    if($cleanupFailures.Count){throw ($cleanupFailures -join '; ')}
    Assert-CleanTestInstallation
    Write-AcceptanceJson $result "$evidence/progress.json"
}
try {
    Assert-TestDesktop
    if ([IO.File]::ReadAllText('C:\AgentHub-VM-Test\acceptance-lock\owner.txt') -ne $job.runId) { throw 'Worker does not own VM lock' }
    Assert-CleanTestInstallation
    $baseline=Get-TestMachineState
    Write-AcceptanceJson $baseline "$evidence/environment-before.json"
    $candidate=Get-CandidateProof "$runRoot/candidate"
    if (($candidate | ConvertTo-Json -Depth 30 -Compress) -ne ($job.candidate | ConvertTo-Json -Depth 30 -Compress)) { throw 'Staged candidate differs from host' }
    $cli="$runRoot/helpers/candidate-cli.exe"
    if ((Get-FileHash -LiteralPath $cli).Hash -ne $job.cliSha256) { throw 'Fixture CLI hash mismatch' }
    $installer="$runRoot/candidate/AgentHub_$($candidate.version)_x64-setup.exe"
    Run-VmScenario 'cleanup-failure-probe' "$PSScriptRoot/../verify-windows-vm.ps1" @('-Installer',$installer,'-Cli',$cli,'-InjectFailureAfterInstall') -ExpectedFailure
    Run-VmScenario 'installer-lifecycle' "$PSScriptRoot/../verify-windows-vm.ps1" @('-Installer',$installer,'-Cli',$cli)
    Run-VmScenario 'portable-relocation' "$PSScriptRoot/../verify-vm-portable.ps1" @('-PortableZip',"$runRoot/candidate/AgentHub-$($candidate.version)-windows-x64.zip",'-Cli',$cli)
    if ($job.hasBase) {
        $base=Get-CandidateProof "$runRoot/base"
        if ($base.version -ne $job.baseVersion -or (Get-FileHash -LiteralPath "$runRoot/helpers/base-cli.exe").Hash -ne $job.baseCliSha256) { throw 'Base provenance mismatch' }
        Run-VmScenario 'upgrade' "$PSScriptRoot/../verify-windows-vm.ps1" @('-Installer',"$runRoot/base/AgentHub_$($base.version)_x64-setup.exe",'-Cli',"$runRoot/helpers/base-cli.exe",'-UpgradeInstaller',$installer,'-UpgradeCli',$cli)
    } else { $result.scenarios+=New-AcceptanceResult 'upgrade' 'environment-blocked' 'Supply published base candidate and matching CLI with BaseVersion' $start }
    Run-VmScenario 'installer-languages' "$PSScriptRoot/languages.ps1" @('-Installer',$installer)
    if($job.packagedUi -eq $true){
        if((Get-FileHash "$runRoot/helpers/node.exe").Hash -ne $job.nodeSha256){throw 'Node helper hash mismatch'}
        $manifest=[IO.File]::ReadAllText("$runRoot/candidate/build-manifest.json")|ConvertFrom-Json
        Run-VmScenario 'native-maintenance' "$PSScriptRoot/packaged.ps1" @('-PortableZip',"$runRoot/candidate/AgentHub-$($candidate.version)-windows-x64.zip",'-Cli',$cli,'-Node',"$runRoot/helpers/node.exe",'-DesktopSha256',$manifest.desktop.sha256) -Limited
    }
    if($job.allowRuntimeIsolation -eq $true){
        Run-VmScenario 'webview-download-failure-recovery' "$PSScriptRoot/runtime.ps1" @('-Installer',$installer,'-PortableZip',"$runRoot/candidate/AgentHub-$($candidate.version)-windows-x64.zip",'-AuthorizedRuntimeIsolation')
        $runtimeReport=[IO.File]::ReadAllText("$evidence/webview-download-failure-recovery.json")|ConvertFrom-Json
        $missingStatus=if($runtimeReport.missingPassed -and $runtimeReport.cleanup.status -eq 'passed'){'passed'}else{'failed'}
        $result.scenarios+=New-AcceptanceResult 'webview-missing' $missingStatus 'Operator-authorized runtime isolation; see missing-runtime and download UI evidence' $start @('vm/webview-download-failure-recovery.json')
    }
    if($job.browserDownload -eq $true){
        Run-VmScenario 'browser-download-startup' "$PSScriptRoot/browser.ps1" @('-CandidatePath',"$runRoot/candidate",'-UrlFile',"$runRoot/private-download-url.txt") -Limited
    }
} catch { $result.scenarios+=New-AcceptanceResult 'vm-worker' 'failed' ($_.ToString()+' '+$_.ScriptStackTrace) $start }
finally {
    $errors=@($cleanupFailures.ToArray())
    try{if(Test-Path -LiteralPath "$runRoot/private-download-url.txt"){Remove-Item -LiteralPath (Assert-AcceptancePath "$runRoot/private-download-url.txt" $runRoot)}}catch{$errors+=$_.ToString()}
    try {
        if(Get-ScheduledTask -TaskName ($job.taskName+'-*') -ErrorAction SilentlyContinue){throw 'Interactive child task remains; preserve VM lock'}
        $after=Get-TestMachineState
        Write-AcceptanceJson $after "$evidence/environment-after.json"
        if (-not $baseline) { throw 'No validated baseline; restoration cannot be established' }
        if (($baseline | ConvertTo-Json -Depth 30 -Compress) -ne ($after | ConvertTo-Json -Depth 30 -Compress)) { throw 'VM critical state differs: compare environment-before/after.json' }
        $remaining=@(Get-CimInstance Win32_Process | Where-Object { $_.Name -in @('AgentHub.exe','msedgewebview2.exe') -and $_.CommandLine -and $_.CommandLine.Contains($job.runId) })
        if ($remaining.Count) { throw 'Owned application/WebView2 processes remain' }
    } catch { $errors+=$_.ToString() }
    try {
        $task=Get-ScheduledTask -TaskName $job.taskName -ErrorAction SilentlyContinue
        if ($task) {
            if ($task.Actions.Arguments -notlike ('*'+$runRoot+'*')) { throw 'Scheduled task ownership changed' }
            Unregister-ScheduledTask -TaskName $job.taskName -Confirm:$false
        }
        if ($errors.Count -eq 0) {
            $lock=Assert-AcceptancePath 'C:\AgentHub-VM-Test\acceptance-lock' 'C:\AgentHub-VM-Test'
            if ([IO.File]::ReadAllText((Join-Path $lock 'owner.txt')) -ne $job.runId) { throw 'Lock ownership changed' }
            Remove-Item -LiteralPath (Join-Path $lock 'owner.txt')
            Remove-Item -LiteralPath $lock
        }
    } catch { $errors+=$_.ToString() }
    $result.cleanup=@{status=$(if($errors.Count){'failed'}else{'passed'});errors=$errors;retainedEvidence=$runRoot;retainedCases=$job.runId;finishedAt=[DateTime]::UtcNow.ToString('o')}
    $result.scenarios+=New-AcceptanceResult 'cleanup' $result.cleanup.status ($errors -join '; ') $start @('vm/environment-before.json','vm/environment-after.json','vm/cleanup.json')
    try {
        $afterProof=Get-CandidateProof "$runRoot/candidate"
        if (($afterProof | ConvertTo-Json -Depth 30 -Compress) -ne ($job.candidate | ConvertTo-Json -Depth 30 -Compress)) { throw 'Remote candidate changed' }
    } catch { $result.scenarios+=New-AcceptanceResult 'remote-candidate-unchanged' 'failed' $_.ToString() $start }
    Write-AcceptanceJson $result.cleanup "$evidence/cleanup.json"
    Write-AcceptanceJson $result "$evidence/result.pending.json"
    Move-Item -LiteralPath "$evidence/result.pending.json" -Destination "$evidence/result.json"
}
exit (Get-AcceptanceExitCode $result.scenarios)
