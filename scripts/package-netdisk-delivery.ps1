[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Receipt,
    [Parameter(Mandatory)][string]$Installer,
    [Parameter(Mandatory)][string]$OutputDirectory
)
$ErrorActionPreference = 'Stop'
$taskRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$taskConfig = Get-Content -Raw -LiteralPath (Join-Path $taskRoot 'source/src/apps/desktop/src-tauri/tauri.conf.json') | ConvertFrom-Json
$taskReceipt = Get-Content -Raw -LiteralPath $Receipt | ConvertFrom-Json
$taskCommit = (& git -C $taskRoot rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0) { throw 'Cannot read source commit' }
$taskDirty = & git -C $taskRoot status --porcelain --untracked-files=normal
if ($LASTEXITCODE -ne 0 -or $taskDirty) { throw 'Commit reviewed changes before packaging' }
if ($taskReceipt.status -ne 'built' -or $taskReceipt.exitCode -ne 0 -or -not $taskReceipt.sourceStable -or $taskReceipt.source.dirty -or $taskReceipt.source.head -ne $taskCommit) { throw 'Successful stable build from the current clean commit is required' }
$taskInstaller = (Resolve-Path -LiteralPath $Installer).Path
$taskOriginalName = "$($taskConfig.productName)_$($taskConfig.version)_x64-setup.exe"
$taskRecord = @($taskReceipt.artifacts | Where-Object { [IO.Path]::GetFileName($_.path) -eq $taskOriginalName })
if ($taskRecord.Count -ne 1) { throw 'Build receipt must contain this version installer exactly once' }
$taskHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $taskInstaller).Hash.ToLowerInvariant()
if ($taskHash -ne $taskRecord[0].sha256 -or (Get-Item -LiteralPath $taskInstaller).Length -ne $taskRecord[0].bytes) { throw 'Installer does not match its build receipt' }
$taskOutput = [IO.Path]::GetFullPath($OutputDirectory)
$taskAllowed = [IO.Path]::GetFullPath((Join-Path $taskRoot 'local-only')) + [IO.Path]::DirectorySeparatorChar
if (-not $taskOutput.StartsWith($taskAllowed, [StringComparison]::OrdinalIgnoreCase)) { throw 'Review package must be under this repository local-only directory' }
if ((Test-Path -LiteralPath $taskOutput) -or (Test-Path -LiteralPath "$taskOutput.zip")) { throw 'Never overwrite an earlier delivery' }
New-Item -ItemType Directory -Path $taskOutput | Out-Null
$taskName = "Agent-Bot-$($taskConfig.version)-windows-x64-setup.exe"
Copy-Item -LiteralPath $taskInstaller -Destination (Join-Path $taskOutput $taskName)
$taskFiles = [ordered]@{
    'docs/release/QUICKSTART.txt' = '先读我.txt'
    'docs/release/INSTALL.md' = 'INSTALL.md'
    'docs/release/PRIVACY.md' = 'PRIVACY.md'
    'docs/release/CHANGELOG.md' = 'CHANGELOG.md'
    'docs/release/ACCEPTANCE.md' = 'ACCEPTANCE.md'
    'source/LICENSE' = 'LICENSE'
    'source/docs/third-party.md' = 'THIRD_PARTY.md'
    'source/docs/dependency-notices.txt' = 'DEPENDENCY_NOTICES.txt'
    'source/docs/release-license-supplement.txt' = 'RELEASE_LICENSE_SUPPLEMENT.txt'
    'source/src/packages/hud/src/assets/THIRD_PARTY_NOTICES.txt' = 'THIRD_PARTY_NOTICES.txt'
    'source/src/packages/hud/src/assets/LobeIcons-LICENSE.txt' = 'LobeIcons-LICENSE.txt'
}
foreach ($taskFile in $taskFiles.GetEnumerator()) { Copy-Item -LiteralPath (Join-Path $taskRoot $taskFile.Key) -Destination (Join-Path $taskOutput $taskFile.Value) }
$taskManifest = [ordered]@{
    schemaVersion = 1; product = 'Agent Bot'; version = $taskConfig.version
    status = 'local-review-candidate'; saleApproved = $false; published = $false
    sourceCommit = $taskCommit; sourceStable = $true; sourceDirty = $false
    installer = @{ file = $taskName; bytes = $taskRecord[0].bytes; sha256 = $taskHash; signature = (Get-AuthenticodeSignature -LiteralPath $taskInstaller).Status.ToString() }
    updateMode = 'manual-shared-folder'
    downloadUrlConfigured = [bool]((Get-Content -Raw -LiteralPath (Join-Path $taskRoot 'source/src/release-channel.json') | ConvertFrom-Json).downloadUrl)
    excluded = @('accounts and chats', 'local experiments and credentials', 'Bun MCP executable', 'seller listing draft', 'raw build receipts')
    pending = @('Clean Windows install, WebView2 recovery, upgrade, uninstall and rollback', 'Dependency version/license correspondence and artwork usage review', 'Current marketplace category and delivery rules')
}
$taskManifest | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $taskOutput 'release-manifest.json') -Encoding utf8
$taskSums = @(Get-ChildItem -LiteralPath $taskOutput -File | Sort-Object Name | ForEach-Object { '{0}  {1}' -f (Get-FileHash -Algorithm SHA256 -LiteralPath $_.FullName).Hash.ToLowerInvariant(), $_.Name })
[IO.File]::WriteAllText((Join-Path $taskOutput 'SHA256SUMS.txt'), ($taskSums -join "`n") + "`n", [Text.UTF8Encoding]::new($false))
Add-Type -AssemblyName System.IO.Compression.FileSystem
[IO.Compression.ZipFile]::CreateFromDirectory($taskOutput, "$taskOutput.zip")
$taskZip = [IO.Compression.ZipFile]::OpenRead("$taskOutput.zip")
try {
    $taskExpected = @($taskName) + @($taskFiles.Values) + @('release-manifest.json', 'SHA256SUMS.txt')
    if (Compare-Object ($taskExpected | Sort-Object) (@($taskZip.Entries.FullName) | Sort-Object)) { throw 'Unexpected ZIP contents' }
    foreach ($taskEntry in $taskZip.Entries) {
        $taskStream = $taskEntry.Open(); $taskSha = [Security.Cryptography.SHA256]::Create()
        try { $taskZipHash = [BitConverter]::ToString($taskSha.ComputeHash($taskStream)).Replace('-', '') } finally { $taskStream.Dispose(); $taskSha.Dispose() }
        if ($taskZipHash -ne (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $taskOutput $taskEntry.FullName)).Hash) { throw 'ZIP content hash mismatch' }
    }
} finally { $taskZip.Dispose() }
[ordered]@{ directory = $taskOutput; zip = "$taskOutput.zip"; entries = $taskExpected.Count; sourceCommit = $taskCommit; sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath "$taskOutput.zip").Hash.ToLowerInvariant(); saleApproved = $false } | ConvertTo-Json
