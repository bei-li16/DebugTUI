param([string]$Binary = (Join-Path (Split-Path $PSScriptRoot) 'target/debug/debugtui.exe'))
$ErrorActionPreference='Stop'
$repo=Split-Path $PSScriptRoot
$Binary=(Resolve-Path -LiteralPath $Binary).Path
$out=Join-Path $repo ('artifacts/devices-tui-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $out | Out-Null
Copy-Item -LiteralPath $Binary -Destination (Join-Path $out 'debugtui.exe')
$Binary=Join-Path $out 'debugtui.exe'
$oldConfig=$env:DEBUGTUI_CONFIG_DIR
$env:DEBUGTUI_CONFIG_DIR=Join-Path $out 'user'
Add-Type -Path (Join-Path $repo 'tests/conpty.cs')
$esc=[char]27
$terminal=$null
function Wait-Screen([string]$Pattern) {
    $deadline=[datetime]::UtcNow.AddSeconds(20)
    do {
        $screen=$terminal.Screen()
        if ($screen -match $Pattern) { return $screen }
        Start-Sleep -Milliseconds 50
    } while ([datetime]::UtcNow -lt $deadline)
    $screen | Set-Content -LiteralPath (Join-Path $out 'failure.screen.txt')
    throw "Screen did not match $Pattern; $out"
}
function Close-Terminal {
    $terminal.Send([string][char]17)
    if (-not $terminal.WaitForExit(10000)) { throw 'Exit timed out' }
    if ($terminal.ExitCode() -ne 0) { throw 'Debugger exit failed' }
    $terminal.Dispose()
    $script:terminal=$null
}
try {
    @'
version=3
[tools]
profile='debug-env.toml'
[debug]
chip='tha6206'
cores=[0]
[ui]
animations='off'
'@ | Set-Content -LiteralPath (Join-Path $out 'debug.toml') -Encoding utf8
    @'
[gdb]
executable='gdb'
[target]
mode='local'
endpoint='local.0'
[backends.tha6]
[backends.generic]
[core_targets."0"]
endpoint='local.0'
[core_targets."1"]
endpoint='local.1'
'@ | Set-Content -LiteralPath (Join-Path $out 'debug-env.toml') -Encoding utf8
    $terminal=[DebugTuiTerminal]::new($Binary,'--setup',$out,120,36)
    Wait-Screen 'Chip\s+tha6206' | Out-Null
    # Arrow navigation starts at Project, wraps across fields and excludes
    # the action buttons. ELF path prefix is skipped while remapping is off.
    Wait-Screen '\u203a Project' | Out-Null
    $terminal.Send("${esc}[A")
    Wait-Screen '\u203a Memory channels' | Out-Null
    foreach ($field in @('Register catalogue','CPU registers','Source remap')) {
        $terminal.Send("${esc}[A")
        Wait-Screen ('\u203a '+[regex]::Escape($field)) | Out-Null
    }
    # The disabled ELF prefix is skipped between Source remap and CPU registers.
    foreach ($field in @('CPU registers','Register catalogue','Memory channels','Project')) {
        $terminal.Send("${esc}[B")
        Wait-Screen ('\u203a '+[regex]::Escape($field)) | Out-Null
    }
    $terminal.Send(("`t"*3)+"`r") # Project -> Tools / profile -> Probe -> Chip
    Wait-Screen 'Choose chip' | Out-Null
    $terminal.Send("`r")
    Wait-Screen 'core\.1' | Out-Null
    $terminal.Send('n'+"${esc}[B"+' '+"`r") # core1 only
    Wait-Screen 'Debug cores\s+\[1\]' | Out-Null
    $terminal.Send([string][char]19)
    Wait-Screen 'Saved (?:[A-Za-z]:|\\\\)' | Out-Null
    $terminal.Screen() | Set-Content -LiteralPath (Join-Path $out 'core1.screen.txt')
    Close-Terminal
    $terminal=[DebugTuiTerminal]::new($Binary,'--setup',$out,120,36)
    Wait-Screen 'Debug cores\s+\[1\]' | Out-Null
    $terminal.Send(("`t"*3)+"`r`r"+'a'+"`r")
    Wait-Screen 'Debug cores\s+\[0, 1\]' | Out-Null
    $terminal.Send([string][char]19)
    Wait-Screen 'Saved (?:[A-Za-z]:|\\\\)' | Out-Null
    $terminal.Screen() | Set-Content -LiteralPath (Join-Path $out 'dual.screen.txt')
    $terminal.Send("${esc}[A`rn") # Core IDs -> Chip -> Add
    Wait-Screen 'Add chip' | Out-Null
    $terminal.Send('s32k144'+[string][char]19)
    Wait-Screen 'Choose chip' | Out-Null
    $terminal.Send("`r`r")
    Wait-Screen 'Chip\s+s32k144' | Out-Null
    $terminal.Send([string][char]19)
    Wait-Screen 'Saved (?:[A-Za-z]:|\\\\)' | Out-Null
    $terminal.Screen() | Set-Content -LiteralPath (Join-Path $out 's32k144.screen.txt')
    Close-Terminal
    $profile=Join-Path $env:DEBUGTUI_CONFIG_DIR 'profiles/devices.toml'
    $before=Get-Content -LiteralPath $profile -Raw
    if ($before -notmatch '\[devices.s32k144\]' -or $before -notmatch '\[devices.tha6412\]') { throw 'Customer/default chips missing' }
    & $Binary --init-profiles | Out-Null
    if ($LASTEXITCODE -ne 0 -or (Get-Content -LiteralPath $profile -Raw) -cne $before) { throw 'Upgrade changed user catalogue' }
    @{passed=$true;cases=@('Project focus and field-only arrow navigation','core1 selection and reload','core0+1','add customer chip in TUI','upgrade retains custom entries');profile=$profile} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $out 'report.json')
    Write-Output "PASS chip/core Setup and installation persistence: $out"
} finally {
    if ($terminal) { $terminal.Send([string][char]17); $null=$terminal.WaitForExit(3000); $terminal.Dispose() }
    $env:DEBUGTUI_CONFIG_DIR=$oldConfig
}
