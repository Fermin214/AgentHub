# Parse only: never execute VM/installer code. Run in actual Windows PowerShell 5.1.
$ErrorActionPreference='Stop'
if($PSVersionTable.PSVersion.Major -ne 5 -or $PSVersionTable.PSVersion.Minor -ne 1){throw 'Requires Windows PowerShell 5.1'}
$project=Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$files=@('scripts/verify-windows-vm.ps1','scripts/verify-vm-portable.ps1','scripts/acceptance/vm-safety.ps1','scripts/acceptance/common.ps1','scripts/acceptance/worker.ps1','scripts/acceptance/languages.ps1','scripts/acceptance/runtime.ps1','scripts/acceptance/packaged.ps1','scripts/acceptance/desktop-fixture.ps1')
foreach($file in $files){
    $bytes=[IO.File]::ReadAllBytes((Join-Path $project $file))
    $bom=$bytes.Length -ge 3 -and $bytes[0] -eq 239 -and $bytes[1] -eq 187 -and $bytes[2] -eq 191
    # PS 5.1 honors a UTF-8 BOM; otherwise Western Windows decodes as ANSI 1252.
    # Decode explicitly to reproduce CI even on a host with UTF-8 as its ACP.
    $text=if($bom){[Text.Encoding]::UTF8.GetString($bytes,3,$bytes.Length-3)}else{[Text.Encoding]::GetEncoding(1252).GetString($bytes)}
    $tokens=$null;$errors=$null
    [void][Management.Automation.Language.Parser]::ParseInput($text,[ref]$tokens,[ref]$errors)
    if($errors.Count){throw "Windows PowerShell 5.1 / Windows-1252 parse failed in ${file}: $($errors[0].Message)"}
    # Also catch silent corruption of string contents that still parse correctly.
    if(-not $bom -and @($bytes|Where-Object {$_ -gt 127}).Count){throw "Non-ASCII source requires UTF-8 BOM in the Windows PowerShell loading chain: $file"}
    Write-Output "PASS Windows PowerShell 5.1 encoding: $file"
}
