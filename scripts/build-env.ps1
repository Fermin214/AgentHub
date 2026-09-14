# Dot-source this script. All environment changes are process-local.
$ErrorActionPreference = 'Stop'
$agenthubProjectRoot = Split-Path $PSScriptRoot -Parent
$agenthubBuildRoot = if ($env:AGENTHUB_BUILD_ROOT) { $env:AGENTHUB_BUILD_ROOT } else { Join-Path $agenthubProjectRoot '.build' }
if (Test-Path -LiteralPath (Join-Path $agenthubBuildRoot 'cargo/bin/cargo.exe')) {
    $env:CARGO_HOME = Join-Path $agenthubBuildRoot 'cargo'
    $env:RUSTUP_HOME = Join-Path $agenthubBuildRoot 'rustup'
    $env:PATH = (Join-Path $env:CARGO_HOME 'bin') + ';' + $env:PATH
}
$agenthubMsvcRoot = Join-Path $agenthubBuildRoot 'msvc'
if (Test-Path -LiteralPath $agenthubMsvcRoot) {
    $agenthubMsvcVersion = (Get-ChildItem -LiteralPath (Join-Path $agenthubMsvcRoot 'VC/Tools/MSVC') -Directory | Sort-Object Name -Descending | Select-Object -First 1).Name
    $agenthubSdkVersion = (Get-ChildItem -LiteralPath (Join-Path $agenthubMsvcRoot 'Windows Kits/10/Lib') -Directory | Sort-Object Name -Descending | Select-Object -First 1).Name
    $agenthubCompiler = Join-Path $agenthubMsvcRoot "VC/Tools/MSVC/$agenthubMsvcVersion"
    $agenthubSdk = Join-Path $agenthubMsvcRoot 'Windows Kits/10'
    $env:PATH = "$agenthubCompiler/bin/Hostx64/x64;$agenthubSdk/bin/$agenthubSdkVersion/x64;" + $env:PATH
    $env:INCLUDE = "$agenthubCompiler/include;$agenthubSdk/Include/$agenthubSdkVersion/ucrt;$agenthubSdk/Include/$agenthubSdkVersion/shared;$agenthubSdk/Include/$agenthubSdkVersion/um;$agenthubSdk/Include/$agenthubSdkVersion/winrt"
    $env:LIB = "$agenthubCompiler/lib/x64;$agenthubSdk/Lib/$agenthubSdkVersion/ucrt/x64;$agenthubSdk/Lib/$agenthubSdkVersion/um/x64"
    $env:VCToolsInstallDir = "$agenthubCompiler/"
    $env:WindowsSdkDir = "$agenthubSdk/"
    $env:WindowsSDKVersion = "$agenthubSdkVersion/"
    $env:VSCMD_ARG_TGT_ARCH = 'x64'
    $env:VSCMD_ARG_HOST_ARCH = 'x64'
}
$env:CARGO_TARGET_DIR = Join-Path $agenthubProjectRoot 'target'
