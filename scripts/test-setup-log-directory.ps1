param([string]$Binary = (Join-Path (Split-Path $PSScriptRoot) 'target/debug/debugtui.exe'))
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot
$Binary = (Resolve-Path -LiteralPath $Binary).Path
$out = Join-Path $root ('artifacts/setup-log-directory-' + [guid]::NewGuid().ToString('N'))
$projectRoot = Join-Path $out 'project'
New-Item -ItemType Directory -Path $projectRoot -Force | Out-Null
Add-Type -Path (Join-Path $root 'tests/conpty.cs')
$cases = [System.Collections.Generic.List[object]]::new()
$terminal = $null
$esc = [char]27
$oldConfigRoot = $env:DEBUGTUI_CONFIG_DIR
$env:DEBUGTUI_CONFIG_DIR = Join-Path $out 'user settings'
$project = Join-Path $projectRoot 'debug.toml'
function Wait-Screen([string]$Pattern) {
    $deadline = [datetime]::UtcNow.AddSeconds(10)
    do {
        $screen = $terminal.Screen()
        if ($screen -match $Pattern) { return $screen }
        Start-Sleep -Milliseconds 50
    } while ([datetime]::UtcNow -lt $deadline)
    $screen | Set-Content -LiteralPath (Join-Path $out 'last-failure.screen.txt') -Encoding utf8
    throw "Visible terminal did not match '$Pattern'"
}
function Save-Config {
    $terminal.Send([string][char]19)
    Wait-Screen 'Saved\s+(?:[A-Za-z]:|\\\\)' | Out-Null
}
function Stop-Terminal {
    $terminal.Send([string][char]17)
    if (-not $terminal.WaitForExit(10000) -or $terminal.ExitCode() -ne 0) { throw 'Terminal exit failed' }
    $terminal.Transcript() | Set-Content -LiteralPath (Join-Path $out ('terminal-' + $cases.Count + '.vt.txt')) -Encoding utf8
    $terminal.Dispose()
    $script:terminal = $null
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
        throw
    }
}
try {
    $terminal = [DebugTuiTerminal]::new($Binary, '--setup', $projectRoot, 140, 40)
    Test-Case 'LOGSETUP-01' 'First launch in an empty directory disables file logging' {
        Wait-Screen 'Log directory\s+\(not set\)'
        if ((Get-Content -LiteralPath $project -Raw) -match '(?m)^log_dir\s*=') { throw 'New template enables file logs' }
        if (Test-Path -LiteralPath (Join-Path $projectRoot 'debug-logs')) { throw 'Setup created a log directory' }
    }
    Test-Case 'LOGSETUP-02' 'Arrow keys enable debug-logs and disable it; save does not create folders' {
        # Debug cores is disabled until a chip is selected, so navigation skips it.
        $terminal.Send(("${esc}[B" * 10) + "${esc}[C")
        Wait-Screen 'Log directory\s+debug-logs'
        Save-Config
        if ((Get-Content -LiteralPath $project -Raw) -notmatch 'log_dir\s*=\s*"debug-logs"') { throw 'Enabled path was not saved' }
        if (Test-Path -LiteralPath (Join-Path $projectRoot 'debug-logs')) { throw 'Saving created a log directory' }
        $terminal.Send("${esc}[D")
        Wait-Screen 'Log directory\s+\(not set\)'
        Save-Config
        if ((Get-Content -LiteralPath $project -Raw) -notmatch 'log_dir\s*=\s*""') { throw 'Explicit disabled state was not saved' }
    }
    Test-Case 'LOGSETUP-03' 'Enter edits a custom path; cancel preserves it and clearing disables logs' {
        $terminal.Send("`rcustom logs/nested`r")
        Wait-Screen 'Log directory\s+custom logs/nested'
        Save-Config
        if (Test-Path -LiteralPath (Join-Path $projectRoot 'custom logs')) { throw 'Editing created a log directory' }
        $terminal.Send("`r" + [string][char]21 + 'discarded' + [string]$esc)
        Wait-Screen 'Log directory\s+custom logs/nested'
        $terminal.Send("`r" + [string][char]21 + "`r")
        Wait-Screen 'Log directory\s+\(not set\)'
    }
    Test-Case 'LOGSETUP-04' 'F2 browses only folders; Enter opens and Space selects a relative path' {
        New-Item -ItemType Directory -Path (Join-Path $projectRoot 'chosen folder') -Force | Out-Null
        'fixture' | Set-Content -LiteralPath (Join-Path $projectRoot 'not-a-folder.txt') -Encoding utf8
        $terminal.Send("${esc}OQ")
        Wait-Screen 'Folders /'; Wait-Screen 'Space: select current folder'
        if ($terminal.Screen() -match 'not-a-folder.txt|· file') { throw 'Folder picker offers files' }
        $terminal.Send("${esc}[B`r")
        # The long isolated artifact path can truncate the browser title.
        # The selected relative value below proves Enter opened the intended folder.
        $terminal.Send(' ')
        Wait-Screen 'Log directory\s+chosen folder'
        Save-Config
        if ((Get-Content -LiteralPath $project -Raw) -notmatch 'log_dir\s*=\s*"chosen folder"') { throw 'Folder path is not project-relative' }
    }
    Test-Case 'LOGSETUP-05' 'Saved custom path and disabled state survive restart' {
        Stop-Terminal
        $script:terminal = [DebugTuiTerminal]::new($Binary, '--setup', $projectRoot, 140, 40)
        Wait-Screen 'Log directory\s+chosen folder'
        $terminal.Send(("${esc}[B" * 10) + "${esc}[C")
        Wait-Screen 'Log directory\s+\(not set\)'
        Save-Config
        Stop-Terminal
        $script:terminal = [DebugTuiTerminal]::new($Binary, '--setup', $projectRoot, 100, 28)
        Wait-Screen 'Log directory\s+\(not set\)'
        if (Get-ChildItem -LiteralPath $projectRoot -Filter 'session-*.log' -Recurse) { throw 'Setup wrote session logs' }
    }
    Stop-Terminal
} catch {
    Write-Output $_.Exception.Message
} finally {
    if ($terminal) { $terminal.Dispose() }
    $env:DEBUGTUI_CONFIG_DIR = $oldConfigRoot
    $passed = @($cases | Where-Object status -eq passed).Count
    $failed = @($cases | Where-Object status -eq failed).Count
    @{binary=$Binary;sha256=(Get-FileHash -LiteralPath $Binary).Hash;layer='Windows ConPTY Setup';board_tests_executed=$false;passed=($failed -eq 0 -and $passed -eq 5);counts=@{passed=$passed;failed=$failed;skipped=0};cases=@($cases.ToArray())} | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $out 'report.json') -Encoding utf8
    Write-Output "RESULT passed=$passed failed=$failed $out"
}
if ($failed -gt 0 -or $passed -ne 5) { exit 1 }
