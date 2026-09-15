. (Join-Path $PSScriptRoot 'common.ps1')
function Assert-TestDesktop {
    if ($env:COMPUTERNAME -ne 'TEST' -or $env:USERNAME -ne 'Try' -or [Diagnostics.Process]::GetCurrentProcess().SessionId -eq 0) { throw 'Requires TEST/Try interactive disposable VM desktop' }
    $machine = Get-CimInstance Win32_ComputerSystem
    if ($machine.Model -notmatch 'VMware|Virtual Machine|VirtualBox|KVM|QEMU') { throw 'Disposable virtual machine hardware was not detected' }
    [void](Assert-AcceptancePath 'C:\AgentHub-VM-Test' 'C:\AgentHub-VM-Test')
    foreach($name in @('cases','evidence','runs','acceptance-lock')) { [void](Assert-AcceptancePath (Join-Path 'C:\AgentHub-VM-Test' $name) 'C:\AgentHub-VM-Test') }
}
function Get-TestMachineState {
    $proxy = Get-Item 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet Settings'
    $values = [ordered]@{}
    foreach ($name in @('ProxyEnable','ProxyServer','ProxyOverride','AutoConfigURL','AutoDetect')) {
        $values[$name] = @{exists=($proxy.GetValueNames() -contains $name); value=$proxy.GetValue($name)}
    }
    $registration = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\AgentHub'
    $remembered = 'HKCU:\Software\agenthub\AgentHub'
    $runtime = 'C:\Program Files (x86)\Microsoft\EdgeWebView\Application'
    return [ordered]@{
        computer=$env:COMPUTERNAME; user=$env:USERNAME; session=[Diagnostics.Process]::GetCurrentProcess().SessionId
        model=(Get-CimInstance Win32_ComputerSystem).Model; os=[Environment]::OSVersion.VersionString
        registrationExists=(Test-Path -LiteralPath $registration); rememberedExists=(Test-Path -LiteralPath $remembered)
        installationExists=(Test-Path -LiteralPath (Join-Path $env:LOCALAPPDATA 'AgentHub'))
        shortcuts=@(@([Environment]::GetFolderPath('Desktop'),[Environment]::GetFolderPath('Programs')) | ForEach-Object { Test-Path -LiteralPath (Join-Path $_ 'AgentHub.lnk') })
        proxy=$values
        runtime=@(Get-ChildItem -LiteralPath $runtime -Filter msedgewebview2.exe -Recurse -ErrorAction SilentlyContinue | Sort-Object FullName | ForEach-Object { @{path=$_.FullName; sha256=(Get-FileHash -LiteralPath $_.FullName).Hash} })
        agenthubProcesses=@(Get-CimInstance Win32_Process -Filter "Name='AgentHub.exe'" | Select-Object ProcessId,ExecutablePath,CommandLine)
    }
}
function Assert-CleanTestInstallation {
    $state = Get-TestMachineState
    if ($state.registrationExists -or $state.rememberedExists -or $state.installationExists -or $state.shortcuts -contains $true -or $state.agenthubProcesses.Count) { throw 'Preserve existing installation, data, shortcuts and processes; restore a clean VM first' }
}
function Remove-OwnedTestInstallation([string]$InstallRoot, [string]$CaseRoot) {
    # The caller must have completed the clean-state preflight before taking ownership.
    $owned = Assert-AcceptancePath $InstallRoot $InstallRoot
    $casePath = Assert-AcceptancePath $CaseRoot 'C:\AgentHub-VM-Test\cases'
    if (-not (Test-Path -LiteralPath (Join-Path $casePath 'installation-owner.json'))) { throw 'Missing installation ownership marker' }
    $owner = [IO.File]::ReadAllText((Join-Path $casePath 'installation-owner.json')) | ConvertFrom-Json
    if ($owner.installRoot -ne $owned -or $owner.caseRoot -ne $casePath) { throw 'Installation ownership mismatch' }
    if (Test-Path -LiteralPath $owned) {
        if (@(Get-ChildItem -LiteralPath $owned -Recurse -Force | Where-Object { $_.Attributes -band [IO.FileAttributes]::ReparsePoint }).Count) { throw 'Refusing cleanup of linked installation contents' }
        $uninstall = Join-Path $owned 'uninstall.exe'
        if (Test-Path -LiteralPath $uninstall) {
            $p = Start-Process -FilePath $uninstall -ArgumentList "/S _?=$owned" -WindowStyle Hidden -PassThru
            if (-not $p.WaitForExit(90000) -or $p.ExitCode -ne 0) { throw 'Cleanup uninstaller did not succeed' }
        }
        if (Test-Path -LiteralPath (Join-Path $owned 'AgentHub.exe')) { throw 'Cleanup left executable; preserve for recovery' }
    }
    $remembered = 'HKCU:\Software\agenthub\AgentHub'
    if (Test-Path -LiteralPath $remembered) {
        $key = Get-Item -LiteralPath $remembered
        if ($key.GetValue('') -ne $owned -or $key.ValueCount -ne 1 -or $key.SubKeyCount -ne 0) { throw 'Unexpected remembered state; preserve it' }
        Remove-Item -LiteralPath $remembered
    }
    if (Test-Path -LiteralPath 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\AgentHub') { throw 'Registration remains after cleanup' }
    if ((Test-Path -LiteralPath $owned) -and $owned -eq (Join-Path $env:LOCALAPPDATA 'AgentHub')) {
        $destination = Assert-AcceptancePath (Join-Path $casePath 'retained-installation') $casePath
        if (Test-Path -LiteralPath $destination) { throw 'Preserve existing retained evidence' }
        Move-Item -LiteralPath $owned -Destination $destination
    }
}
