# Explicit opt-in development desktop adapter. No installer or product changes.
function Invoke-DevelopmentDesktop($Report,[string]$Evidence,[string]$Project) {
    $start=[DateTime]::UtcNow.ToString('o')
    $owned=@(); $app=$null; $server=$null
    $caseRoot=Assert-AcceptancePath (Join-Path $Evidence 'fixture') $Evidence
    $oldEnv=@{}
    foreach($name in @('AGENTHUB_DATA_DIR','WEBVIEW2_USER_DATA_FOLDER','WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS')) { $oldEnv[$name]=[Environment]::GetEnvironmentVariable($name,'Process') }
    function Result($Id,$Status,$Reason,$Files=@()) { $Report.scenarios+=New-AcceptanceResult $Id $Status $Reason $start $Files }
    function Start-Owned($Exe,$Arguments,$Name) {
        $info=[Diagnostics.ProcessStartInfo]::new($Exe)
        $info.UseShellExecute=$false; $info.CreateNoWindow=$true; $info.WorkingDirectory=$Project
        $info.RedirectStandardOutput=$true; $info.RedirectStandardError=$true
        foreach($arg in $Arguments){$info.ArgumentList.Add($arg)}
        $p=[Diagnostics.Process]::new();$p.StartInfo=$info
        if(-not $p.Start()){throw "Could not start $Name"}
        $record=@{process=$p;name=$Name;stdout=$p.StandardOutput.ReadToEndAsync();stderr=$p.StandardError.ReadToEndAsync();pid=$p.Id;startedAt=$p.StartTime.ToUniversalTime().ToString('o');path=$Exe}
        return $record
    }
    function Stop-Owned($Record) {
        if(-not $Record){return}
        $p=$Record.process
        if(-not $p.HasExited){
            if($Record.name -like 'app*'){[void]$p.CloseMainWindow();[void]$p.WaitForExit(10000)}
            if(-not $p.HasExited){$p.Kill($true);if(-not $p.WaitForExit(10000)){throw 'Owned process did not exit'}}
        }
        [IO.File]::WriteAllText((Join-Path $Evidence "logs/$($Record.name).log"),$Record.stdout.GetAwaiter().GetResult()+$Record.stderr.GetAwaiter().GetResult())
    }
    function Await-Endpoint($App,$Port) {
        $deadline=[DateTime]::UtcNow.AddSeconds(40)
        do {
            if($App.process.HasExited){throw 'Native app exited before CDP was ready'}
            try {
                $listener=Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction Stop | Select-Object -First 1
                if($listener.LocalAddress -notin @('127.0.0.1','::1')){throw 'CDP is not loopback-only'}
                # Follow live ancestry to the exact process we launched, never a shared port.
                $cursor=[int]$listener.OwningProcess; $seen=@()
                while($cursor -ne $App.pid -and $cursor -gt 0 -and $cursor -notin $seen){$seen+=$cursor;$cursor=[int](Get-CimInstance Win32_Process -Filter "ProcessId=$cursor").ParentProcessId}
                if($cursor -ne $App.pid){throw 'CDP listener is not owned by this app'}
                $null=Invoke-RestMethod "http://127.0.0.1:$Port/json/version" -TimeoutSec 2
                return
            } catch { $last=$_.ToString();Start-Sleep -Milliseconds 200 }
        } while([DateTime]::UtcNow -lt $deadline)
        throw "Native endpoint unavailable: $last"
    }
    function Resize-Owned($App,[int]$Width,[int]$Height) {
        $p=$App.process;$p.Refresh()
        $client=[AcceptanceWindow+Rect]::new();$outer=[AcceptanceWindow+Rect]::new()
        if(-not [AcceptanceWindow]::GetClientRect($p.MainWindowHandle,[ref]$client)){throw 'Native client rectangle unavailable'}
        if(-not [AcceptanceWindow]::GetWindowRect($p.MainWindowHandle,[ref]$outer)){throw 'Native window rectangle unavailable'}
        if(-not [AcceptanceWindow]::SetWindowPos($p.MainWindowHandle,[IntPtr]::Zero,0,0,$Width+$outer.right-$outer.left-$client.right+$client.left,$Height+$outer.bottom-$outer.top-$client.bottom+$client.top,6)){throw 'Native resize failed'}
    }
    $stage='native-environment'
    try {
        if(-not $IsWindows -or -not [Environment]::UserInteractive){throw 'ENVIRONMENT_BLOCKED: Windows interactive desktop required'}
        . (Join-Path $Project 'scripts/build-env.ps1')
        foreach($command in @('cargo','rustc','node')){if(-not(Get-Command $command -ErrorAction SilentlyContinue)){throw "ENVIRONMENT_BLOCKED: $command unavailable; see build-env.ps1"}}
        if(-not(Test-Path "$Project/node_modules/@playwright/test/package.json")){throw 'ENVIRONMENT_BLOCKED: run npm ci first'}
        $runtime=@(@("${env:ProgramFiles(x86)}/Microsoft/EdgeWebView/Application","$env:LOCALAPPDATA/Microsoft/EdgeWebView/Application") | Where-Object {Test-Path -LiteralPath $_} | ForEach-Object {Get-ChildItem -LiteralPath $_ -Filter msedgewebview2.exe -Recurse} | Select-Object -First 1)
        if(-not $runtime.Count){throw 'ENVIRONMENT_BLOCKED: installed WebView2 runtime not found'}
        $url=[uri]([IO.File]::ReadAllText("$Project/src-tauri/tauri.conf.json")|ConvertFrom-Json).build.devUrl
        if($url.Host -ne '127.0.0.1'){throw 'ENVIRONMENT_BLOCKED: development URL must be loopback'}
        if(Get-NetTCPConnection -LocalPort $url.Port -State Listen -ErrorAction SilentlyContinue){throw "ENVIRONMENT_BLOCKED: development port $($url.Port) already owned"}
        $reservation=[Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback,0);$reservation.Start();$port=$reservation.LocalEndpoint.Port;$reservation.Stop()
        $rustVersion=(& rustc --version) -join ''
        if($LASTEXITCODE -ne 0){throw 'ENVIRONMENT_BLOCKED: rustc cannot run; inspect selected toolchain'}
        $cargoVersion=(& cargo --version) -join ''
        if($LASTEXITCODE -ne 0){throw 'ENVIRONMENT_BLOCKED: cargo cannot run'}
        Write-AcceptanceJson @{rust=$rustVersion;cargo=$cargoVersion;rustupToolchain=$env:RUSTUP_TOOLCHAIN;buildRoot=$agenthubBuildRoot;webview=$runtime[0].VersionInfo.ProductVersion;devUrl=$url.AbsoluteUri;cdpPort=$port;isolation=$caseRoot} "$Evidence/native-environment.json"
        Result $stage 'passed' 'Toolchain found; unused loopback ports; isolated paths' @('native-environment.json')
        $stage='native-build'
        Invoke-AcceptanceTimedProcess 'cargo' @('build','-p','agenthub-cli','-p','agenthub-desktop') "$Evidence/logs/native-build.log"
        $exe=Join-Path $env:CARGO_TARGET_DIR 'debug/AgentHub.exe';$cli=Join-Path $env:CARGO_TARGET_DIR 'debug/agenthub-dev.exe'
        Write-AcceptanceJson @{sourceCommit=$Report.sourceCommit;appSha256=(Get-FileHash $exe).Hash;cliSha256=(Get-FileHash $cli).Hash;kind='development-debug'} "$Evidence/native-build.json"
        Result $stage 'passed' 'Built this checkout, no pre-existing binary accepted' @('native-build.json','logs/native-build.log')
        $stage='native-contract'
        Invoke-AcceptanceTimedProcess 'pwsh' @('-NoProfile','-File',"$Project/scripts/acceptance/desktop-fixture.ps1",'-Cli',$cli,'-CaseRoot',$caseRoot) "$Evidence/logs/native-contract.log"
        Result $stage 'passed' 'Fictional data and real Rust dispatch contracts verified' @('logs/native-contract.log')
        $env:AGENTHUB_DATA_DIR=Join-Path $caseRoot 'data'
        $env:WEBVIEW2_USER_DATA_FOLDER=Join-Path $caseRoot 'webview'
        $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=$port --remote-debugging-address=127.0.0.1"
        $server=Start-Owned (Get-Command node).Source @("$Project/node_modules/vite/bin/vite.js",'--config',"$Project/scripts/acceptance/ui-vite.config.ts",'--host','127.0.0.1','--port',"$($url.Port)",'--strictPort') 'vite';$owned+=$server
        $deadline=[DateTime]::UtcNow.AddSeconds(30)
        do {try{$null=Invoke-WebRequest $url.AbsoluteUri -TimeoutSec 2;break}catch{if($server.process.HasExited -or [DateTime]::UtcNow -gt $deadline){throw 'Owned Vite failed to become ready'};Start-Sleep -Milliseconds 200}} while($true)
        $stage='native-ui'
        Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class AcceptanceWindow {
 [StructLayout(LayoutKind.Sequential)] public struct Rect { public int left,top,right,bottom; }
 [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr h,out Rect r);
 [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h,out Rect r);
 [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h,IntPtr after,int x,int y,int cx,int cy,uint flags);
}
'@
        $app=Start-Owned $exe @() 'app-first';$owned+=$app;Await-Endpoint $app $port
        foreach($size in @(@(1280,860),@(860,640))){
            Resize-Owned $app $size[0] $size[1]
            $phase="layout-$($size[0])x$($size[1])"
            Invoke-AcceptanceTimedProcess 'node' @("$Project/scripts/acceptance/desktop-check.mjs","http://127.0.0.1:$port",$env:AGENTHUB_DATA_DIR,$Evidence,$phase,$url.AbsoluteUri) "$Evidence/logs/$phase.log" 180
        }
        Invoke-AcceptanceTimedProcess 'node' @("$Project/scripts/acceptance/desktop-check.mjs","http://127.0.0.1:$port",$env:AGENTHUB_DATA_DIR,$Evidence,'favorites',$url.AbsoluteUri) "$Evidence/logs/favorites.log" 180
        Result $stage 'passed' 'Bilingual native layout and real saves; delay/rejection are explicit IPC injection' @('layout-1280x860.json','layout-860x640.json','favorites.json','screenshots')
        $stage='native-restart'
        Stop-Owned $app
        $app=Start-Owned $exe @() 'app-restart';$owned+=$app;Await-Endpoint $app $port
        Invoke-AcceptanceTimedProcess 'node' @("$Project/scripts/acceptance/desktop-check.mjs","http://127.0.0.1:$port",$env:AGENTHUB_DATA_DIR,$Evidence,'restart',$url.AbsoluteUri) "$Evidence/logs/restart.log" 90
        Result $stage 'passed' 'New application process reads committed values without injection' @('restart.json','screenshots/native-restart.png')
    } catch {
        $status=if($_.ToString().StartsWith('ENVIRONMENT_BLOCKED:')){'environment-blocked'}else{'failed'}
        Result $stage $status ($_.ToString()+' '+$_.ScriptStackTrace)
    } finally {
        $errors=@()
        foreach($record in @($owned | Sort-Object pid -Descending)){try{Stop-Owned $record}catch{$errors+=$_.ToString()}}
        if($owned.Count){
            try {
                $deadline=[DateTime]::UtcNow.AddSeconds(5)
                do {
                    $left=@(Get-CimInstance Win32_Process -Filter "Name='msedgewebview2.exe'" | Where-Object { $_.CommandLine -and $_.CommandLine.Contains((Join-Path $caseRoot 'webview')) })
                    if(-not $left.Count){break};Start-Sleep -Milliseconds 200
                }while([DateTime]::UtcNow -lt $deadline)
                if($left.Count){throw 'Owned WebView2 profile still has live processes; preserve evidence for inspection'}
            }catch{$errors+=$_.ToString()}
        }
        # Keep owned fixture data as evidence; no recursive deletion or shared-process cleanup.
        foreach($name in $oldEnv.Keys){[Environment]::SetEnvironmentVariable($name,$oldEnv[$name],'Process')}
        $cleanup=@{status=$(if($errors.Count){'failed'}else{'passed'});errors=$errors;owned=@($owned|ForEach-Object{@{pid=$_.pid;startedAt=$_.startedAt;path=$_.path;exited=$_.process.HasExited}});retainedFixture=$caseRoot}
        Write-AcceptanceJson $cleanup "$Evidence/native-cleanup.json"
        Result 'native-cleanup' $cleanup.status 'Only owned processes stopped; isolated data retained as evidence' @('native-cleanup.json')
        foreach($record in $owned){$record.process.Dispose()}
    }
}
