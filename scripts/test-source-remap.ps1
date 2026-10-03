param(
    [string]$Binary = (Join-Path (Split-Path $PSScriptRoot) 'target/debug/debugtui.exe'),
    [string]$Gdb = 'C:/Program Files/mingw64/bin/gdb.exe',
    [string]$Compiler = 'C:/Program Files/mingw64/bin/gcc.exe',
    [string]$Elf,
    [string]$SourceRoot
)
# ConPTY exercises the real Setup scanner. Optional ELF/root are metadata-only:
# no target connection, service, reset, download, or user configuration writes.
$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot
$Binary = (Resolve-Path -LiteralPath $Binary).Path
$out = Join-Path $repo ('artifacts/source-remap-' + (Get-Date -Format yyyyMMdd-HHmmss) + '-' + [guid]::NewGuid().ToString('N').Substring(0,8))
New-Item -ItemType Directory -Path $out | Out-Null
# Windows locks running executables. Test a private copy so a build cannot race it.
$inputBinary = $Binary
$Binary = Join-Path $out 'debugtui.exe'
Copy-Item -LiteralPath $inputBinary -Destination $Binary
Add-Type -Path (Join-Path $repo 'tests/conpty.cs')
$esc = [char]27
$terminal = $null
$results = [System.Collections.Generic.List[object]]::new()
$success = $false
function Wait-Screen([string]$Pattern) {
    $deadline = [datetime]::UtcNow.AddSeconds(35)
    do {
        $screen = $terminal.Screen()
        if ($screen -match $Pattern) { return $screen }
        Start-Sleep -Milliseconds 50
    } while ([datetime]::UtcNow -lt $deadline)
    $screen | Set-Content -LiteralPath (Join-Path $out 'failure.screen.txt') -Encoding utf8
    throw "Screen did not match '$Pattern'; see $out"
}
function Finish-Terminal {
    if ($terminal) {
        $terminal.Send([string][char]17)
        if (-not $terminal.WaitForExit(10000)) { throw 'Terminal exit timed out' }
        if ($terminal.ExitCode() -ne 0) { throw "Terminal exit code $($terminal.ExitCode())" }
        $terminal.Dispose()
        $script:terminal=$null
    }
}
function Exercise-Setup([string]$Id,[string]$Program,[string]$LocalRoot,[string]$ExpectedPrefix) {
    $directory = Join-Path $out $Id
    New-Item -ItemType Directory -Path $directory -Force | Out-Null
    $config = Join-Path $directory 'debug.toml'
    # These commands must never run during metadata inspection.
    $marker = (Join-Path $directory 'unexpected-init.txt').Replace('\','/')
    $text = @"
version=2
[gdb]
executable='$($Gdb.Replace('\','/'))'
args=['-ex','shell echo forbidden > $marker']
init=['shell echo forbidden > $marker']
[target]
mode='local'
[program]
elf='$($Program.Replace('\','/'))'
source_root='$($LocalRoot.Replace('\','/'))'
[session]
on_exit='disconnect'
[ui]
animations='off'
"@
    [IO.File]::WriteAllText($config, $text)
    $script:terminal = [DebugTuiTerminal]::new($Binary, '--setup', $directory, 120, 36)
    Wait-Screen 'Source remap.*No' | Out-Null
    Wait-Screen 'Ctrl\+Q: exit' | Out-Null
    $terminal.Screen() | Set-Content -LiteralPath (Join-Path $directory 'setup-disabled.screen.txt') -Encoding utf8
    $terminal.Send(("${esc}[A" * 2) + "`r") # Disabled prefix is skipped.
    Wait-Screen 'source files; columns: found / covered' | Out-Null
    Wait-Screen '\[found\]' | Out-Null
    if ($ExpectedPrefix) { Wait-Screen ([regex]::Escape($ExpectedPrefix) + ' ->') | Out-Null }
    $terminal.Screen() | Set-Content -LiteralPath (Join-Path $directory 'scan.screen.txt') -Encoding utf8
    # Parent/child navigation changes the preview without applying it.
    $terminal.Send("${esc}[D")
    Start-Sleep -Milliseconds 100
    $terminal.Send("${esc}[C")
    Start-Sleep -Milliseconds 100
    # Rescan selects the best match again.
    $terminal.Send('r')
    Start-Sleep -Milliseconds 150
    Wait-Screen 'source files; columns: found / covered' | Out-Null
    $terminal.Send("`r")
    Wait-Screen 'Selected .* -> Source root' | Out-Null
    $terminal.Screen() | Set-Content -LiteralPath (Join-Path $directory 'setup-enabled.screen.txt') -Encoding utf8
    $terminal.Send([string][char]19)
    Start-Sleep -Milliseconds 100
    Wait-Screen 'Saved ' | Out-Null
    $saved = Get-Content -LiteralPath $config -Raw
    if ($saved -notmatch '\[source_remap\]' -or $saved -notmatch 'enabled = true') { throw 'Mapping was not persisted' }
    if (Test-Path -LiteralPath $marker) { throw 'Offline scan executed profile initialization' }
    Finish-Terminal
    $script:terminal = [DebugTuiTerminal]::new($Binary, '--setup', $directory, 120, 36)
    Wait-Screen 'Source remap.*Yes' | Out-Null
    $terminal.Send(("${esc}[A" * 3) + "`r") # Enabled prefix participates in navigation.
    Wait-Screen 'Source remap.*No' | Out-Null
    $terminal.Send([string][char]19)
    Start-Sleep -Milliseconds 100
    Wait-Screen 'Saved ' | Out-Null
    $disabled = Get-Content -LiteralPath $config -Raw
    if ($disabled -notmatch 'enabled = false' -or $disabled -notmatch 'from = ') { throw 'Disable lost the prefix' }
    Copy-Item -LiteralPath $config -Destination (Join-Path $directory 'disabled.toml')
    $terminal.Send("`r")
    Wait-Screen 'source files; columns: found / covered' | Out-Null
    $terminal.Send([string][char]27)
    Wait-Screen 'Source remap.*Yes' | Out-Null
    $terminal.Send([string][char]19)
    Start-Sleep -Milliseconds 100
    Wait-Screen 'Saved ' | Out-Null
    Finish-Terminal
    # Sanitized copy for local GDB symbol/breakpoint verification, never hardware.
    $clean = (Get-Content -LiteralPath $config -Raw) -replace '(?ms)^\[gdb\]\r?\n.*?(?=^\[|\z)', "[gdb]`nexecutable='$($Gdb.Replace('\','/'))'`n`n"
    [IO.File]::WriteAllText((Join-Path $directory 'session.toml'), $clean)
    [IO.File]::WriteAllText((Join-Path $directory 'session-disabled.toml'), ($clean -replace 'enabled = true', 'enabled = false'))
    $results.Add(@{id=$Id;passed=$true;elf=$Program;sourceRoot=$LocalRoot;configuration=$config;scan=(Join-Path $directory 'scan.screen.txt')})
    Write-Host "PASS ${Id}: scan, preview, choose, save, reload, disable, enable; no profile initialization"
    return $directory
}
function Check-Scan-Failures([string]$Program) {
    $fake = Join-Path $out 'scan-fixture.exe'
    & $Compiler (Join-Path $repo 'tests/fixtures/source-scan-gdb.c') -o $fake
    if ($LASTEXITCODE -ne 0) { throw 'Scan fixture compilation failed' }
    foreach ($mode in @('cancel','timeout','empty','error','exit')) {
        $directory = Join-Path $out $mode
        New-Item -ItemType Directory -Path $directory | Out-Null
        $timeout = if ($mode -eq 'timeout') {1000} else {8000}
        $text = @"
version=2
[gdb]
executable='$($fake.Replace('\','/'))'
[gdb.env]
DEBUGTUI_SCAN_TEST_MODE='$mode'
[program]
elf='$($Program.Replace('\','/'))'
source_root='.'
[target]
mode='local'
[session]
timeout_ms=$timeout
[ui]
animations='off'
"@
        [IO.File]::WriteAllText((Join-Path $directory 'debug.toml'), $text)
        $script:terminal = [DebugTuiTerminal]::new($Binary, '--setup', $directory, 120, 36)
        Wait-Screen 'Ctrl\+Q: exit' | Out-Null
        $terminal.Send(("${esc}[A" * 2) + "`r")
        $deadline = [datetime]::UtcNow.AddSeconds(5)
        $pidFile = Join-Path $directory 'scan.pid'
        while (-not (Test-Path -LiteralPath $pidFile) -and [datetime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 30 }
        if (-not (Test-Path -LiteralPath $pidFile)) { throw "No worker process for $mode" }
        $workerId = [int](Get-Content -LiteralPath $pidFile -Raw)
        switch ($mode) {
            'cancel' { $terminal.Send([string][char]27); Wait-Screen 'Source remap.*Yes' | Out-Null }
            'timeout' { Wait-Screen 'ELF scan timed out' | Out-Null }
            'empty' { Wait-Screen 'ELF has no source file records' | Out-Null }
            'error' { Wait-Screen 'fixture invalid ELF' | Out-Null }
            'exit' { Finish-Terminal }
        }
        $deadline = [datetime]::UtcNow.AddSeconds(3)
        while ((Get-Process -Id $workerId -ErrorAction SilentlyContinue) -and [datetime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 30 }
        if (Get-Process -Id $workerId -ErrorAction SilentlyContinue) { throw "Scan GDB $workerId leaked after $mode" }
        if ($terminal) { $terminal.Screen() | Set-Content -LiteralPath (Join-Path $directory 'screen.txt'); Finish-Terminal }
        $results.Add(@{id=$mode;passed=$true;workerExited=$workerId})
        Write-Host "PASS ${mode}: responsive Setup and scan process cleaned up"
    }
}
try {
    if ($Elf) {
        if (-not $SourceRoot) { throw '-Elf requires -SourceRoot' }
        $null = Exercise-Setup 'project-elf' (Resolve-Path -LiteralPath $Elf).Path (Resolve-Path -LiteralPath $SourceRoot).Path ''
    } else {
        $server = Join-Path $out 'server'
        $local = Join-Path $out 'local source'
        New-Item -ItemType Directory -Path (Join-Path $server 'src'),(Join-Path $local 'src') | Out-Null
        $source = "volatile int remap_counter = 7;`nint remap_function(void) { return remap_counter + 1; }`nint main(void) { return remap_function(); }`n"
        [IO.File]::WriteAllText((Join-Path $server 'src/main.c'), $source)
        [IO.File]::WriteAllText((Join-Path $local 'src/main.c'), $source)
        $styles = @{
            posix='/ci/build/project'; windows='Z:\agent\project'; mixed='Z:\agent/project';
            spaces='/ci/build space/project'
        }
        foreach ($entry in $styles.GetEnumerator() | Sort-Object Key) {
            $program = Join-Path $out ($entry.Key + '.exe')
            & $Compiler -g -O0 "-fdebug-prefix-map=$($server.Replace('\','/'))=$($entry.Value)" (Join-Path $server 'src/main.c').Replace('\','/') -o $program
            if ($LASTEXITCODE -ne 0) { throw 'Fixture compilation failed' }
            $directory = Exercise-Setup $entry.Key $program $local $entry.Value.Replace('\','/')
            & node (Join-Path $PSScriptRoot 'test-source-remap-gdb.cjs') $Binary $directory (Join-Path $local 'src/main.c')
            if ($LASTEXITCODE -ne 0) { throw "GDB remapping failed: $($entry.Key)" }
        }
        Check-Scan-Failures $program
    }
    $success = $true
} finally {
    if ($terminal) {
        $terminal.Screen() | Set-Content -LiteralPath (Join-Path $out 'last.screen.txt') -Encoding utf8
        $terminal.Transcript() | Set-Content -LiteralPath (Join-Path $out 'terminal.vt.txt') -Encoding utf8
        $terminal.Dispose()
    }
    @{passed=$success;cases=@($results.ToArray());binary=$Binary;inputBinary=$inputBinary;gdb=$Gdb;limits=@('ELF metadata and local symbols only; no target execution or hardware download.','UNC syntax is covered by unit tests; GDB resolution of unavailable network hosts can time out.')} | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $out 'report.json') -Encoding utf8
    Write-Output "RESULT $out"
}
