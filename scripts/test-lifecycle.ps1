param([Parameter(Mandatory=$true)][string]$Elf)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path $PSScriptRoot
$binary = Join-Path $projectRoot 'target\release\debugtui.exe'
$toolRoot = Join-Path $projectRoot 'tools'
$runRoot = Join-Path $projectRoot ('artifacts\lifecycle-' + (Get-Date -Format yyyyMMdd-HHmmss))
New-Item -ItemType Directory -Path $runRoot -Force | Out-Null
$Elf = (Resolve-Path -LiteralPath $Elf).Path
'{"id":1,"method":"connect"}' | Set-Content "$runRoot\connect.jsonl" -Encoding utf8

function Invoke-Headless([string]$Name, [string[]]$Extra, [int]$ExpectedExit) {
    & $binary --tools-dir $toolRoot --elf $Elf --script "$runRoot\connect.jsonl" @Extra > "$runRoot\$Name.events.jsonl" 2> "$runRoot\$Name.errors.txt"
    if ($LASTEXITCODE -ne $ExpectedExit) { throw "$Name returned $LASTEXITCODE instead of $ExpectedExit" }
    $events = @(Get-Content "$runRoot\$Name.events.jsonl" | ForEach-Object { $_ | ConvertFrom-Json })
    # Invalid environment files fail before a session or child process is created.
    if ($ExpectedExit -ne 0 -and $events.Count -eq 0) {
        if (-not (Get-Content "$runRoot\$Name.errors.txt" -Raw)) { throw "$Name failed without diagnostics" }
        return
    }
    if (-not ($events | Where-Object { $_.event -eq 'response' -and $_.id -eq [uint64]::MaxValue }).ok) { throw "$Name cleanup failed" }
    $events
}
$null = Invoke-Headless 'missing-elf' @('--elf', "$runRoot\missing.elf") 1
$null = Invoke-Headless 'missing-tools' @('--tools-dir', "$runRoot\missing-tools") 1
$null = Invoke-Headless 'refused-port' @('--connect', '127.0.0.1:3349') 1
Write-Output 'PASS invalid inputs: missing ELF/tools and refused server return failure with cleanup.'

# An OpenOCD started here is external to DebugTUI's process job.
$openocd = Join-Path $toolRoot 'bin\openocd\bin\openocd.exe'
$serverArgs = '-s "{0}" -f "{1}"' -f (Join-Path $toolRoot 'bin\openocd\scripts'), (Join-Path $toolRoot 'config\stm32f429-live.cfg')
$server = Start-Process -FilePath $openocd -ArgumentList $serverArgs -WindowStyle Hidden -RedirectStandardOutput "$runRoot\external-server.log" -RedirectStandardError "$runRoot\external-server.err" -PassThru
$null = $server.Handle
try {
    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    do {
        # OpenOCD logs to stderr.
        $ready = (Get-Content "$runRoot\external-server.err" -Raw -ErrorAction SilentlyContinue) -match 'Listening on port 3333 for gdb connections'
        if ($server.HasExited) { throw 'External server exited before ready' }
        if ([DateTime]::UtcNow -gt $deadline) { throw 'External server startup timed out' }
        if (-not $ready) { Start-Sleep -Milliseconds 50 }
    } until ($ready)
    $null = Invoke-Headless 'occupied-probe' @() 1
    if ($server.HasExited) { throw 'Failed managed connection killed the external server' }
    $external = @(Invoke-Headless 'external' @('--connect', '127.0.0.1:3333') 0)
    if (-not ($external | Where-Object { $_.event -eq 'response' -and $_.id -eq 1 }).ok) { throw 'External connection failed' }
    # OpenOCD keeps serving after GDB detaches; DebugTUI must not have stopped it.
    if ($server.HasExited) { throw 'External session terminated the external server' }
    Write-Output 'PASS external: OpenOCD startup, occupied probe failure preserves external process, --connect works, server survives the session.'
} finally {
    if (-not $server.HasExited) { $server.Kill() }
}

$info = [System.Diagnostics.ProcessStartInfo]::new($binary)
$info.UseShellExecute = $false
$info.CreateNoWindow = $true
$info.RedirectStandardInput = $true
$info.RedirectStandardOutput = $true
$info.RedirectStandardError = $true
$info.Arguments = '--tools-dir "{0}" --elf "{1}" --headless --stdio' -f $toolRoot,$Elf
$app = [System.Diagnostics.Process]::Start($info)
$errors = $app.StandardError.ReadToEndAsync()
$output = [System.Collections.Generic.List[string]]::new()
try {
    $app.StandardInput.WriteLine('{"id":1,"method":"connect"}')
    $app.StandardInput.Flush()
    $deadline = [DateTime]::UtcNow.AddSeconds(20)
    do {
        $line = $app.StandardOutput.ReadLineAsync()
        if (-not $line.Wait(20000)) { throw 'Forced-exit setup timed out' }
        if ($null -eq $line.Result) { throw 'Forced-exit setup closed unexpectedly' }
        $output.Add($line.Result)
        $event = $line.Result | ConvertFrom-Json
        if ([DateTime]::UtcNow -gt $deadline) { throw 'Forced-exit setup deadline exceeded' }
    } until ($event.event -eq 'response' -and $event.id -eq 1)
    if (-not $event.ok) { throw "Forced-exit connection: $($event.error)" }
    $owned = @(Get-CimInstance Win32_Process -Filter "Name='openocd.exe' OR Name='arm-none-eabi-gdb.exe'" | Where-Object ParentProcessId -eq $app.Id)
    if ($owned.Count -ne 2) { throw 'Expected one owned GDB and one owned server' }
    $app.Kill()
    if (-not $app.WaitForExit(5000)) { throw 'Owned application did not terminate' }
    $deadline = [DateTime]::UtcNow.AddSeconds(5)
    do {
        $left = @($owned | Where-Object { Get-Process -Id $_.ProcessId -ErrorAction SilentlyContinue })
        if (-not $left.Count) { break }
        if ([DateTime]::UtcNow -gt $deadline) { throw 'Job object left child processes behind' }
        Start-Sleep -Milliseconds 50
    } while ($true)
    $output.Add($app.StandardOutput.ReadToEnd())
    $output | Set-Content "$runRoot\forced-exit.events.jsonl" -Encoding utf8
    $errors.Result | Set-Content "$runRoot\forced-exit.errors.txt" -Encoding utf8
    Write-Output 'PASS forced termination: killing the owned application also terminates its exact GDB/server children.'
} finally { if (-not $app.HasExited) { $app.Kill() } }
$null = Invoke-Headless 'recover-after-kill' @() 0
Write-Output "PASS recovery: fresh connection succeeds after forced termination; target resumed on cleanup. Artifacts: $runRoot"
