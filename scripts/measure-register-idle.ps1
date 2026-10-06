param(
    [Parameter(Mandatory = $true)][int]$ProcessId,
    [Parameter(Mandatory = $true)][string]$Transcript,
    [Parameter(Mandatory = $true)][string]$Output,
    [Parameter(Mandatory = $true)][string]$Scenario,
    [Parameter(Mandatory = $true)][string]$Configuration,
    [ValidateRange(5, 60)][int]$Seconds = 15
)

# Measure an already running interactive DebugTUI; this script never starts a
# debugger, connects a target, changes a configuration, or sends a request.
$ErrorActionPreference = 'Stop'
$taskProcess = Get-Process -Id $ProcessId
if ($taskProcess.ProcessName -ne 'debugtui') { throw 'Expected a live DebugTUI process' }
$taskBinary = $taskProcess.Path
$taskStartTime = $taskProcess.StartTime.ToUniversalTime().ToString('o')
$taskConfigurationHash = (Get-FileHash -LiteralPath $Configuration -Algorithm SHA256).Hash.ToLowerInvariant()
$taskBeforeLines = @(Get-Content -LiteralPath $Transcript -Encoding UTF8)
$taskCpuBefore = $taskProcess.TotalProcessorTime.TotalSeconds
$taskClock = [Diagnostics.Stopwatch]::StartNew()
$taskSamples = @()
for ($taskIndex = 0; $taskIndex -le $Seconds; $taskIndex++) {
    $taskProcess.Refresh()
    if ($taskProcess.HasExited) { throw 'DebugTUI exited during measurement' }
    $taskSamples += [pscustomobject]@{
        elapsed_seconds = $taskClock.Elapsed.TotalSeconds
        cpu_seconds = $taskProcess.TotalProcessorTime.TotalSeconds
        working_set_bytes = $taskProcess.WorkingSet64
        private_bytes = $taskProcess.PrivateMemorySize64
        handles = $taskProcess.HandleCount
        threads = $taskProcess.Threads.Count
    }
    if ($taskIndex -lt $Seconds) { Start-Sleep -Milliseconds 1000 }
}
$taskClock.Stop()
$taskAfterLines = @(Get-Content -LiteralPath $Transcript -Encoding UTF8)
if ((Get-FileHash -LiteralPath $Configuration -Algorithm SHA256).Hash.ToLowerInvariant() -ne $taskConfigurationHash) {
    throw 'Configuration changed during the idle interval'
}
if ($taskAfterLines.Count -lt $taskBeforeLines.Count) { throw 'MI transcript was truncated' }
for ($taskIndex = 0; $taskIndex -lt $taskBeforeLines.Count; $taskIndex++) {
    if ($taskBeforeLines[$taskIndex] -ne $taskAfterLines[$taskIndex]) {
        throw 'MI transcript prefix changed'
    }
}
$taskNewCommands = @($taskAfterLines | Select-Object -Skip $taskBeforeLines.Count)
$taskLast = $taskSamples[-1]
$taskFirst = $taskSamples[0]
$taskReport = [ordered]@{
    schema = 'debugtui-register-idle-1'
    scenario = $Scenario
    timestamp_utc = [DateTime]::UtcNow.ToString('o')
    board_tests_executed = $false
    measurement = 'live Windows interactive TUI process; software MI fixture'
    process_id = $ProcessId
    process_start_utc = $taskStartTime
    binary = $taskBinary
    binary_sha256 = (Get-FileHash -LiteralPath $taskBinary -Algorithm SHA256).Hash.ToLowerInvariant()
    configuration = (Resolve-Path -LiteralPath $Configuration).Path
    configuration_sha256 = $taskConfigurationHash
    requested_seconds = $Seconds
    elapsed_seconds = $taskClock.Elapsed.TotalSeconds
    cpu_seconds = $taskLast.cpu_seconds - $taskCpuBefore
    cpu_percent_of_one_logical_processor = 100 * ($taskLast.cpu_seconds - $taskCpuBefore) / $taskClock.Elapsed.TotalSeconds
    working_set_mib = $taskLast.working_set_bytes / 1MB
    private_mib = $taskLast.private_bytes / 1MB
    working_set_delta_bytes = $taskLast.working_set_bytes - $taskFirst.working_set_bytes
    private_delta_bytes = $taskLast.private_bytes - $taskFirst.private_bytes
    transcript = (Resolve-Path -LiteralPath $Transcript).Path
    commands_before = $taskBeforeLines.Count
    commands_after = $taskAfterLines.Count
    new_commands = $taskNewCommands
    samples = $taskSamples
}
$taskReport | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $Output -Encoding UTF8
if ($taskNewCommands.Count -ne 0) { throw 'Idle interval emitted MI commands; inspect the saved report' }
[pscustomobject]$taskReport | Select-Object scenario, elapsed_seconds, cpu_percent_of_one_logical_processor, working_set_mib, private_mib, working_set_delta_bytes, private_delta_bytes, commands_before, commands_after | ConvertTo-Json
