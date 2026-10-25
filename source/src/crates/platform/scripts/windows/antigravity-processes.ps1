# Candidate discovery only. Rust checks the process token and creation time.
# stdout is an in-memory pipe and may contain a CSRF token; never log it.
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
function Json-String([string]$value) {
    '"' + [regex]::Replace($value, '["\\\x00-\x1f]', {
        param($match)
        '\u{0:x4}' -f [int][char]$match.Value
    }) + '"'
}
trap { exit 1 }
$count = 0
foreach ($candidate in [Diagnostics.Process]::GetProcesses()) {
    try { $name = $candidate.ProcessName; $processId = $candidate.Id } catch { continue }
    if ($name -ne 'agy' -and $name -notlike 'language_server*' -and $name -notlike 'language-server*') { continue }
    if (++$count -gt 64) { exit 1 }
    try { $process = Get-CimInstance Win32_Process -Filter "ProcessId = $processId" } catch { continue }
    if ($null -eq $process.CreationDate -or [string]::IsNullOrEmpty($process.ExecutablePath) -or [string]::IsNullOrEmpty($process.CommandLine)) { continue }
    $record = '{"pid":' + $processId + ',"created":' + $process.CreationDate.ToUniversalTime().ToFileTimeUtc() +
        ',"executable":' + (Json-String $process.ExecutablePath) + ',"command":' + (Json-String $process.CommandLine) + '}'
    [Console]::Out.WriteLine($record)
}
