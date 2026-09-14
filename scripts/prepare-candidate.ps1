param([string]$Destination = 'output/release-candidate')
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot -Parent)
$version = (Get-Content package.json -Raw | ConvertFrom-Json).version
$sha = (& git rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0 -or $sha -notmatch '^[0-9a-f]{40}$') { throw 'Source commit unavailable' }
if (@(& git status --porcelain --untracked-files=no).Count) { throw 'Candidate requires a clean tracked source tree' }
if (Test-Path -LiteralPath $Destination) { throw 'Candidate destination already exists; do not overwrite verified bytes' }
$installer = "target/release/bundle/nsis/AgentHub_${version}_x64-setup.exe"
$portable = "dist-portable/AgentHub-$version-windows-x64.zip"
$exe = 'target/release/AgentHub.exe'
foreach ($file in @($installer, $exe)) {
    if ((Get-Item -LiteralPath $file).VersionInfo.ProductVersion -ne $version) { throw "Unexpected product version: $file" }
    if ((Get-AuthenticodeSignature -LiteralPath $file).Status -ne 'NotSigned') { throw "Candidate signature policy changed: $file" }
}
New-Item -ItemType Directory -Path $Destination | Out-Null
foreach ($file in @($installer, $portable)) { Copy-Item -LiteralPath $file -Destination $Destination }
$files = @(Get-ChildItem -LiteralPath $Destination -File | Sort-Object Name | ForEach-Object {
    [ordered]@{name=$_.Name; bytes=$_.Length; sha256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant(); signature=if ($_.Extension -eq '.exe') {'NotSigned'} else {'not-applicable; contained executable NotSigned'}}
})
$manifest = [ordered]@{
    schemaVersion=1; product='AgentHub'; version=$version; sourceCommit=$sha
    repository=$env:GITHUB_REPOSITORY; ciRunId=$env:GITHUB_RUN_ID; ciRunAttempt=$env:GITHUB_RUN_ATTEMPT
    createdAt=(Get-Date).ToUniversalTime().ToString('o')
    environment=[ordered]@{os=[Environment]::OSVersion.VersionString; arch='x86_64-pc-windows-msvc'; runnerImage=$env:ImageOS; runnerImageVersion=$env:ImageVersion; node=(& node --version); rust=(& rustc --version); cargo=(& cargo --version)}
    signaturePolicy='Unsigned Windows application. Windows may show publisher or reputation warnings, and device policy may block execution.'
    desktop=[ordered]@{name='AgentHub.exe';sha256=(Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash.ToLowerInvariant();version=$version;signature='NotSigned'}
    files=$files; windowsAcceptance='pending; must validate these exact file hashes before publication'
}
$manifest | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath "$Destination/build-manifest.json" -Encoding utf8NoBOM
$checksums = @($files | ForEach-Object { "$($_.sha256)  $($_.name)" })
$checksums += "$((Get-FileHash -LiteralPath "$Destination/build-manifest.json" -Algorithm SHA256).Hash.ToLowerInvariant())  build-manifest.json"
$checksums | Set-Content -LiteralPath "$Destination/SHA256SUMS.txt" -Encoding utf8NoBOM
Write-Output "Candidate saved: $Destination; source $sha"
