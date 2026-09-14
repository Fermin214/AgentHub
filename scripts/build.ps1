param([switch]$Installer, [switch]$DebugBuild)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'build-env.ps1')
Push-Location $agenthubProjectRoot
try {
    if (-not $Installer) {
        & npm.cmd run build
        if ($LASTEXITCODE -ne 0) { throw 'Frontend build failed' }
    }
    if ($Installer) {
        & npm.cmd exec -- tauri build
    } elseif ($DebugBuild) {
        & cargo build -p agenthub-desktop
    } else {
        & cargo build --release -p agenthub-desktop --features agenthub-desktop/custom-protocol
    }
    if ($LASTEXITCODE -ne 0) { throw 'Rust build failed' }
} finally { Pop-Location }
