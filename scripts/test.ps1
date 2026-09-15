param([ValidateSet('All','Rust','Frontend','Build')][string]$Stage='All')
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'build-env.ps1')
Push-Location $agenthubProjectRoot
try {
    if ($Stage -in @('All','Rust')) {
        & cargo test --workspace
        if ($LASTEXITCODE -ne 0) { throw 'Rust tests failed' }
    }
    if ($Stage -in @('All','Frontend')) {
        & npm.cmd test -- --run
        if ($LASTEXITCODE -ne 0) { throw 'Frontend tests failed' }
    }
    if ($Stage -in @('All','Build')) {
        & npm.cmd run build
        if ($LASTEXITCODE -ne 0) { throw 'TypeScript/production build failed' }
    }
} finally { Pop-Location }
