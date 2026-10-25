[CmdletBinding()]
param([string]$DshHome = $(if ($env:DSH_HOME) { $env:DSH_HOME } else { Join-Path $HOME '.dsh' }))
$ErrorActionPreference = 'Stop'
$taskSource = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../source/integrations/dsh-navigation'))
$taskHome = (Resolve-Path -LiteralPath $DshHome).Path
$taskPatch = Join-Path $taskHome 'profiles/desktop/cordis.patch.yml'
if (-not (Test-Path -LiteralPath $taskPatch -PathType Leaf)) { throw 'DSH desktop patch not found' }
$taskTarget = Join-Path $taskHome 'plugins/agent-hub-dsh-navigation'
$taskUri = ([Uri](Join-Path $taskTarget 'index.js')).AbsoluteUri
$taskText = [IO.File]::ReadAllText($taskPatch)
if ($taskText -match '(?m)^\s+- id: agent-hub-navigation\s*$') { throw 'Navigation entry already exists; review it before updating' }
$taskStamp = Get-Date -Format 'yyyyMMdd-HHmmss-fff'
$taskBackup = "$taskPatch.bak-agent-hub-$taskStamp"
Copy-Item -LiteralPath $taskPatch -Destination $taskBackup
if (Test-Path -LiteralPath $taskTarget) { throw 'Plugin directory already exists; no files overwritten' }
New-Item -ItemType Directory -Path $taskTarget | Out-Null
foreach ($taskName in @('package.json','index.js','client.js')) {
    Copy-Item -LiteralPath (Join-Path $taskSource $taskName) -Destination (Join-Path $taskTarget $taskName)
}
$taskEntry = "`n# Agent Hub: local-only exact-session navigation`n- insert:`n    - id: agent-hub-navigation`n      name: '$taskUri'`n"
if ([IO.File]::ReadAllText($taskPatch) -cne $taskText) { throw 'DSH patch changed concurrently; original remains untouched' }
[IO.File]::WriteAllText($taskPatch, $taskText.TrimEnd() + "`n" + $taskEntry, [Text.UTF8Encoding]::new($false))
[pscustomobject]@{ installed=$true; plugin=$taskTarget; backup=$taskBackup; restartMayBeRequired=$true } | ConvertTo-Json
