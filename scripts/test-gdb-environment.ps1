param([string]$Binary = (Join-Path (Split-Path $PSScriptRoot) 'target\release\debugtui.exe'))
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path $PSScriptRoot
$runRoot = Join-Path $projectRoot ('artifacts\environment test 工程 ' + (Get-Date -Format yyyyMMdd-HHmmss))
New-Item -ItemType Directory -Path $runRoot | Out-Null
Copy-Item -LiteralPath $Binary -Destination "$runRoot\debugtui.exe"
$binary = Join-Path $runRoot 'debugtui.exe'
$node = (Get-Command node.exe).Source.Replace('\','/')
$fixture = (Join-Path $projectRoot 'tests\mock-gdb.cjs').Replace('\','/')
$groups = @{
    arm = @('r0','r1','sp','lr','pc','xpsr')
    riscv = @('zero','ra','sp','a0','pc','mstatus')
    x64 = @('rax','rbx','rip','rsp','eflags')
}
foreach ($arch in @('arm','riscv','x64')) {
    $transcript = "$runRoot\$arch.commands.txt".Replace('\','/')
    $registers = ConvertTo-Json -InputObject $groups[$arch] -Compress
    $config = @"
version = 2
[gdb]
executable = '$node'
args = ['$fixture']
[gdb.env]
DEBUGTUI_TEST_REGISTERS = '$registers'
DEBUGTUI_TEST_TRANSCRIPT = '$transcript'
[target]
mode = 'remote'
endpoint = 'localhost:1234'
[session]
on_exit = 'detach'
"@
    $config | Set-Content "$runRoot\$arch.toml" -Encoding utf8
    @('{"id":1,"method":"connect"}','{"id":2,"method":"status"}','{"id":3,"method":"step"}','{"id":4,"method":"wait_stopped"}','{"id":5,"method":"console","params":{"command":"run"}}','{"id":6,"method":"wait_stopped"}') | Set-Content "$runRoot\$arch.jsonl" -Encoding utf8
    & $binary --project "$runRoot\$arch.toml" --script "$runRoot\$arch.jsonl" > "$runRoot\$arch.events.jsonl" 2> "$runRoot\$arch.errors.txt"
    if ($LASTEXITCODE -ne 0) { throw "$arch fixture failed: $(Get-Content "$runRoot\$arch.errors.txt" -Raw)" }
    $events = @(Get-Content "$runRoot\$arch.events.jsonl" | ForEach-Object { $_ | ConvertFrom-Json })
    $responses = @($events | Where-Object event -eq response)
    if (@($responses | Where-Object { -not $_.ok }).Count -ne 0) { throw "$arch command failure" }
    $actual = @(($responses | Where-Object id -eq 2).result.registers | ForEach-Object name)
    if (@(Compare-Object $groups[$arch] $actual).Count -ne 0) { throw "$arch register list was filtered" }
    $commands = Get-Content -LiteralPath $transcript
    if ($commands -match 'monitor|maintenance|target-download|reset') { throw 'Implicit target-specific command was sent' }
    if ($commands -notcontains '-exec-run') { throw 'run did not retain GDB semantics' }
    Write-Output "PASS $arch MI fixture: standalone TUI, dynamic registers, step, run, detach, no target-specific commands."
}
# Reuse the strict x64 fixture to exercise local READY -> RUNNING -> STOPPED.
@('{"id":1,"method":"connect"}','{"id":2,"method":"continue"}','{"id":3,"method":"wait_stopped"}') | Set-Content "$runRoot\local.jsonl" -Encoding utf8
& $binary --project "$runRoot\x64.toml" --local --script "$runRoot\local.jsonl" > "$runRoot\local.events.jsonl" 2> "$runRoot\local.errors.txt"
if ($LASTEXITCODE -ne 0) { throw 'Local target fixture failed' }
$events = @(Get-Content "$runRoot\local.events.jsonl" | ForEach-Object { $_ | ConvertFrom-Json })
if (($events | Where-Object { $_.event -eq 'response' -and $_.id -eq 1 }).result.state -ne 'READY') { throw 'Local target incorrectly reported STOPPED' }
foreach ($action in @('restart','download')) {
    @('{"id":1,"method":"connect"}', ('{"id":2,"method":"' + $action + '"}')) | Set-Content "$runRoot\unsupported.jsonl" -Encoding utf8
    & $binary --project "$runRoot\x64.toml" --script "$runRoot\unsupported.jsonl" > "$runRoot\$action.events.jsonl" 2> "$runRoot\$action.errors.txt"
    if ($LASTEXITCODE -ne 1) { throw "Unconfigured $action was not rejected" }
    $events = @(Get-Content "$runRoot\$action.events.jsonl" | ForEach-Object { $_ | ConvertFrom-Json })
    if (($events | Where-Object { $_.event -eq 'response' -and $_.id -eq 2 }).error -notmatch 'not configured') { throw 'Unexpected unsupported-action error' }
}
Write-Output "PASS local launch and unsupported actions. Artifacts: $runRoot"

# Generic service readiness may be printed to stderr without a trailing newline.
$servicePidFile = "$runRoot/service.pid".Replace('\','/')
$serviceArgs = ConvertTo-Json -InputObject @('-e','require("node:fs").writeFileSync(process.argv[1], String(process.pid)); process.stderr.write("ENV_READY"); setInterval(()=>{},1000);',$servicePidFile) -Compress
$serviceConfig = (Get-Content "$runRoot\x64.toml" -Raw) + @"

[service]
command = '$node'
args = $serviceArgs
ready = ['ENV_READY']
timeout_ms = 3000
"@
$serviceConfig | Set-Content "$runRoot\service.toml" -Encoding utf8
'{"id":1,"method":"connect"}' | Set-Content "$runRoot\service.jsonl" -Encoding utf8
& $binary --project "$runRoot\service.toml" --script "$runRoot\service.jsonl" > "$runRoot\service.events.jsonl" 2> "$runRoot\service.errors.txt"
if ($LASTEXITCODE -ne 0) { throw "Generic service failed: $(Get-Content "$runRoot\service.errors.txt" -Raw)" }
$ownedServiceId = [int](Get-Content -LiteralPath $servicePidFile -Raw)
if (Get-Process -Id $ownedServiceId -ErrorAction SilentlyContinue) { throw 'Owned generic service was not cleaned up' }
Write-Output 'PASS generic service: configurable command, stderr readiness without newline, owned-process cleanup.'
