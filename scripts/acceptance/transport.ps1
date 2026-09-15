function Invoke-VmCommand([string]$Target, [string]$Code, [switch]$ReadOnly) {
    $encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes("`$ErrorActionPreference='Stop';`$ProgressPreference='SilentlyContinue';" + $Code))
    $attempts=if($ReadOnly){3}else{1}
    for($attempt=1;$attempt -le $attempts;$attempt++) {
        $output = & ssh -o BatchMode=yes -o ConnectTimeout=10 -o ServerAliveInterval=15 -o ServerAliveCountMax=2 $Target "powershell -NoProfile -EncodedCommand $encoded" 2>&1
        if($LASTEXITCODE -eq 0){break}
        if($attempt -eq $attempts){throw "SSH failed: $output"}
        Start-Sleep -Seconds 2
    }
    return ($output -join "`n")
}
function Invoke-AcceptanceVm($Config, [string]$Evidence, [string]$RunId, [string]$Candidate, [string]$BaseVersion) {
    if ($Config.sshTarget -notmatch '^Try@[a-zA-Z0-9][a-zA-Z0-9.-]+$') { throw 'sshTarget must be Try@hostname (no options or shell syntax)' }
    if ($RunId -notmatch '^a-[a-z0-9-]+$') { throw 'Invalid generated run ID' }
    $target = $Config.sshTarget
    $remote = "C:\AgentHub-VM-Test\runs\$RunId"
    $stage = Join-Path $Evidence 'staging'
    New-Item -ItemType Directory -Path "$stage/scripts/acceptance","$stage/candidate","$stage/helpers" | Out-Null
    foreach ($name in @('verify-windows-vm.ps1','verify-vm-portable.ps1')) { Copy-Item -LiteralPath (Join-Path $PSScriptRoot "../$name") -Destination "$stage/scripts/$name" }
    foreach ($name in @('common.ps1','vm-safety.ps1','worker.ps1','languages.ps1')) { Copy-Item -LiteralPath (Join-Path $PSScriptRoot $name) -Destination "$stage/scripts/acceptance/$name" }
    $proof = Get-CandidateProof $Candidate
    foreach ($file in $proof.files) { Copy-Item -LiteralPath (Join-Path $Candidate $file.name) -Destination "$stage/candidate" }
    Copy-Item -LiteralPath ([IO.Path]::GetFullPath($Config.cliPath)) -Destination "$stage/helpers/candidate-cli.exe"
    $job = @{runId=$RunId; taskName="AgentHub-Acceptance-$RunId"; candidate=$proof; cliSha256=(Get-FileHash -LiteralPath "$stage/helpers/candidate-cli.exe").Hash; baseVersion=$BaseVersion; hasBase=$false}
    if ($BaseVersion -and $Config.baseCandidatePath -and $Config.baseCliPath) {
        $base = Get-CandidateProof ([IO.Path]::GetFullPath($Config.baseCandidatePath))
        if ($base.version -ne $BaseVersion -or [version]$BaseVersion -ge [version]$proof.version) { throw 'Base must match BaseVersion and precede candidate' }
        New-Item -ItemType Directory -Path "$stage/base" | Out-Null
        foreach ($file in $base.files) { Copy-Item -LiteralPath (Join-Path $Config.baseCandidatePath $file.name) -Destination "$stage/base" }
        Copy-Item -LiteralPath ([IO.Path]::GetFullPath($Config.baseCliPath)) -Destination "$stage/helpers/base-cli.exe"
        $job.hasBase=$true; $job.base=$base; $job.baseCliSha256=(Get-FileHash -LiteralPath "$stage/helpers/base-cli.exe").Hash
    }
    Write-AcceptanceJson $job "$stage/job.json"
    # Staged bytes, not user paths, enter the remote command. Identity and hardware are fixed.
    $prepare = @'
if($env:COMPUTERNAME -ne 'TEST' -or $env:USERNAME -ne 'Try'){throw 'Wrong VM identity'}
if((Get-CimInstance Win32_ComputerSystem).Model -notmatch 'VMware|Virtual Machine|VirtualBox|KVM|QEMU'){throw 'Not a disposable VM'}
$root='C:\AgentHub-VM-Test';$cursor='__REMOTE__'
while($cursor){if((Test-Path -LiteralPath $cursor) -and ((Get-Item -LiteralPath $cursor -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)){throw 'Linked VM root'};$cursor=Split-Path $cursor -Parent}
if(Test-Path -LiteralPath '__REMOTE__'){throw 'Run directory exists'}
if(Test-Path -LiteralPath (Join-Path $root 'acceptance-lock')){throw 'Another acceptance run owns the VM; inspect its journal before recovery'}
New-Item -ItemType Directory -Path (Join-Path $root 'acceptance-lock') | Out-Null
try {
 [IO.File]::WriteAllText((Join-Path $root 'acceptance-lock/owner.txt'),'__RUN__')
 New-Item -ItemType Directory -Path '__REMOTE__' | Out-Null
} catch {
 $owner=Join-Path $root 'acceptance-lock/owner.txt'
 if((Test-Path -LiteralPath $owner) -and [IO.File]::ReadAllText($owner) -eq '__RUN__'){Remove-Item -LiteralPath $owner}
 if(@(Get-ChildItem -LiteralPath (Join-Path $root 'acceptance-lock') -Force).Count -eq 0){Remove-Item -LiteralPath (Join-Path $root 'acceptance-lock')}
 throw
}
'@
    $prepared=$false; $started=$false; $completed=$false
    try {
        Invoke-VmCommand $target ($prepare.Replace('__REMOTE__',$remote).Replace('__RUN__',$RunId)) | Out-Null
        $prepared=$true
        & scp -q -r "$stage/." "${target}:$($remote.Replace('\','/'))/"
        if ($LASTEXITCODE -ne 0) { throw 'VM staging copy failed' }
        $launch = @'
$remote='__REMOTE__';$task='AgentHub-Acceptance-__RUN__'
$action=New-ScheduledTaskAction -Execute 'powershell.exe' -Argument ('-NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File "'+$remote+'\scripts\acceptance\worker.ps1" -RunDirectory "'+$remote+'"')
$principal=New-ScheduledTaskPrincipal -UserId 'Try' -LogonType Interactive -RunLevel Highest
$settings=New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([TimeSpan]::Zero) -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries
Register-ScheduledTask -TaskName $task -Action $action -Principal $principal -Settings $settings | Out-Null
Start-ScheduledTask -TaskName $task
'@
        Invoke-VmCommand $target ($launch.Replace('__REMOTE__',$remote).Replace('__RUN__',$RunId)) | Out-Null
        $started=$true
        $deadline=[DateTime]::UtcNow.AddMinutes(30)
        do {
            $state=Invoke-VmCommand $target "if(Test-Path -LiteralPath '$remote/evidence/result.json'){'True'}else{`$task=Get-ScheduledTask -TaskName 'AgentHub-Acceptance-$RunId' -ErrorAction SilentlyContinue;if(`$task -and `$task.State -ne 'Running' -and (Get-ScheduledTaskInfo -TaskName `$task.TaskName).LastTaskResult -notin @(0,267009,267011)){'Worker not running'}else{'Running'}}" -ReadOnly
            if ($state.Trim() -eq 'True') { $completed=$true; break }
            if ($state.Trim() -eq 'Worker not running') { throw "Worker stopped before its final report; inspect $remote and the scheduled task. Preserve the VM lock for recovery." }
            Write-Host '[Release] VM worker running; evidence and recovery journal remain on VM'
            Start-Sleep -Seconds 10
        } while ([DateTime]::UtcNow -lt $deadline)
        if (-not $completed) { throw "VM worker timed out; do not terminate it or remove its lock. Inspect $remote and rerun evidence retrieval after its finally completes." }
        & scp -q -r "${target}:$($remote.Replace('\','/'))/evidence" "$Evidence/vm"
        if ($LASTEXITCODE -ne 0) { throw 'VM evidence retrieval failed; remote evidence is retained' }
        $transportCleanup = @{status='passed'; remote=$remote; workerCompleted=$true; taskAbsent=(Invoke-VmCommand $target "-not [bool](Get-ScheduledTask -TaskName 'AgentHub-Acceptance-$RunId' -ErrorAction SilentlyContinue)").Trim() -eq 'True'}
        if (-not $transportCleanup.taskAbsent) { $transportCleanup.status='failed';$transportCleanup.error='Worker left its scheduled task' }
        Write-AcceptanceJson $transportCleanup "$Evidence/transport-cleanup.json"
    } catch {
        if ($started) { Write-AcceptanceJson @{status='failed';error=('Remote completion or cleanup could not be verified: '+$_.ToString());remote=$remote} "$Evidence/transport-cleanup.json" }
        throw
    } finally {
        if ($prepared -and -not $started) {
            # If task creation partially succeeded, preserve state for review instead of stopping it.
            $rollback = @'
$task=Get-ScheduledTask -TaskName 'AgentHub-Acceptance-__RUN__' -ErrorAction SilentlyContinue
if($task){throw 'Task creation partially succeeded; inspect it before lock recovery'}
$lock='C:\AgentHub-VM-Test\acceptance-lock'
if([IO.File]::ReadAllText((Join-Path $lock 'owner.txt')) -ne '__RUN__'){throw 'Lock ownership changed'}
Remove-Item -LiteralPath (Join-Path $lock 'owner.txt')
Remove-Item -LiteralPath $lock
'@
            try { Invoke-VmCommand $target ($rollback.Replace('__RUN__',$RunId)) | Out-Null; Write-AcceptanceJson @{status='passed';remote=$remote;notes='Staging failed before task start; owned lock removed, staged evidence retained'} "$Evidence/transport-cleanup.json" }
            catch { Write-AcceptanceJson @{status='failed'; error=$_.ToString(); remote=$remote} "$Evidence/transport-cleanup.json" }
        } elseif ($started -and -not $completed) {
            Write-AcceptanceJson @{status='failed'; error='Remote worker completion/restoration unverified; preserve lock and inspect remote journal'; remote=$remote} "$Evidence/transport-cleanup.json"
        }
    }
}
