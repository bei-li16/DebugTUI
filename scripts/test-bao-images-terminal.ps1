param(
    [switch]$Run,
    [Parameter(Mandatory)][string]$Binary,
    [Parameter(Mandatory)][string]$Project
)
$ErrorActionPreference='Stop'
if (-not $Run) { throw 'Physical THA6206 test requires -Run' }
$root=Split-Path $PSScriptRoot
$Binary=(Resolve-Path -LiteralPath $Binary).Path
$Project=(Resolve-Path -LiteralPath $Project).Path
$bao=(Split-Path $Project).Replace('\','/')
$out=Join-Path $root ('artifacts/bao-images-terminal-'+(Get-Date -Format yyyyMMdd-HHmmss)+'-'+[guid]::NewGuid().ToString('N').Substring(0,8))
New-Item -ItemType Directory -Path $out -Force | Out-Null
$projectText=[IO.File]::ReadAllText($Project)
$projectText=$projectText.Replace('elf = "','elf = "'+$bao+'/').Replace('source_root = "."','source_root = "'+$bao+'"')
# This fixture owns its project preference writes; the user's config is unchanged.
$local=Join-Path $out 'debug.toml'
[IO.File]::WriteAllText($local,$projectText)
Add-Type -Path (Join-Path $root 'tests/conpty.cs')
$terminal=$null;$esc=[char]27;$cases=[Collections.Generic.List[object]]::new()
function Wait-Screen([string]$Pattern,[int]$Seconds=20) {
    $deadline=[datetime]::UtcNow.AddSeconds($Seconds)
    do {
        $screen=$terminal.Screen()
        if ($screen -match $Pattern) {return $screen}
        Start-Sleep -Milliseconds 50
    } while ([datetime]::UtcNow -lt $deadline)
    $screen | Set-Content -LiteralPath (Join-Path $out 'failure.screen.txt') -Encoding utf8
    throw "Terminal did not display $Pattern"
}
function Send-Command([string]$Text) {
    $terminal.Send(':'+$Text);$terminal.Send("`r")
    Start-Sleep -Milliseconds 150
}
function Test-Case([string]$Id,[string]$Description,[scriptblock]$Body) {
    try {
        & $Body | Out-Null
        $terminal.Screen() | Set-Content -LiteralPath (Join-Path $out ($Id+'.screen.txt')) -Encoding utf8
        $cases.Add(@{id=$Id;description=$Description;status='passed'})
        Write-Output "PASS ${Id}: $Description"
    } catch {
        $cases.Add(@{id=$Id;description=$Description;status='failed';error=$_.Exception.Message})
        throw
    }
}
try {
    $argsText='--project "'+$local+'" --log-dir "'+(Join-Path $out 'logs')+'"'
    $terminal=[DebugTuiTerminal]::new($Binary,$argsText,$out,160,45)
    Test-Case 'BAO-TERM-CONNECT' 'Native TUI connects both physical cores' {
        Wait-Screen 'DEBUG WORKSPACE' 45;Wait-Screen 'STOPPED' 45;Wait-Screen 'core.1'
    }
    Test-Case 'BAO-TERM-HVC' 'Keyboard controls run both cores to a genuine VM0 HVC stop in Bao' {
        Send-Command 'scope all';Send-Command 'core 1';Send-Command 'break init'
        Send-Command 'core 0';Send-Command 'run';Wait-Screen 'init.*breakpoint-hit'
        Send-Command 'break hvc_handler';Send-Command 'continue';Wait-Screen 'hvc_handler.*breakpoint-hit'
    }
    Test-Case 'BAO-TERM-GUEST' 'Saved Guest inspector shows native source, three frames and true Guest local registers' {
        Send-Command 'guest vm0';Wait-Screen 'Saved Guest vm0';Wait-Screen '#0 notify';Wait-Screen '#1 guest_main';Wait-Screen '#2 guest_reset';Wait-Screen 'r0 = 2248146945'
        $terminal.Send("${esc}[B");Wait-Screen 'last_send_tick';Wait-Screen '>#1 guest_main'
    }
    Test-Case 'BAO-TERM-RESIZE' 'Saved Guest view stays clipped and navigable at minimum terminal size' {
        $terminal.Resize(45,12);Wait-Screen 'Saved Guest';$terminal.Send("${esc}[5~");Start-Sleep -Milliseconds 150
        $terminal.Resize(160,45);Wait-Screen '#2 guest_reset'
    }
    Test-Case 'BAO-TERM-PHYSICAL' 'Escape restores the same physical Bao stop, without changing GDB frame or control state' {
        $terminal.Send([string][char]27);Wait-Screen 'hvc_handler.*breakpoint-hit';Wait-Screen 'STOPPED'
        if ($terminal.Screen().Contains('Saved Guest vm0')) {throw 'Guest modal did not close'}
    }
    Test-Case 'BAO-TERM-RETURN' 'Saved Guest r explicitly returns from HVC on the selected physical core' {
        Send-Command 'guest vm0';Wait-Screen 'Saved Guest vm0';Wait-Screen '#0 notify'
        $terminal.Send('r');Wait-Screen 'notify.*breakpoint-hit';Wait-Screen 'STOPPED'
        if ($terminal.Screen().Contains('Saved Guest vm0')) {throw 'Guest return did not leave the saved inspector'}
    }
    Test-Case 'BAO-TERM-IMAGE-WATCH' 'Native Watch displays the VM0 image owner and its real global value' {
        Send-Command 'watch @vm0:debug_domain_id';Wait-Screen '\[vm0\]';Wait-Screen '1828716544'
        Wait-Screen 'notify.*breakpoint-hit';Wait-Screen 'STOPPED'
    }
    Test-Case 'BAO-TERM-BROWSE' 'Opening Bao and VM0 source keeps the physical VM0 frame and separately labels the browsed image' {
        Send-Command 'open @bao:src/arch/armv8/aarch32/vm.c'
        Wait-Screen 'Exec: vm0.*Browse: bao.*Frame: vm0';Wait-Screen '\[bao\].*vm.c'
        Wait-Screen 'notify.*breakpoint-hit';Wait-Screen 'STOPPED'
        Send-Command 'open @vm0:configs/tha6206-dual/guest.c'
        Wait-Screen 'Exec: vm0.*Browse: vm0.*Frame: vm0';Wait-Screen '\[vm0\].*guest.c'
        Wait-Screen 'notify.*breakpoint-hit';Wait-Screen 'STOPPED'
    }
    Test-Case 'BAO-TERM-EXIT' 'Ctrl+Q disconnects the owned native terminal cleanly' {
        $terminal.Send([string][char]17)
        if (-not $terminal.WaitForExit(15000)) {throw 'TUI exit timed out'}
        if ($terminal.ExitCode() -ne 0) {throw "TUI exit code $($terminal.ExitCode())"}
    }
} finally {
    if ($terminal) {
        if (-not $terminal.WaitForExit(0)) {$terminal.Send([string][char]17);[void]$terminal.WaitForExit(15000)}
        $terminal.Transcript() | Set-Content -LiteralPath (Join-Path $out 'terminal.vt.txt') -Encoding utf8
        $terminal.Dispose()
    }
    $passed=@($cases | Where-Object status -eq passed).Count
    $failed=@($cases | Where-Object status -eq failed).Count
    @{board_tests_executed=$true;binary=$Binary;binary_sha256=(Get-FileHash -LiteralPath $Binary -Algorithm SHA256).Hash;project=$Project;layer='Native Windows ConPTY / real Bao+VM target';passed=($failed -eq 0 -and $passed -eq 9);counts=@{passed=$passed;failed=$failed};cases=@($cases.ToArray())} | ConvertTo-Json -Depth 7 | Set-Content -LiteralPath (Join-Path $out 'report.json') -Encoding utf8
    Write-Output "RESULT passed=$passed failed=$failed $out"
}
if ($failed -gt 0 -or $passed -ne 9) {exit 1}
