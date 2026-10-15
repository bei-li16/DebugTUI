# Physical-board ConPTY regression. Supply an isolated project fixture with
# absolute ELF/source/tool paths; UI preferences are saved only in run copies.
# Attaches existing firmware, reads registers, then follows its on_exit policy.
# Does not build, flash, reset, or write system registers.
param(
    [Parameter(Mandatory)][string]$ProjectFixture,
    [string]$Binary = (Join-Path (Split-Path $PSScriptRoot) 'target/debug/debugtui.exe'),
    [string]$WorkingDirectory,
    [string]$ArtifactRoot = (Join-Path (Split-Path $PSScriptRoot) 'artifacts'),
    [ValidateRange(1,20)][int]$Rounds = 5
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot
$Binary = (Resolve-Path -LiteralPath $Binary).Path
$ProjectFixture = (Resolve-Path -LiteralPath $ProjectFixture).Path
$ArtifactRoot = [IO.Path]::GetFullPath($ArtifactRoot)
if (-not $WorkingDirectory) { $WorkingDirectory = Split-Path $ProjectFixture }
$WorkingDirectory = (Resolve-Path -LiteralPath $WorkingDirectory).Path
$fixtureHash = (Get-FileHash -LiteralPath $ProjectFixture).Hash
$out = Join-Path $ArtifactRoot ('register-search-hardware-' + (Get-Date -Format yyyyMMdd-HHmmss) + '-' + [guid]::NewGuid().ToString('N').Substring(0,8))
New-Item -ItemType Directory -Path $out -Force | Out-Null
Add-Type -Path (Join-Path $repo 'tests/conpty.cs')
$terminal = $null
$esc = [char]27
$cases = [Collections.Generic.List[object]]::new()
$exits = [Collections.Generic.List[object]]::new()
$failure = $null
$roundDirectory = $out
function Wait-Screen([string]$Pattern, [int]$Seconds = 10) {
    $deadline = [DateTime]::UtcNow.AddSeconds($Seconds)
    do {
        $screen = $terminal.Screen()
        if ($screen -match $Pattern) { return $screen }
        Start-Sleep -Milliseconds 50
    } while ([DateTime]::UtcNow -lt $deadline)
    $screen | Set-Content -LiteralPath (Join-Path $roundDirectory 'failure-screen.txt') -Encoding utf8
    throw "Timed out waiting for '$Pattern'"
}
function Search-Keys([string]$Name) {
    # No wait between Ctrl+U, the filter, Enter and Home. This used to queue
    # register_preferences + registers_read and strand the latter indefinitely.
    return 's' + [char]21 + $Name + "`r${esc}[H"
}
function Check-Value([string]$Name, [string]$Id, [Diagnostics.Stopwatch]$Timer) {
    $pattern = [regex]::Escape($Name) + '\s*=\s*(0x[0-9a-fA-F]+)\s+'
    $screen = Wait-Screen $pattern
    if ($screen -notmatch 'Shown 1\s+Valid 1') { $screen = Wait-Screen 'Shown 1\s+Valid 1' }
    if ($screen -notmatch $pattern) { throw "Value row disappeared for $Name" }
    $value = $Matches[1]
    if ($Name -eq 'SCB.CPUID' -and $value -ne '0x410fc241') { throw "Unexpected F429 CPUID: $value" }
    $screen | Set-Content -LiteralPath (Join-Path $roundDirectory "$Id.txt") -Encoding utf8
    $cases.Add([ordered]@{round=$round;id=$Id;register=$Name;value=$value;elapsed_ms=$Timer.ElapsedMilliseconds;status='passed'})
}
try {
    for ($round=1; $round -le $Rounds; $round++) {
        $roundDirectory = Join-Path $out "round-$round"
        New-Item -ItemType Directory -Path $roundDirectory -Force | Out-Null
        $project = Join-Path $roundDirectory 'debug.toml'
        Copy-Item -LiteralPath $ProjectFixture -Destination $project
        try {
            $logDir = Join-Path $roundDirectory 'debug-logs'
            $terminal = [DebugTuiTerminal]::new($Binary, "--project `"$project`" --log-dir `"$logDir`"", $WorkingDirectory, 120, 36)
            $null = Wait-Screen 'STOPPED' 30
            # Startup settling is identical in baseline and fixed runs. Filter
            # input below has no delay; each round uses a new sample cache.
            Start-Sleep -Milliseconds 1500
            $terminal.Send("${esc}5")
            Start-Sleep -Milliseconds 150
            $terminal.Send('z')
            Start-Sleep -Milliseconds 150
            # First observe an actual completed read: the idle marker alone can
            # still describe the frame before visible-register loading starts.
            # Probe is deliberately disabled while that initial read is pending.
            $terminal.Send((Search-Keys 'SCB.CPUID'))
            $null = Wait-Screen 'SCB.CPUID\s*=\s*0x410fc241'
            $null = Wait-Screen '● STOPPED' 15
            $terminal.Send(":register-probe`r")
            $probe = Wait-Screen 'Capability probe:.*facts' 15
            $probe | Set-Content -LiteralPath (Join-Path $roundDirectory 'probe.txt') -Encoding utf8
            $terminal.Send("${esc}5")
            Start-Sleep -Milliseconds 150
            $index = 0
            foreach ($name in @('SCB.CPUID','NVIC.ISER0','SysTick.LOAD','SCB.ICSR','NVIC.ISPR0','SysTick.VAL','SCB.CPACR','NVIC.IPR90','SysTick.CALIB')) {
                $timer = [Diagnostics.Stopwatch]::StartNew()
                $terminal.Send('s')
                $null = Wait-Screen 'Find:'
                $terminal.Send([string][char]21)
                $terminal.Send($name)
                $terminal.Send("`r")
                $terminal.Send("${esc}[H")
                Check-Value $name ('search-' + (++$index)) $timer
            }
            foreach ($burst in @(
                @('SCB.VTOR','NVIC.ISER1','SCB.CFSR'),
                @('SCB.HFSR','NVIC.ISER2','SysTick.LOAD'),
                @('SysTick.VAL','SCB.ICSR','NVIC.ICPR2')
            )) {
                $keys = ($burst | ForEach-Object { Search-Keys $_ }) -join ''
                $timer = [Diagnostics.Stopwatch]::StartNew()
                $terminal.Send($keys)
                Check-Value $burst[-1] ('burst-' + (++$index)) $timer
            }
            $terminal.Send([string][char]17)
            if (-not $terminal.WaitForExit(15000)) { throw 'Normal detach timed out' }
            if ($terminal.ExitCode() -ne 0) { throw "Debugger exit code $($terminal.ExitCode())" }
            $exits.Add(@{round=$round;exit_code=0})
            Write-Output "PASS round ${round}: nine rapid searches, three filter bursts, normal detach"
        } finally {
            if ($terminal) {
                if (-not $terminal.WaitForExit(0)) { $terminal.Send([string][char]17); $null=$terminal.WaitForExit(15000) }
                $terminal.Transcript() | Set-Content -LiteralPath (Join-Path $roundDirectory 'transcript.txt') -Encoding utf8
                $terminal.Dispose()
                $terminal = $null
            }
        }
    }
    if ((Get-FileHash -LiteralPath $ProjectFixture).Hash -ne $fixtureHash) { throw 'Input fixture changed' }
} catch {
    $failure = $_.ToString()
} finally {
    [ordered]@{
        passed=($null -eq $failure);board_tests_executed=$true;binary=$Binary
        driver_sha256=(Get-FileHash -LiteralPath $PSCommandPath).Hash.ToLowerInvariant()
        binary_sha256=(Get-FileHash -LiteralPath $Binary).Hash.ToLowerInvariant()
        project_fixture=$ProjectFixture;fixture_sha256=$fixtureHash;working_directory=$WorkingDirectory
        rounds_requested=$Rounds;rounds_completed=$exits.Count;filter_input_delay_ms=0
        cases=$cases;exits=$exits;error=$failure
        limits=@('Read verification only; no reset, flash or system-register writes','Repeated sessions provide fresh register caches; captures are real hardware evidence')
    } | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $out 'report.json') -Encoding utf8
    Write-Output "Report: $out/report.json"
}
if ($failure) { throw $failure }
