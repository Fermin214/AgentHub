param([string]$Exe, [switch]$Hidden)
$ErrorActionPreference = 'Stop'
$agenthubRoot = Split-Path $PSScriptRoot -Parent
if (-not $Exe) { $Exe = Join-Path $agenthubRoot 'target/release/AgentHub.exe' }
$agenthubExecutable = (Resolve-Path -LiteralPath $Exe).Path
$agenthubPreviousData = $env:AGENTHUB_DATA_DIR
$agenthubPreviousWebview = $env:WEBVIEW2_USER_DATA_FOLDER
try {
    $env:AGENTHUB_DATA_DIR = Join-Path $agenthubRoot 'output/development/data'
    $env:WEBVIEW2_USER_DATA_FOLDER = Join-Path $env:AGENTHUB_DATA_DIR 'webview'
    Start-Process -FilePath $agenthubExecutable -WorkingDirectory (Split-Path $agenthubExecutable -Parent) -WindowStyle $(if ($Hidden) { 'Hidden' } else { 'Normal' })
} finally {
    $env:AGENTHUB_DATA_DIR = $agenthubPreviousData
    $env:WEBVIEW2_USER_DATA_FOLDER = $agenthubPreviousWebview
}
