param(
    [string]$Installer,
    [string]$Cli
)
$ErrorActionPreference = 'Stop'
$agenthubRoot = Split-Path $PSScriptRoot -Parent
if (-not $Installer) {
    $agenthubVersion = (Get-Content -LiteralPath (Join-Path $agenthubRoot 'package.json') -Raw | ConvertFrom-Json).version
    $Installer = Join-Path $agenthubRoot "target/release/bundle/nsis/AgentHub_$($agenthubVersion)_x64-setup.exe"
}
$agenthubInstaller = (Resolve-Path -LiteralPath $Installer).Path
if (-not $Cli) { $Cli = Join-Path $agenthubRoot 'target/debug/agenthub-dev.exe' }
$agenthubTestCli = (Resolve-Path -LiteralPath $Cli).Path
$agenthubRememberedKey = 'HKCU:\Software\agenthub\AgentHub'
$agenthubRememberedExists = Test-Path -LiteralPath $agenthubRememberedKey
$agenthubRememberedValue = if ($agenthubRememberedExists) { (Get-Item -LiteralPath $agenthubRememberedKey).GetValue('') } else { $null }
# Do not replace an existing user's registered installation during verification.
$agenthubExisting = Test-Path -LiteralPath 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\AgentHub'
if ($agenthubExisting) { throw 'An AgentHub installation already exists; use an isolated Windows account for installer verification.' }
foreach ($agenthubShortcutFolder in @([Environment]::GetFolderPath('Desktop'), [Environment]::GetFolderPath('Programs'))) {
    foreach ($agenthubShortcutName in @('AgentHub.lnk')) {
        if (Test-Path -LiteralPath (Join-Path $agenthubShortcutFolder $agenthubShortcutName)) { throw 'Existing application shortcut must be preserved.' }
    }
}
$agenthubTestRoot = Join-Path $agenthubRoot ('output/installer-tests/' + (Get-Date -Format 'yyyyMMdd-HHmmss') + '-' + [guid]::NewGuid().ToString('N').Substring(0,8))
$agenthubInstallRoot = Join-Path $agenthubTestRoot 'app'
New-Item -ItemType Directory -Path $agenthubTestRoot | Out-Null
$agenthubPreviousData = $env:AGENTHUB_DATA_DIR
$agenthubPreviousWebview = $env:WEBVIEW2_USER_DATA_FOLDER
$agenthubReport = [ordered]@{ status='running'; installer=$agenthubInstaller; sha256=(Get-FileHash -LiteralPath $agenthubInstaller -Algorithm SHA256).Hash; installRoot=$agenthubInstallRoot; steps=@() }
$agenthubUninstalled = $false
function Install-AgentHubFixture {
    $agenthubInstallProcess = Start-Process -FilePath $agenthubInstaller -ArgumentList "/S /D=$agenthubInstallRoot" -WindowStyle Hidden -Wait -PassThru
    if ($agenthubInstallProcess.ExitCode -ne 0) { throw "Installer exit code: $($agenthubInstallProcess.ExitCode)" }
    if (-not (Test-Path -LiteralPath (Join-Path $agenthubInstallRoot 'AgentHub.exe'))) { throw 'Installed desktop executable is missing' }
}
try {
    Remove-Item Env:AGENTHUB_DATA_DIR -ErrorAction SilentlyContinue
    Remove-Item Env:WEBVIEW2_USER_DATA_FOLDER -ErrorAction SilentlyContinue
    Install-AgentHubFixture
    & node (Join-Path $agenthubRoot 'scripts/verify-portable.mjs') $agenthubInstallRoot --installed *> (Join-Path $agenthubTestRoot 'package.json')
    if ($LASTEXITCODE -ne 0) { throw 'Installed package is not desktop-only' }
    $agenthubReport.steps += 'Fresh install contains only desktop runtime, layout marker and license notices'
    & (Join-Path $agenthubRoot 'scripts/verify-packaged-startup.ps1') -Exe (Join-Path $agenthubInstallRoot 'AgentHub.exe') -Cli $agenthubTestCli -DataDir (Join-Path $agenthubInstallRoot 'data') *> (Join-Path $agenthubTestRoot 'desktop.json')
    $agenthubReport.desktop = Get-Content -LiteralPath (Join-Path $agenthubRoot ('output/desktop-verification-' + (Get-Content -LiteralPath (Join-Path $agenthubRoot 'package.json') -Raw | ConvertFrom-Json).version + '.json')) -Raw | ConvertFrom-Json -Depth 100
    if ($agenthubReport.desktop.status -ne 'passed') { throw 'Installed desktop startup verification failed' }
    if (-not (Test-Path -LiteralPath (Join-Path $agenthubInstallRoot 'data/agenthub.sqlite3'))) { throw 'Desktop did not initialize its isolated database' }
    $agenthubReport.steps += 'Installed desktop renders with isolated test data; internal CLI is used only to prepare that fixture'
    $agenthubLibraryRoot = Join-Path $agenthubInstallRoot 'data/library/skills/installer-fixture'
    New-Item -ItemType Directory -Force -Path $agenthubLibraryRoot | Out-Null
    $agenthubLibraryFile = Join-Path $agenthubLibraryRoot 'SKILL.md'
    [IO.File]::WriteAllText($agenthubLibraryFile, 'Installer data boundary fixture')
    $agenthubLibraryHash = (Get-FileHash -LiteralPath $agenthubLibraryFile -Algorithm SHA256).Hash
    Install-AgentHubFixture
    if ((Get-FileHash -LiteralPath $agenthubLibraryFile -Algorithm SHA256).Hash -ne $agenthubLibraryHash) { throw 'Reinstall changed Skill data' }
    $agenthubReport.steps += 'Same-version reinstall preserves Skill data outside the application runtime'
    $agenthubUninstaller = Join-Path $agenthubInstallRoot 'uninstall.exe'
    $agenthubUninstallProcess = Start-Process -FilePath $agenthubUninstaller -ArgumentList "/S _?=$agenthubInstallRoot" -WindowStyle Hidden -Wait -PassThru
    if ($agenthubUninstallProcess.ExitCode -ne 0) { throw "Uninstaller exit code: $($agenthubUninstallProcess.ExitCode)" }
    if (Test-Path -LiteralPath (Join-Path $agenthubInstallRoot 'AgentHub.exe')) { throw 'Uninstall left the program executable behind' }
    if ((Get-FileHash -LiteralPath $agenthubLibraryFile -Algorithm SHA256).Hash -ne $agenthubLibraryHash) { throw 'Ordinary uninstall removed central Skill data' }
    $agenthubUninstalled = $true
    $agenthubReport.steps += 'Ordinary uninstall removes the program and preserves central Skill data'
    $agenthubReport.status = 'passed'
} catch {
    $agenthubReport.status = 'failed'
    $agenthubReport.error = $_.ToString()
} finally {
    if (-not $agenthubUninstalled -and (Test-Path -LiteralPath (Join-Path $agenthubInstallRoot 'uninstall.exe'))) {
        $agenthubCleanup = Start-Process -FilePath (Join-Path $agenthubInstallRoot 'uninstall.exe') -ArgumentList "/S _?=$agenthubInstallRoot" -WindowStyle Hidden -Wait -PassThru
        $agenthubUninstalled = $agenthubCleanup.ExitCode -eq 0 -and -not (Test-Path -LiteralPath (Join-Path $agenthubInstallRoot 'AgentHub.exe'))
        if (-not $agenthubUninstalled) { $agenthubReport.cleanupError = 'Verification uninstall failed'; $agenthubReport.status = 'failed' }
    }
    # NSIS retains the last directory after uninstall; never leave our fixture as the user's default.
    if (Test-Path -LiteralPath $agenthubRememberedKey) {
        $agenthubCurrentRemembered = (Get-Item -LiteralPath $agenthubRememberedKey).GetValue('')
        if ($agenthubCurrentRemembered -eq $agenthubInstallRoot) {
            if ($null -ne $agenthubRememberedValue) {
                Set-Item -LiteralPath $agenthubRememberedKey -Value $agenthubRememberedValue
            } else {
                $agenthubWritableKey = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Software\agenthub\AgentHub', $true)
                try { $agenthubWritableKey.DeleteValue('', $false) } finally { $agenthubWritableKey.Dispose() }
                $agenthubRemainingKey = Get-Item -LiteralPath $agenthubRememberedKey
                if (-not $agenthubRememberedExists -and $agenthubRemainingKey.ValueCount -eq 0 -and $agenthubRemainingKey.SubKeyCount -eq 0) {
                    Remove-Item -LiteralPath $agenthubRememberedKey
                }
            }
        }
    }
    $env:AGENTHUB_DATA_DIR = $agenthubPreviousData
    $env:WEBVIEW2_USER_DATA_FOLDER = $agenthubPreviousWebview
    $agenthubReport | ConvertTo-Json -Depth 100 | Set-Content -LiteralPath (Join-Path $agenthubTestRoot 'report.json') -Encoding utf8
    $agenthubReport | ConvertTo-Json -Depth 100
    if ($agenthubUninstalled) {
        $agenthubResolvedTestRoot = (Resolve-Path -LiteralPath $agenthubTestRoot).Path
        $agenthubResolvedApp = (Resolve-Path -LiteralPath $agenthubInstallRoot).Path
        if (-not $agenthubResolvedApp.StartsWith($agenthubResolvedTestRoot + '\', [StringComparison]::OrdinalIgnoreCase)) { throw 'Cleanup path escapes installer fixture' }
        # All contents of this directory were created by the isolated test above.
        Remove-Item -LiteralPath $agenthubResolvedApp -Recurse -Force
    }
}
if ($agenthubReport.status -ne 'passed') { throw $agenthubReport.error }
