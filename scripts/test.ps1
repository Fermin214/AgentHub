$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'build-env.ps1')
Push-Location $agenthubProjectRoot
try {
    & cargo test --workspace
    if ($LASTEXITCODE -ne 0) { throw 'Rust tests failed' }
    & npm.cmd test -- --run
    if ($LASTEXITCODE -ne 0) { throw 'Frontend tests failed' }
    & npm.cmd run build
    if ($LASTEXITCODE -ne 0) { throw 'TypeScript/production build failed' }
} finally { Pop-Location }
