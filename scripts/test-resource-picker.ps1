param([string]$Binary = (Join-Path (Split-Path $PSScriptRoot) 'target/release/debugtui.exe'))
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot
$Binary = (Resolve-Path -LiteralPath $Binary).Path
$out = Join-Path $root ('artifacts/resource-picker-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path (Join-Path $out '.vscode') -Force | Out-Null
Add-Type -Path (Join-Path $root 'tests/conpty.cs')
$cases = [System.Collections.Generic.List[object]]::new()
$terminal = $null
$esc = [char]27
$oldConfigRoot = $env:DEBUGTUI_CONFIG_DIR
$env:DEBUGTUI_CONFIG_DIR = Join-Path $out 'user settings'
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
    $profile = "backend='stm32f4'`n[target]`nmode='extended-remote'`nendpoint='localhost:3333'`n"
    $profile | Set-Content -LiteralPath (Join-Path $out '.vscode/debug-env-cmsis-dap.toml') -Encoding utf8
    Copy-Item -LiteralPath (Join-Path $root 'tools/svd/STM32F429.svd') -Destination (Join-Path $out '.vscode/STM32F429.svd')
    $project = Join-Path $out 'debug.toml'
    @'
version=3
watch=['counter']
[tools]
profile='.vscode/debug-env-cmsis-dap.toml'
[debug]
chip='stm32f429'
cores=[0]
[program]
elf='firmware.elf'
svd='.vscode/STM32F429.svd'
[tasks]
build='build.cmd'
[session]
on_exit='detach'
'@ | Set-Content -LiteralPath $project -Encoding utf8
    $before = (Get-FileHash -LiteralPath $project).Hash
    $terminal = [DebugTuiTerminal]::new($Binary, '--setup', $out, 160, 45)
    Test-Case 'RESOURCE-01' 'Real Setup loads the old project without hardware access' {
        Wait-Screen 'debug-env-cmsis-dap.toml'; Wait-Screen 'Inherit profile'
    }
    Test-Case 'RESOURCE-02' 'Legacy Probe opens installed profile choices and Escape preserves the project' {
        $terminal.Send("${esc}[B${esc}[B`r")
        Wait-Screen 'Tools / installed and external profiles'; Wait-Screen 'Installed file:'
        $terminal.Screen() | Set-Content -LiteralPath (Join-Path $out 'installed-profile-picker.screen.txt') -Encoding utf8
        $terminal.Send([string]$esc); Wait-Screen 'Inherit profile'
        if ((Get-FileHash -LiteralPath $project).Hash -ne $before) { throw 'Cancel changed project' }
    }
    Test-Case 'RESOURCE-03' 'Accepting bundled tools applies the requested probe and migrates only the stock SVD' {
        $terminal.Send("`r"); Wait-Screen 'Bundled ARM / OpenOCD'
        $terminal.Send("`r"); Wait-Screen 'Probe\s+cmsis-dap'; Wait-Screen 'builtin:svd/STM32F429.svd'
        if ((Get-FileHash -LiteralPath $project).Hash -ne $before) { throw 'Selection saved before Ctrl+S' }
    }
    Test-Case 'RESOURCE-04' 'Profile browser opens the resolved installed tools directory' {
        $terminal.Send("${esc}OQ"); Wait-Screen 'Tools / installed and external profiles'
        $terminal.Send("${esc}OQ"); Wait-Screen 'Files / .*tools'; Wait-Screen 'debug-env.toml'
        $terminal.Send([string]$esc); Wait-Screen 'Tools / profile'
    }
    Test-Case 'RESOURCE-05' 'SVD picker offers installed files, automatic inheritance, disabled and external browsing' {
        $terminal.Send(("${esc}[B" * 11) + "${esc}OQ")
        Wait-Screen 'SVD / installed and external files'; Wait-Screen 'Bundled SVD: STM32F429.svd'; Wait-Screen 'Disabled / no SVD'
        $terminal.Send("${esc}[B"); Wait-Screen 'Installed file:'
        $terminal.Screen() | Set-Content -LiteralPath (Join-Path $out 'installed-svd-picker.screen.txt') -Encoding utf8
        $terminal.Send("`r"); Wait-Screen 'builtin:svd/STM32F429.svd'
    }
    Test-Case 'RESOURCE-06' 'Save retains project fields and portable resource aliases; Probe cycles without errors' {
        $terminal.Send(("${esc}[A" * 10) + "${esc}[C")
        Wait-Screen 'Probe\s+jlink'
        if ($terminal.Screen().Contains('Error:')) { throw 'Probe still reports an error' }
        # Do not match the static help text "Saved as [tools] probe" before saving.
        $terminal.Send([string][char]19); Wait-Screen 'Saved (?:[A-Za-z]:|\\\\)'
        $saved = Get-Content -LiteralPath $project -Raw
        foreach ($expected in @('builtin:arm-openocd','builtin:svd/STM32F429.svd','jlink','counter','firmware.elf','build.cmd')) {
            if (-not $saved.Contains($expected)) { throw "Lost field: $expected" }
        }
        if ($saved.Contains('.vscode')) { throw 'Stock resource references were not migrated' }
    }
    Test-Case 'RESOURCE-07' 'Ctrl+Q exits without starting a debugger or server' {
        $terminal.Send([string][char]17)
        if (-not $terminal.WaitForExit(10000) -or $terminal.ExitCode() -ne 0) { throw 'Terminal exit failed' }
    }
} catch {
    Write-Output $_.Exception.Message
} finally {
    if ($terminal) { $terminal.Transcript() | Set-Content -LiteralPath (Join-Path $out 'terminal.vt.txt') -Encoding utf8; $terminal.Dispose() }
    $env:DEBUGTUI_CONFIG_DIR = $oldConfigRoot
    $passed = @($cases | Where-Object status -eq passed).Count
    $failed = @($cases | Where-Object status -eq failed).Count
    @{binary=$Binary;sha256=(Get-FileHash -LiteralPath $Binary).Hash;layer='Windows ConPTY Setup';board_tests_executed=$false;passed=($failed -eq 0 -and $passed -eq 7);counts=@{passed=$passed;failed=$failed;skipped=0};cases=@($cases.ToArray())} | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $out 'report.json') -Encoding utf8
    Write-Output "RESULT passed=$passed failed=$failed $out"
}
if ($failed -gt 0 -or $passed -ne 7) { exit 1 }
