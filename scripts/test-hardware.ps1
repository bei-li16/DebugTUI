param(
    [Parameter(Mandatory=$true)][string]$Elf,
    [string]$Tools = (Join-Path (Split-Path $PSScriptRoot) 'tools'),
    [string]$Binary = (Join-Path (Split-Path $PSScriptRoot) 'target\release\debugtui.exe'),
    [int]$Port = 3333,
    [switch]$Download
)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path $PSScriptRoot
$runRoot = Join-Path $projectRoot ('artifacts\hardware-' + (Get-Date -Format yyyyMMdd-HHmmss))
New-Item -ItemType Directory -Path $runRoot -Force | Out-Null
$Elf = (Resolve-Path -LiteralPath $Elf).Path
$Tools = (Resolve-Path -LiteralPath $Tools).Path
$Binary = (Resolve-Path -LiteralPath $Binary).Path
$elfToml = $Elf.Replace('\','/') | ConvertTo-Json -Compress
$toolsToml = $Tools.Replace('\','/') | ConvertTo-Json -Compress
$runToml = $runRoot.Replace('\','/') | ConvertTo-Json -Compress
@"
version = 1
watch = ["xTickCount", "g_w25q_jedec_id"]
[tools]
root = $toolsToml
[program]
elf = $elfToml
[server]
port = $Port
[session]
on_exit = "resume"
log_dir = $runToml
"@ | Set-Content "$runRoot\project.toml" -Encoding utf8

function Invoke-Scenario([string]$Name, [object[]]$Commands, [int]$ExpectedExit = 0) {
    $commandFile = Join-Path $runRoot "$Name.jsonl"
    $Commands | ForEach-Object { $_ | ConvertTo-Json -Depth 8 -Compress } | Set-Content $commandFile -Encoding utf8
    $child = Start-Process -FilePath $Binary -ArgumentList ('--project "{0}\project.toml" --script "{1}"' -f $runRoot,$commandFile) -WindowStyle Hidden -RedirectStandardOutput "$runRoot\$Name.events.jsonl" -RedirectStandardError "$runRoot\$Name.errors.txt" -PassThru
    $null = $child.Handle
    $completed = $false
    for ($attempt = 0; $attempt -lt 4; $attempt++) {
        if ($child.WaitForExit(30000)) { $completed = $true; break }
        Write-Output "Waiting for $Name (30 seconds elapsed)..." | Out-Host
    }
    if (-not $completed) { $child.Kill(); throw "$Name timed out" }
    if ($child.ExitCode -ne $ExpectedExit) { throw "$Name exit=$($child.ExitCode): $(Get-Content "$runRoot\$Name.errors.txt" -Raw)" }
    $events = @(Get-Content "$runRoot\$Name.events.jsonl" | ForEach-Object { $_ | ConvertFrom-Json })
    $responses = @($events | Where-Object event -eq response)
    if ($ExpectedExit -eq 0 -and @($responses | Where-Object { -not $_.ok }).Count -ne 0) { throw "$Name returned a failed command" }
    if ($ExpectedExit -eq 0 -and $responses.Count -ne $Commands.Count + 1) { throw "$Name did not complete all commands and cleanup" }
    $events
}

$core = @(
 @{id=1;method='connect'},
 @{id=2;method='evaluate';params=@{expression='g_w25q_jedec_id'}},
 @{id=3;method='break';params=@{location='DEBUG_PRINTF';temporary=$true}},
 @{id=4;method='continue'}, @{id=5;method='wait_stopped'},
 @{id=6;method='step'}, @{id=7;method='wait_stopped'},
 @{id=8;method='stepi'}, @{id=9;method='wait_stopped'},
 @{id=10;method='data_break';params=@{expression='Log_Tx_En'}},
 @{id=11;method='continue'}, @{id=12;method='wait_stopped'},
 @{id=13;method='memory';params=@{address='&Log_Tx_En';count=32}},
 @{id=14;method='disassemble'}, @{id=15;method='files'},
 @{id=16;method='frame';params=@{level=1}}, @{id=17;method='frame';params=@{level=0}},
 @{id=18;method='delete_break';params=@{number=''}},
 @{id=19;method='continue'}, @{id=20;method='pause'},
 @{id=21;method='restart'},
 @{id=22;method='break';params=@{location='main';temporary=$true}},
 @{id=23;method='continue'}, @{id=24;method='wait_stopped'},
 @{id=25;method='status'}
)
$events = @(Invoke-Scenario 'core' $core)
$responses = @{}
foreach ($response in $events | Where-Object event -eq response) { $responses[[string]$response.id] = $response }
if (-not $responses['1'].result.async_supported) { throw 'Target does not support asynchronous MI' }
if ($responses['2'].result.value -ne '15679512') { throw 'Unexpected JEDEC ID' }
if ($responses['5'].result.frame.function -ne 'DEBUG_PRINTF') { throw 'Source breakpoint was not reached' }
if ($responses['7'].result.frame.line -eq $responses['5'].result.frame.line) { throw 'Source step did not advance' }
if ($responses['9'].result.frame.address -eq $responses['7'].result.frame.address) { throw 'Instruction step did not advance' }
if ($responses['12'].result.reason -ne 'watchpoint-trigger') { throw 'Hardware watchpoint did not trigger' }
if ($responses['24'].result.frame.function -ne 'main') { throw 'Reset did not reach main' }
if (@($events | Where-Object { $_.event -eq 'log' -and $_.channel -eq 'stop' -and $_.text -match '->' }).Count -eq 0) { throw 'Watchpoint values were not recorded' }
Write-Output 'PASS core: MI async, symbols, breakpoint, source/instruction step, data breakpoint, memory, assembly, source list, frames, pause and reset.'

$failure = @(
 @{id=1;method='connect'},
 @{id=2;method='evaluate';params=@{expression='debugtui_missing_symbol_123'}},
 @{id=3;method='continue'}
)
$errorEvents = @(Invoke-Scenario 'invalid-expression' $failure 1)
if (@($errorEvents | Where-Object { $_.event -eq 'response' -and $_.id -eq 3 }).Count) { throw 'Script continued after failure' }
if (-not ($errorEvents | Where-Object { $_.event -eq 'response' -and $_.id -eq [uint64]::MaxValue }).ok) { throw 'Failure cleanup did not resume and disconnect' }
Write-Output 'PASS failure: invalid expression exits nonzero, later commands are skipped, target cleanup succeeds.'

$reconnect = @(@{id=1;method='connect'},@{id=2;method='disconnect'},@{id=3;method='connect'},@{id=4;method='evaluate';params=@{expression='xTickCount'}})
$null = Invoke-Scenario 'reconnect' $reconnect
Write-Output 'PASS reconnect: fresh server session reconnects.'

$execution = @(
 @{id=1;method='connect'},
 @{id=2;method='break';params=@{location='DEBUG_PRINTF';temporary=$true}},
 @{id=3;method='continue'}, @{id=4;method='wait_stopped'},
 @{id=5;method='next'}, @{id=6;method='wait_stopped'},
 @{id=7;method='finish'}, @{id=8;method='wait_stopped'},
 @{id=9;method='console';params=@{command='p/x g_w25q_jedec_id'}},
 @{id=10;method='break';params=@{location='main';temporary=$true}},
 @{id=11;method='console';params=@{command='run'}}, @{id=12;method='wait_stopped'}
)
$executionEvents = @(Invoke-Scenario 'execution' $execution)
$finish = $executionEvents | Where-Object { $_.event -eq 'response' -and $_.id -eq 8 }
$reset = $executionEvents | Where-Object { $_.event -eq 'response' -and $_.id -eq 12 }
if ($finish.result.frame.function -ne 'Task100ms' -or $reset.result.frame.function -ne 'main') { throw 'Finish or run alias did not reach expected source location' }
if (-not ($executionEvents | Where-Object { $_.event -eq 'log' -and $_.channel -eq 'gdb' -and $_.text -match '0xef4018' })) { throw 'GDB console result was not emitted' }
Write-Output 'PASS execution: next, finish to caller, raw GDB result, run alias resets and executes to main.'

if ($Download) {
    $flash = @(@{id=1;method='connect'},@{id=2;method='download'},@{id=3;method='console';params=@{command='compare-sections -r'}})
    $flashEvents = @(Invoke-Scenario 'download' $flash)
    $matches = @($flashEvents | Where-Object { $_.event -eq 'log' -and $_.text -match 'Section .*matched\.' })
    if ($matches.Count -lt 6) { throw 'Downloaded image did not verify all read-only sections' }
    Write-Output 'PASS download: ELF written, reset, all six read-only sections match.'
}

$remaining = @(Get-CimInstance Win32_Process -Filter "Name='JLinkGDBServerCL.exe' OR Name='arm-none-eabi-gdb.exe'" | Where-Object { $_.ExecutablePath -and $_.ExecutablePath.StartsWith($Tools,[StringComparison]::OrdinalIgnoreCase) })
if ($remaining.Count) { throw 'Test left debug child processes running' }
Write-Output "Hardware test artifacts: $runRoot"
