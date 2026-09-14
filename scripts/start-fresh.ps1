param([string]$Exe, [switch]$Hidden)
$ErrorActionPreference = 'Stop'
$agenthubRoot = Split-Path $PSScriptRoot -Parent
if (-not $Exe) {
    $agenthubVersion = (Get-Content -LiteralPath (Join-Path $agenthubRoot 'src-tauri/tauri.conf.json') -Raw | ConvertFrom-Json).version
    $Exe = Join-Path $agenthubRoot "dist-portable/AgentHub-$agenthubVersion-windows-x64/AgentHub.exe"
}
$agenthubExecutable = (Resolve-Path -LiteralPath $Exe).Path
$agenthubRunName = (Get-Date -Format 'yyyyMMdd-HHmmss') + '-' + [guid]::NewGuid().ToString('N').Substring(0, 8)
$agenthubRunDirectory = Join-Path $agenthubRoot ('output/manual-tests/' + $agenthubRunName)
New-Item -ItemType Directory -Path $agenthubRunDirectory | Out-Null
$agenthubPreviousData = $env:AGENTHUB_DATA_DIR
$agenthubPreviousWebview = $env:WEBVIEW2_USER_DATA_FOLDER
try {
    $env:AGENTHUB_DATA_DIR = Join-Path $agenthubRunDirectory 'data'
    $env:WEBVIEW2_USER_DATA_FOLDER = Join-Path $agenthubRunDirectory 'webview'
    # Interactive testing entry point: each invocation starts a fresh profile.
    # Keep previous profiles because their archives can hold cancelled Skills.
    $agenthubProcess = Start-Process -FilePath $agenthubExecutable -WorkingDirectory (Split-Path $agenthubExecutable -Parent) -WindowStyle $(if ($Hidden) { 'Hidden' } else { 'Normal' }) -PassThru
    [pscustomobject]@{ ProcessId = $agenthubProcess.Id; DataDirectory = $env:AGENTHUB_DATA_DIR }
} finally {
    $env:AGENTHUB_DATA_DIR = $agenthubPreviousData
    $env:WEBVIEW2_USER_DATA_FOLDER = $agenthubPreviousWebview
}
