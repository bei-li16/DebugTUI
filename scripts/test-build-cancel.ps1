$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path $PSScriptRoot
$runRoot = Join-Path $projectRoot ('artifacts\build-cancel-' + (Get-Date -Format yyyyMMdd-HHmmss))
New-Item -ItemType Directory -Path $runRoot -Force | Out-Null
@'
version = 1
[build]
command = 'powershell.exe'
args = ['-NoProfile', '-Command', 'Start-Sleep -Seconds 30']
'@ | Set-Content "$runRoot\project.toml" -Encoding utf8
$info = [System.Diagnostics.ProcessStartInfo]::new("$projectRoot\target\release\debugtui.exe")
$info.UseShellExecute = $false
$info.CreateNoWindow = $true
$info.RedirectStandardInput = $true
$info.RedirectStandardOutput = $true
$info.RedirectStandardError = $true
$info.Arguments = '--project "{0}\project.toml" --headless --stdio' -f $runRoot
$app = [System.Diagnostics.Process]::Start($info)
$errors = $app.StandardError.ReadToEndAsync()
$lines = [System.Collections.Generic.List[string]]::new()
try {
    $app.StandardInput.WriteLine('{"id":1,"method":"build"}')
    $app.StandardInput.Flush()
    do {
        $read = $app.StandardOutput.ReadLineAsync()
        if (-not $read.Wait(10000) -or $null -eq $read.Result) { throw 'Build did not start' }
        $lines.Add($read.Result)
        $event = $read.Result | ConvertFrom-Json
    } until ($event.event -eq 'snapshot' -and $event.snapshot.state -eq 'BUILDING')
    $childIds = @(Get-CimInstance Win32_Process -Filter "ParentProcessId=$($app.Id)" | ForEach-Object ProcessId)
    $clock = [Diagnostics.Stopwatch]::StartNew()
    $app.StandardInput.WriteLine('{"id":2,"method":"quit"}')
    $app.StandardInput.Flush()
    $rest = $app.StandardOutput.ReadToEndAsync()
    if (-not $app.WaitForExit(5000)) { throw 'Quit did not cancel the build promptly' }
    $clock.Stop()
    $lines.Add($rest.Result)
    $lines | Set-Content "$runRoot\events.jsonl" -Encoding utf8
    $errors.Result | Set-Content "$runRoot\errors.txt" -Encoding utf8
    $events = @(Get-Content "$runRoot\events.jsonl" | Where-Object { $_.Trim() } | ForEach-Object { $_ | ConvertFrom-Json })
    $build = $events | Where-Object { $_.event -eq 'response' -and $_.id -eq 1 }
    $quit = $events | Where-Object { $_.event -eq 'response' -and $_.id -eq 2 }
    if ($app.ExitCode -ne 0 -or $build.ok -or $build.error -notmatch 'cancelled' -or -not $quit.ok) { throw 'Cancellation responses were incorrect' }
    foreach ($childId in $childIds) {
        if (Get-Process -Id $childId -ErrorAction SilentlyContinue) { throw 'Cancelled build left an owned process behind' }
    }
    Write-Output "PASS build cancellation: quit handled in $($clock.ElapsedMilliseconds) ms; build child terminated. Artifacts: $runRoot"
} finally { if (-not $app.HasExited) { $app.Kill() } }
