param([string]$DataDir)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'build-env.ps1')
if (-not $DataDir) { $DataDir = Join-Path $agenthubProjectRoot 'output/development/data' }
$env:AGENTHUB_DATA_DIR = [IO.Path]::GetFullPath($DataDir)
$env:WEBVIEW2_USER_DATA_FOLDER = Join-Path $env:AGENTHUB_DATA_DIR 'webview'
Push-Location $agenthubProjectRoot
try {
    Write-Host "开发数据：$env:AGENTHUB_DATA_DIR"
    Write-Host '界面保存后自动刷新；Rust 改动自动编译并重启桌面。Ctrl+C 停止。'
    & npm.cmd run tauri:dev
    if ($LASTEXITCODE -ne 0) { throw 'Desktop development session failed' }
} finally { Pop-Location }
