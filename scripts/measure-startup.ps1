param([Parameter(Mandatory)][string]$Exe, [Parameter(Mandatory)][string]$DataDir, [int]$ObserveSeconds = 20)
$ErrorActionPreference = 'Stop'
if ($ObserveSeconds -lt 1 -or $ObserveSeconds -gt 60) { throw 'ObserveSeconds must be between 1 and 60' }
if (Test-Path -LiteralPath $DataDir) { throw 'Use a new isolated data directory for startup measurement' }
New-Item -ItemType Directory -Path $DataDir -Force | Out-Null
$agenthubPreviousData = $env:AGENTHUB_DATA_DIR
$agenthubProcess = $null
try {
    $env:AGENTHUB_DATA_DIR = $DataDir
    $agenthubStarted = [DateTime]::UtcNow.ToString('o')
    $agenthubWatch = [Diagnostics.Stopwatch]::StartNew()
    $agenthubProcess = Start-Process -FilePath $Exe -WindowStyle Hidden -PassThru
    $agenthubWindowMs = $null
    while ($agenthubWatch.Elapsed.TotalSeconds -lt $ObserveSeconds) {
        $agenthubProcess.Refresh()
        if ($agenthubProcess.HasExited) { break }
        if ($agenthubProcess.MainWindowHandle -ne 0 -and $null -eq $agenthubWindowMs) {
            $agenthubWindowMs = $agenthubWatch.ElapsedMilliseconds
        }
        Start-Sleep -Milliseconds 100
    }
    $agenthubResult = [ordered]@{
        exe = (Resolve-Path -LiteralPath $Exe).Path
        exeSha256 = (Get-FileHash -LiteralPath $Exe).Hash
        dataDir = (Resolve-Path -LiteralPath $DataDir).Path
        startedUtc = $agenthubStarted
        observationMs = $agenthubWatch.ElapsedMilliseconds
        mainWindowHandleObservedMs = $agenthubWindowMs
        visibleWindowMs = $null
        uiInteractiveMs = $null
        backgroundDetectionCompleteMs = $null
        exited = $agenthubProcess.HasExited
        note = 'Hidden launch; MainWindowHandle is an OS signal, not proof of visible or interactive UI.'
    }
    $agenthubResult | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $DataDir 'measurement.json') -Encoding utf8
    $agenthubResult | ConvertTo-Json
} finally {
    if ($agenthubProcess -and -not $agenthubProcess.HasExited) {
        $agenthubProcess.CloseMainWindow() | Out-Null
        Start-Sleep -Milliseconds 500
        $agenthubProcess.Refresh()
        if (-not $agenthubProcess.HasExited) { Stop-Process -Id $agenthubProcess.Id }
    }
    $env:AGENTHUB_DATA_DIR = $agenthubPreviousData
}
