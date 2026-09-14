param([switch]$SkipBuild)
$ErrorActionPreference = 'Stop'
$agenthubPackageRoot = Split-Path $PSScriptRoot -Parent
if (-not $SkipBuild) { & (Join-Path $PSScriptRoot 'build.ps1') }
$agenthubVersion = (Get-Content -LiteralPath (Join-Path $agenthubPackageRoot 'src-tauri/tauri.conf.json') -Raw | ConvertFrom-Json).version
$agenthubReleaseFolder = Join-Path $agenthubPackageRoot "dist-portable/AgentHub-$agenthubVersion-windows-x64"
# A fresh directory prevents stale binaries from older layouts entering the archive.
if (Test-Path -LiteralPath $agenthubReleaseFolder) { throw "Package directory already exists: $agenthubReleaseFolder. Move it aside before packaging." }
$agenthubRuntimeFolder = Join-Path $agenthubReleaseFolder 'data/runtime'
New-Item -ItemType Directory -Force -Path $agenthubRuntimeFolder | Out-Null
Copy-Item -LiteralPath (Join-Path $agenthubPackageRoot 'target/release/AgentHub.exe') -Destination $agenthubReleaseFolder
Copy-Item -LiteralPath (Join-Path $agenthubPackageRoot 'src-tauri/resources/agenthub-layout.json') -Destination $agenthubRuntimeFolder
foreach ($agenthubFile in @('LICENSE','THIRD-PARTY-NOTICES.md')) {
    Copy-Item -LiteralPath (Join-Path $agenthubPackageRoot $agenthubFile) -Destination $agenthubRuntimeFolder
}
foreach ($agenthubFile in @('DEPENDENCY-NOTICES.md','NSIS-COPYING.txt','inventory.json','RUST-STDLIB-NOTICES.html')) {
    Copy-Item -LiteralPath (Join-Path $agenthubPackageRoot "licenses/$agenthubFile") -Destination $agenthubRuntimeFolder
}
$agenthubZipPath = "$agenthubReleaseFolder.zip"
Compress-Archive -Path (Join-Path $agenthubReleaseFolder '*') -DestinationPath $agenthubZipPath -Force
Get-FileHash -LiteralPath $agenthubZipPath -Algorithm SHA256 | Select-Object Hash,Path
