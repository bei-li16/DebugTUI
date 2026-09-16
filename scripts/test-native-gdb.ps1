param(
    [string]$Gdb = 'gdb',
    [string]$Compiler = 'gcc',
    [string]$Binary = (Join-Path (Split-Path $PSScriptRoot) 'target\release\debugtui.exe')
)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path $PSScriptRoot
$runRoot = Join-Path $projectRoot ('artifacts\native-gdb-' + (Get-Date -Format yyyyMMdd-HHmmss))
New-Item -ItemType Directory -Path $runRoot | Out-Null
@'
volatile int counter = 1;
int bump(int n) {
    return n + 1;
}
int main(void) {
    counter = bump(counter);
    counter = bump(counter);
    return 0;
}
'@ | Set-Content "$runRoot\sample.c" -Encoding ascii
& $Compiler -g -O0 "$runRoot\sample.c" -o "$runRoot\sample.exe"
if ($LASTEXITCODE -ne 0) { throw 'Native sample build failed' }
@(
    @{id=1;method='connect'},
    @{id=2;method='break';params=@{location='main'}},
    @{id=3;method='run'}, @{id=4;method='wait_stopped'},
    @{id=5;method='evaluate';params=@{expression='counter'}},
    @{id=6;method='step'}, @{id=7;method='wait_stopped'},
    @{id=8;method='status'}, @{id=9;method='disassemble'},
    @{id=10;method='delete_break';params=@{number=''}},
    @{id=11;method='continue'}
) | ForEach-Object { $_ | ConvertTo-Json -Depth 4 -Compress } | Set-Content "$runRoot\commands.jsonl" -Encoding utf8
& $Binary --gdb $Gdb --local --elf "$runRoot\sample.exe" --log-dir $runRoot --script "$runRoot\commands.jsonl" > "$runRoot\events.jsonl" 2> "$runRoot\errors.txt"
if ($LASTEXITCODE -ne 0) { throw "Native GDB test failed: $(Get-Content "$runRoot\errors.txt" -Raw)" }
$events = @(Get-Content "$runRoot\events.jsonl" | ForEach-Object { $_ | ConvertFrom-Json })
$responses = @($events | Where-Object event -eq response)
if (@($responses | Where-Object { -not $_.ok }).Count -ne 0) { throw 'Native command failed' }
if (($responses | Where-Object id -eq 1).result.state -ne 'READY') { throw 'Native connection must initially be READY' }
if (($responses | Where-Object id -eq 4).result.frame.function -ne 'main') { throw 'Native breakpoint did not reach main' }
if (($responses | Where-Object id -eq 5).result.value -ne '1') { throw 'Native variable evaluation failed' }
$regs = @(($responses | Where-Object id -eq 8).result.registers | ForEach-Object name)
if ($regs -notcontains 'rip' -and $regs -notcontains 'eip') { throw 'Native instruction-pointer register was filtered out' }
$commands = Get-ChildItem $runRoot -Filter 'session-*.log' | Get-Content | Where-Object { $_ -match '^\[mi>\]' }
if ($commands -match 'monitor|maintenance|target-select|target-download') { throw 'Native session sent environment-specific commands' }
Write-Output "PASS real native GDB: local launch, pre-run breakpoint, variable, step, dynamic x86 registers, disassembly, cleanup. Artifacts: $runRoot"
