[CmdletBinding(SupportsShouldProcess)]
param()
$ErrorActionPreference = 'Stop'
$agenthubRoot = (Resolve-Path -LiteralPath (Split-Path $PSScriptRoot -Parent)).Path
$agenthubConfig = Get-Content -LiteralPath (Join-Path $agenthubRoot 'src-tauri/tauri.conf.json') -Raw | ConvertFrom-Json
$agenthubVersion = [version]$agenthubConfig.version
$agenthubReportPath = Join-Path $agenthubRoot "output/release-verification-$agenthubVersion.json"
$agenthubReport = Get-Content -LiteralPath $agenthubReportPath -Raw | ConvertFrom-Json
if ($agenthubReport.version -ne $agenthubVersion.ToString() -or $agenthubReport.desktop.status -ne 'passed' -or $agenthubReport.portable.status -ne 'passed' -or $agenthubReport.installer.build -ne 'passed') {
    throw 'Keep previous releases until the current desktop, portable package and installer are verified.'
}
foreach ($agenthubArtifact in @($agenthubReport.portable, $agenthubReport.installer)) {
    $agenthubArtifactPath = (Resolve-Path -LiteralPath (Join-Path $agenthubRoot $agenthubArtifact.file)).Path
    if (-not $agenthubArtifactPath.StartsWith($agenthubRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'Release artifact is outside this repository.' }
    if ((Get-FileHash -LiteralPath $agenthubArtifactPath -Algorithm SHA256).Hash -ne $agenthubArtifact.sha256) { throw 'Current release artifact changed after verification.' }
}
$agenthubRemoved = @()
foreach ($agenthubRelative in @('dist-portable', 'target/release/bundle/nsis')) {
    $agenthubDirectory = (Resolve-Path -LiteralPath (Join-Path $agenthubRoot $agenthubRelative)).Path
    foreach ($agenthubItem in Get-ChildItem -LiteralPath $agenthubDirectory) {
        $agenthubPattern = if ($agenthubRelative -eq 'dist-portable') { '^AgentHub-(\d+\.\d+\.\d+)-windows-x64(?:\.zip)?$' } else { '^AgentHub_(\d+\.\d+\.\d+)_x64-setup\.exe$' }
        if ($agenthubItem.Name -notmatch $agenthubPattern -or [version]$Matches[1] -ge $agenthubVersion) { continue }
        $agenthubTarget = (Resolve-Path -LiteralPath $agenthubItem.FullName).Path
        if (-not $agenthubTarget.StartsWith($agenthubDirectory + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'Cleanup target is outside the release directory.' }
        if (($agenthubItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'Release cleanup does not follow directory links.' }
        if ($agenthubItem.PSIsContainer) {
            if (Get-ChildItem -LiteralPath $agenthubTarget -Recurse -Force | Where-Object { ($_.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 }) { throw 'Release directory contains a link; kept intact.' }

        }
        if ($PSCmdlet.ShouldProcess($agenthubTarget, 'Remove superseded release artifact')) {
            Remove-Item -LiteralPath $agenthubTarget -Recurse -Force
            $agenthubRemoved += $agenthubTarget
        }
    }
}

$agenthubOutputRoot = (Resolve-Path -LiteralPath (Join-Path $agenthubRoot 'output')).Path
$agenthubDesktopReport = Get-Content -LiteralPath (Join-Path $agenthubOutputRoot "desktop-verification-$agenthubVersion.json") -Raw | ConvertFrom-Json
$agenthubCurrentScreenshots = (Resolve-Path -LiteralPath $agenthubDesktopReport.reportDir).Path
if (-not $agenthubCurrentScreenshots.StartsWith((Join-Path $agenthubOutputRoot 'playwright') + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'Current screenshots must be within this repository output directory.' }
$agenthubTemporary = @()
foreach ($agenthubItem in Get-ChildItem -LiteralPath $agenthubOutputRoot) {
    if (-not $agenthubItem.PSIsContainer -and $agenthubItem.Extension -in @('.log', '.json') -and $agenthubItem.Name -notlike "*$agenthubVersion*") { $agenthubTemporary += $agenthubItem.FullName }
    if ($agenthubItem.PSIsContainer -and ($agenthubItem.Name -like 'icon-build-*' -or $agenthubItem.Name -eq 'installer-tests')) { $agenthubTemporary += $agenthubItem.FullName }
}
foreach ($agenthubItem in Get-ChildItem -LiteralPath (Join-Path $agenthubOutputRoot 'playwright') -Directory) {
    if ($agenthubItem.FullName -ne $agenthubCurrentScreenshots) { $agenthubTemporary += $agenthubItem.FullName }
}
$agenthubTemporary += @(Get-ChildItem -LiteralPath $agenthubCurrentScreenshots -Directory | Select-Object -ExpandProperty FullName)
$agenthubFixtureRoot = Join-Path $agenthubRoot '.test-data'
if (Test-Path -LiteralPath $agenthubFixtureRoot) {
    $agenthubTemporary += @(Get-ChildItem -LiteralPath $agenthubFixtureRoot -Directory | Where-Object Name -Match '^portable-[a-f0-9-]+$' | Select-Object -ExpandProperty FullName)
}
foreach ($agenthubItem in $agenthubTemporary) {
    $agenthubTarget = (Resolve-Path -LiteralPath $agenthubItem).Path
    if (-not ($agenthubTarget.StartsWith($agenthubOutputRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase) -or $agenthubTarget.StartsWith($agenthubFixtureRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase))) { throw 'Temporary output cleanup escaped its verified roots.' }
    if ($PSCmdlet.ShouldProcess($agenthubTarget, 'Delete obsolete development or test output')) {
        Remove-Item -LiteralPath $agenthubTarget -Recurse -Force
        $agenthubRemoved += $agenthubTarget
    }
}
[pscustomobject]@{ Version = $agenthubVersion.ToString(); Removed = $agenthubRemoved } | ConvertTo-Json -Depth 3
