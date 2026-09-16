param([string]$Binary = (Join-Path (Split-Path $PSScriptRoot) 'target/release/debugtui.exe'),[switch]$ExpectLegacyFailure)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path $PSScriptRoot
$runRoot = Join-Path $projectRoot ('artifacts/pause-' + (Get-Date -Format yyyyMMdd-HHmmss))
New-Item -ItemType Directory -Path $runRoot | Out-Null
$node = (Get-Command node.exe).Source.Replace('\','/')
$fixture = (Join-Path $projectRoot 'tests/mock-gdb.cjs').Replace('\','/')
foreach ($mode in @('missing','already-stopped','late','retry','running')) {
    $transcript = "$runRoot/$mode.mi.txt".Replace('\','/')
    @"
version = 2
[gdb]
executable = '$node'
args = ['$fixture']
[gdb.env]
DEBUGTUI_TEST_REGISTERS = '["pc","sp"]'
DEBUGTUI_TEST_TRANSCRIPT = '$transcript'
DEBUGTUI_TEST_PAUSE = '$mode'
[target]
mode = 'remote'
endpoint = 'localhost:1234'
"@ | Set-Content "$runRoot/$mode.toml" -Encoding utf8
    $commands = @(
        @{id=1;method='connect'},
        @{id=2;method='continue'},
        @{id=3;method='pause'},
        @{id=4;method='status'},
        @{id=5;method='step'},
        @{id=6;method='pause'},
        @{id=7;method='disconnect'}
    )
    $commands | ForEach-Object { $_ | ConvertTo-Json -Compress } | Set-Content "$runRoot/$mode.jsonl" -Encoding utf8
    $timer = [Diagnostics.Stopwatch]::StartNew()
    & $Binary --project "$runRoot/$mode.toml" --script "$runRoot/$mode.jsonl" > "$runRoot/$mode.events.jsonl" 2> "$runRoot/$mode.errors.txt"
    $code = $LASTEXITCODE
    $events = @(Get-Content "$runRoot/$mode.events.jsonl" | ForEach-Object { $_ | ConvertFrom-Json })
    $pause = $events | Where-Object { $_.event -eq 'response' -and $_.id -eq 3 }
    if ($ExpectLegacyFailure) {
        if ($pause.ok -or $pause.error -notmatch 'Timed out waiting for target to stop') { throw 'Legacy failure was not reproduced' }
        Write-Output "REPRODUCED legacy pause timeout with a stopped GDB thread ($($timer.ElapsedMilliseconds) ms). $runRoot"
        break
    }
    if ($mode -eq 'running') {
        if ($code -ne 1 -or $pause.ok -or $pause.error -notmatch 'still.*running') { throw 'A genuinely running target was incorrectly marked stopped' }
        if ($timer.Elapsed.TotalSeconds -gt 15) { throw 'Timeout was not bounded' }
        Write-Output 'PASS genuinely running target: bounded timeout; no false stopped state.'
        continue
    }
    if ($code -ne 0 -or -not $pause.ok) { throw "$mode pause failed: $($pause.error)" }
    $status = $events | Where-Object { $_.event -eq 'response' -and $_.id -eq 4 }
    if ($status.result.state -ne 'STOPPED' -or $status.result.frame.function -ne 'main') { throw "$mode did not refresh stopped context" }
    $mi = @(Get-Content $transcript)
    if (@($mi | Where-Object { $_ -match '^-target-select' }).Count -ne 1) { throw 'Recovery unexpectedly reconnected' }
    if ($mi -notcontains '-exec-step') { throw 'Session could not step after pause' }
    if ($mi -match 'monitor|maintenance|reset') { throw 'Vendor-specific fallback leaked into generic pause' }
    Write-Output "PASS ${mode}: pause recovers, context refreshes, step works in the same connection ($($timer.ElapsedMilliseconds) ms)."
}
Write-Output "Pause test artifacts: $runRoot"
