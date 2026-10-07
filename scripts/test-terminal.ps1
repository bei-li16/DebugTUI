param([string]$Binary = (Join-Path (Split-Path $PSScriptRoot) 'target/debug/debugtui.exe'))
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot
$Binary = (Resolve-Path -LiteralPath $Binary).Path
$out = Join-Path $root ('artifacts/terminal-' + (Get-Date -Format yyyyMMdd-HHmmss) + '-' + [guid]::NewGuid().ToString('N').Substring(0,8))
New-Item -ItemType Directory -Path $out -Force | Out-Null
Add-Type -Path (Join-Path $root 'tests/conpty.cs')
$cases = [System.Collections.Generic.List[object]]::new()
$terminal = $null
$esc = [char]27
function Wait-Screen([string]$Pattern) {
    $deadline = [datetime]::UtcNow.AddSeconds(8)
    do {
        $screen = $terminal.Screen()
        if ($screen -match $Pattern) { return $screen }
        Start-Sleep -Milliseconds 50
    } while ([datetime]::UtcNow -lt $deadline)
    $screen | Set-Content -LiteralPath (Join-Path $out 'last-failure.screen.txt') -Encoding utf8
    throw "Visible terminal did not match '$Pattern'; see $out"
}
function Test-Case([string]$Id, [string]$Description, [scriptblock]$Body) {
    try {
        & $Body | Out-Null
        $terminal.Screen() | Set-Content -LiteralPath (Join-Path $out "$Id.screen.txt") -Encoding utf8
        $cases.Add(@{id=$Id;description=$Description;status='passed'})
        Write-Output "PASS ${Id}: $Description"
    } catch {
        $cases.Add(@{id=$Id;description=$Description;status='failed';error=$_.Exception.Message})
        Write-Output "FAIL ${Id}: $($_.Exception.Message)"
    }
}
function Click-Text([string]$Text) {
    $lines = $terminal.Screen().Split("`n")
    for ($row=0; $row -lt $lines.Length; $row++) {
        $column = $lines[$row].IndexOf($Text, [StringComparison]::Ordinal)
        if ($column -ge 0) {
            $x=$column+1; $y=$row+1
            $terminal.Send("$esc[<0;$x;${y}M$esc[<0;$x;${y}m")
            return
        }
    }
    throw "Visible text '$Text' has no mouse target"
}
try {
    # Isolated demo configuration: no probe, GDB, user project or clipboard writes.
    "version=2`n[ui]`nanimations='off'`n" | Set-Content -LiteralPath (Join-Path $out 'debug.toml') -Encoding utf8
    $terminal = [DebugTuiTerminal]::new($Binary, '--demo', $out, 120, 36)
    Test-Case 'TERM-01' 'Native terminal enters the real demo workbench' { Wait-Screen 'DEMO.*STOPPED'; Wait-Screen 'Watch' }
    Test-Case 'TERM-02' 'Ctrl+K opens symbols and Enter navigates a demo source location' {
        $terminal.Send([string][char]11); Wait-Screen 'Find.*symbols'
        $terminal.Send('proc'); Wait-Screen 'process_items'; Wait-Screen 'Demo symbols'
        $terminal.Send("`r"); Wait-Screen 'process_items.*demo/sample.c'
    }
    Test-Case 'TERM-03' 'Tab activates Files; Ctrl+F edits and Ctrl+U clears the filter input' {
        # This host does not deliver injected SGR mouse records on all Windows
        # builds. Mouse hit areas are covered by the App interaction tests.
        $terminal.Send("`t`t"); Wait-Screen 'Find: files'
        $terminal.Send([string][char]6); $terminal.Send('terminal_filter_123'); Wait-Screen 'terminal_filter_123'
        $terminal.Send([string][char]21); Wait-Screen 'Ctrl\+F / click: file name or path'
        if ($terminal.Screen().Contains('terminal_filter_123')) { throw 'Filter text survived Ctrl+U' }
        $terminal.Send([string][char]27)
    }
    Test-Case 'TERM-04' 'F2 opens Setup, F3 Projects and F4 Examples; Esc returns to workbench' {
        $terminal.Send("${esc}OQ"); Wait-Screen 'Tools / profile'
        Wait-Screen 'Memory channels'
        if ($terminal.Screen() -match '\d+ more') { throw 'Roomy Setup unexpectedly hides fields' }
        $terminal.Send("${esc}OR"); Wait-Screen 'Projects / select'; $terminal.Send([string][char]27)
        Start-Sleep -Milliseconds 100
        $terminal.Send("${esc}OS"); Wait-Screen 'Examples / choose'; $terminal.Send([string][char]27)
        Start-Sleep -Milliseconds 100
        $terminal.Send([string][char]27); Wait-Screen 'DEMO.*STOPPED'
    }
    Test-Case 'TERM-05' 'Console command opens Appearance; Escape releases dialog focus' {
        $terminal.Send(':appearance'); $terminal.Send("`r"); Wait-Screen 'Appearance'
        $terminal.Send([string][char]27); Wait-Screen 'DEMO.*STOPPED'
    }
    Test-Case 'TERM-06' 'Resize renders a compact 80 by 24 workbench' {
        $terminal.Resize(80,24); Wait-Screen 'DebugTUI'; Wait-Screen 'Continue'
        if ($terminal.Screen().Split("`n").Length -ne 25) { throw 'Incorrect screen dimensions' }
        $terminal.Send("${esc}OQ"); Wait-Screen 'Tools / profile'; Wait-Screen '\d+ more'; Wait-Screen '┃'
        $terminal.Screen() | Set-Content -LiteralPath (Join-Path $out 'setup-compact-scrollbar.screen.txt') -Encoding utf8
        $terminal.Send([string][char]27); Wait-Screen 'DEMO.*STOPPED'
    }
    Test-Case 'TERM-07' 'Narrow 45 by 12 terminal remains interactive' {
        $terminal.Resize(45,12); Wait-Screen 'DebugTUI'
        $terminal.Send([string][char]11); Wait-Screen 'Find'; $terminal.Send([string][char]27)
    }
    Test-Case 'TERM-08' 'Ctrl+Q exits the native terminal with status zero' {
        $terminal.Send([string][char]17)
        if (-not $terminal.WaitForExit(10000)) { throw 'Terminal exit timed out' }
        if ($terminal.ExitCode() -ne 0) { throw "Terminal exit code $($terminal.ExitCode())" }
    }
} finally {
    if ($terminal) { $terminal.Transcript() | Set-Content -LiteralPath (Join-Path $out 'terminal.vt.txt') -Encoding utf8; $terminal.Dispose() }
    $passed = @($cases | Where-Object status -eq passed).Count
    $failed = @($cases | Where-Object status -eq failed).Count
    @{binary=$Binary;layer='Windows ConPTY demo';passed=($failed -eq 0 -and $cases.Count -eq 8);counts=@{passed=$passed;failed=$failed;skipped=0};cases=@($cases.ToArray());limitations=@('Demo validates terminal input/rendering, not target execution.','System clipboard is covered separately by an explicit opt-in native test.')} | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $out 'report.json') -Encoding utf8
    Write-Output "RESULT passed=$passed failed=$failed $out"
}
if ($failed -gt 0 -or $cases.Count -ne 8) { exit 1 }
