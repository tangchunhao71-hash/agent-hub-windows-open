[CmdletBinding()]
param(
    [switch]$Check,
    [switch]$DebugBuild,
    [switch]$Bundle,
    [ValidateSet('desktop', 'engine')][string]$Component = 'desktop',
    [string]$RustBin,
    [string]$CargoHome,
    [string]$RustupHome,
    [string]$VcVars,
    [string]$PnpmCli,
    [string]$Bun,
    [string]$McpExecutable,
    [string]$McpManifest,
    [string]$Receipt,
    [string]$TargetDirectory
)
$ErrorActionPreference = 'Stop'
$taskSource = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../source'))
$taskPrior = @{}
Get-ChildItem Env: | ForEach-Object { $taskPrior[$_.Name] = $_.Value }
$taskExit = 2
try {
    if ($VcVars) {
        $taskVc = (Resolve-Path -LiteralPath $VcVars).Path
        if ($taskVc -match '["%!\r\n]' -or [IO.Path]::GetExtension($taskVc) -ne '.bat') { throw 'VcVars must be a literal .bat path without shell expansion characters' }
        # Read only the selected Visual Studio environment into this process.
        # Do not print the inherited environment or persist any user setting.
        $taskVars = & cmd.exe /d /s /c "call `"$taskVc`" >nul && set"
        if ($LASTEXITCODE -ne 0) { throw 'Visual Studio environment initialization failed' }
        foreach ($taskLine in $taskVars) {
            if ($taskLine -match '^([^=][^=]*)=(.*)$') { [Environment]::SetEnvironmentVariable($matches[1], $matches[2], 'Process') }
        }
    }
    if ($RustBin) { $env:PATH = (Resolve-Path -LiteralPath $RustBin).Path + [IO.Path]::PathSeparator + $env:PATH }
    foreach ($taskItem in @(
        @('CARGO_HOME', $CargoHome), @('RUSTUP_HOME', $RustupHome),
        @('AGENTHUD_PNPM_CLI', $PnpmCli), @('BUN_EXECUTABLE', $Bun),
        @('AGENTHUD_MCP_EXE', $McpExecutable), @('AGENTHUD_MCP_MANIFEST', $McpManifest),
        @('CARGO_TARGET_DIR', $TargetDirectory)
    )) {
        if ($taskItem[1]) { [Environment]::SetEnvironmentVariable($taskItem[0], [IO.Path]::GetFullPath($taskItem[1]), 'Process') }
    }
    $taskArgs = @((Join-Path $taskSource 'src/tooling/build.mjs'), '--platform', 'windows', '--component', $Component)
    if ($Check) { $taskArgs += '--check' }
    if ($DebugBuild) { $taskArgs += '--debug' }
    if ($Bundle) { $taskArgs += '--bundle' }
    if ($Receipt) { $taskArgs += @('--receipt', [IO.Path]::GetFullPath($Receipt)) }
    & node @taskArgs
    $taskExit = $LASTEXITCODE
} finally {
    foreach ($taskEntry in @(Get-ChildItem Env:)) {
        if (-not $taskPrior.ContainsKey($taskEntry.Name)) { [Environment]::SetEnvironmentVariable($taskEntry.Name, $null, 'Process') }
    }
    foreach ($taskKey in $taskPrior.Keys) { [Environment]::SetEnvironmentVariable($taskKey, $taskPrior[$taskKey], 'Process') }
}
exit $taskExit
