# Keeps the latest build and the newest run of every test kind; removes the rest of what can be regenerated.
# Dry run unless -Apply is given.
#   -Only target|artifacts|dev  limit the cleanup to one area
#   -Builds         also remove cargo outputs the current builds no longer use. This first runs cargo build,
#                   test --no-run and build --release (default features) for the profiles already in target\,
#                   which compiles when the sources are newer than the build.
#   -DropDebug      remove target\debug entirely; scripts/build.ps1 recreates it
#   -RecentMinutes  leave test runs written to within this window alone; another session may still be using them
#   -Keep a,b       artifacts entries (wildcards allowed) to leave completely alone, e.g. a finished audit whose
#                   manifest lists every file it contains
[CmdletBinding()]
param(
    [switch]$Apply,
    [switch]$Builds,
    [switch]$DropDebug,
    [ValidateSet('target', 'artifacts', 'dev')][string]$Only,
    [string[]]$Keep,
    [int]$RecentMinutes = 10
)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path $PSScriptRoot
$areas = if ($Only) { @($Only) } else { @('target', 'artifacts', 'dev') }
# powershell -File passes -Keep a,b as one string, so split it here.
$Keep = @($Keep | ForEach-Object { $_ -split ',' } | Where-Object { $_ })
$plan = [System.Collections.Generic.List[object]]::new()

function Format-Size([long]$Bytes) {
    if ($Bytes -ge 1GB) { return '{0:N1} GB' -f ($Bytes / 1GB) }
    return '{0:N0} MB' -f ($Bytes / 1MB)
}
function Get-Usage([string]$Path) {
    $bytes = 0L
    $newest = [datetime]::MinValue
    try {
        # Extended-length paths keep the deep fixture trees under artifacts\ below MAX_PATH.
        foreach ($file in [IO.DirectoryInfo]::new("\\?\$Path").EnumerateFiles('*', [IO.SearchOption]::AllDirectories)) {
            $bytes += $file.Length
            if ($file.LastWriteTime -gt $newest) { $newest = $file.LastWriteTime }
        }
    } catch { $newest = Get-Date }  # unreadable trees count as in use
    if ($newest -eq [datetime]::MinValue) { $newest = (Get-Item -LiteralPath $Path -Force).LastWriteTime }
    return [pscustomobject]@{ Bytes = $bytes; Newest = $newest }
}
function Test-Recent([datetime]$Time) { return ((Get-Date) - $Time).TotalMinutes -lt $RecentMinutes }
function Add-Plan([string]$Area, [string]$Reason, [string]$Path, [long]$Bytes) {
    $plan.Add([pscustomobject]@{ Area = $Area; Reason = $Reason; Path = $Path; Bytes = $Bytes })
}
# Run directories are named <kind>-<timestamp, pid or guid>; collapsing the numbers and hex ids leaves the kind.
function Get-RunKind([string]$Name) {
    $kind = $Name -creplace '[0-9a-f]{8,}', '#' -creplace '\d+', '#'
    return ($kind -creplace '(-#)+', '-#' -creplace '( #)+', ' #').Trim()
}
# <crate>-<16 hex digits> names every file of one cargo unit in deps\ (lib<crate>-<hash>.rlib, <crate>-<hash>.d, ...).
function Get-UnitKey([string]$Name) {
    if ($Name -match '^(?:lib)?(.+?-[0-9a-f]{16})(\..*)?$') { return $Matches[1] }
    return $Name
}
function Remove-Tree([string]$Path) {
    $full = [IO.Path]::GetFullPath($Path)
    $allowed = @('target', 'artifacts', '.dev') | ForEach-Object { Join-Path $projectRoot $_ }
    if (-not ($allowed | Where-Object { $full.StartsWith("$_\", [StringComparison]::OrdinalIgnoreCase) })) { throw "Refusing to remove outside target, artifacts and .dev: $full" }
    $extended = "\\?\$full"
    if ([IO.Directory]::Exists($extended)) {
        # rd clears read-only files and unlinks junctions instead of following them, which Remove-Item in Windows PowerShell does not.
        & cmd.exe /d /c rd /s /q $extended
    } elseif ([IO.File]::Exists($extended)) {
        [IO.File]::SetAttributes($extended, [IO.FileAttributes]::Normal)
        [IO.File]::Delete($extended)
    }
    if ([IO.Directory]::Exists($extended) -or [IO.File]::Exists($extended)) { throw "Could not remove $full" }
}

Push-Location $projectRoot
try {
    if ($areas -contains 'artifacts' -and (Test-Path -LiteralPath 'artifacts')) {
        $runs = foreach ($item in Get-ChildItem -LiteralPath 'artifacts' -Force) {
            if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { continue }
            if ($Keep | Where-Object { $item.Name -like $_ }) { continue }
            if ($item.PSIsContainer) { $usage = Get-Usage $item.FullName } else { $usage = [pscustomobject]@{ Bytes = $item.Length; Newest = $item.LastWriteTime } }
            [pscustomobject]@{ Item = $item; Kind = (Get-RunKind $item.Name) + $(if ($item.PSIsContainer) { '' } else { '|file' }); Bytes = $usage.Bytes; Newest = $usage.Newest }
        }
        $skipped = 0
        foreach ($group in $runs | Group-Object Kind) {
            $ordered = @($group.Group | Sort-Object Newest)
            $latest = $ordered[-1]
            foreach ($old in $ordered | Select-Object -First ($ordered.Count - 1)) {
                if (Test-Recent $old.Newest) { $skipped++; continue }
                Add-Plan 'artifacts' 'older test runs' $old.Item.FullName $old.Bytes
            }
            # The tested executable and its packages live in target\ and the newest distribution run; copies of debug builds are not worth keeping.
            if ($latest.Item.PSIsContainer -and -not (Test-Recent $latest.Newest)) {
                foreach ($rule in @(@('debugtui*.exe', 100MB), @('*.tgz', 20MB), @('*.zip', 20MB))) {
                    try {
                        foreach ($file in [IO.DirectoryInfo]::new("\\?\$($latest.Item.FullName)").EnumerateFiles($rule[0], [IO.SearchOption]::AllDirectories)) {
                            if ($file.Length -ge $rule[1]) { Add-Plan 'artifacts' 'build copies in kept runs' $file.FullName.Substring(4) $file.Length }
                        }
                    } catch { Write-Warning "Could not scan $($latest.Item.FullName): $($_.Exception.Message)" }
                }
            }
        }
        if ($skipped) { Write-Output "Skipped $skipped older runs written to in the last $RecentMinutes minutes." }
    }

    if ($areas -contains 'target') {
        foreach ($flavor in 'debug', 'release') {
            $dir = "$projectRoot\target\$flavor"
            if (-not (Test-Path -LiteralPath $dir)) { continue }
            if ($flavor -eq 'debug' -and $DropDebug) { Add-Plan 'target' 'debug profile (-DropDebug)' $dir (Get-Usage $dir).Bytes; continue }
            if (Test-Path -LiteralPath "$dir\incremental") { Add-Plan 'target' 'incremental caches' "$dir\incremental" (Get-Usage "$dir\incremental").Bytes }
        }
    }

    if ($areas -contains 'target' -and $Builds) {
        if (Test-Path "$projectRoot\.dev\cargo\bin\cargo.exe") {
            $env:CARGO_HOME = "$projectRoot\.dev\cargo"
            $env:RUSTUP_HOME = "$projectRoot\.dev\rustup"
            $env:PATH = "$env:CARGO_HOME\bin;$env:PATH"
        }
        if ($env:DEBUGTUI_GCC_DIR) { $env:PATH = "$env:DEBUGTUI_GCC_DIR;$env:PATH" }
        if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) { throw 'cargo not found; install .dev\cargo or add cargo to PATH' }
        $commands = [System.Collections.Generic.List[object]]::new()
        if ((Test-Path "$projectRoot\target\debug\deps") -and -not $DropDebug) {
            $commands.Add(@('build'))
            # Only refresh test binaries that were built before; test --no-run would otherwise create them.
            $testNames = if (Test-Path -LiteralPath 'tests') { @(Get-ChildItem -LiteralPath 'tests' -Filter '*.rs' -File | ForEach-Object BaseName) } else { @() }
            $testsBuilt = Get-ChildItem -LiteralPath 'target\debug\deps' -Filter '*.exe' | Where-Object { $testNames -contains ($_.BaseName -replace '-[0-9a-f]{16}$') }
            if ($testsBuilt) { $commands.Add(@('test', '--no-run')) }
        }
        if (Test-Path "$projectRoot\target\release\deps") { $commands.Add(@('build', '--release')) }

        $keepFiles = [System.Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
        $keepDirs = [System.Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
        foreach ($command in $commands) {
            $output = & cargo @command --locked --offline --message-format=json
            if ($LASTEXITCODE -ne 0) { throw "cargo $($command -join ' ') failed; fix the build first" }
            foreach ($line in $output) {
                if ($line -notmatch '^\{.*"reason":"(compiler-artifact|build-script-executed)"') { continue }
                $message = $line | ConvertFrom-Json
                if ($message.reason -eq 'build-script-executed') { [void]$keepDirs.Add((Split-Path $message.out_dir)); continue }
                foreach ($file in $message.filenames) {
                    [void]$keepFiles.Add($file)
                    $parent = Split-Path $file
                    if ((Split-Path (Split-Path $parent) -Leaf) -eq 'build') { [void]$keepDirs.Add($parent) }
                }
            }
        }

        foreach ($flavor in 'debug', 'release') {
            $dir = "$projectRoot\target\$flavor"
            if (-not (Test-Path -LiteralPath "$dir\deps") -or ($flavor -eq 'debug' -and $DropDebug)) { continue }
            $deps = @(Get-ChildItem -LiteralPath "$dir\deps" -File)
            $keepUnits = [System.Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
            foreach ($file in $keepFiles) { [void]$keepUnits.Add((Get-UnitKey (Split-Path $file -Leaf))) }
            # cargo hard-links deps\<bin>-<hash>.exe to target\<profile>\<bin>.exe and reports only the latter.
            $uplifted = @(Get-ChildItem -LiteralPath $dir -File -Filter '*.exe')
            foreach ($file in $deps | Where-Object Extension -eq '.exe') {
                if ($uplifted | Where-Object { $_.Length -eq $file.Length -and $_.LastWriteTimeUtc -eq $file.LastWriteTimeUtc }) { [void]$keepUnits.Add((Get-UnitKey $file.Name)) }
            }
            $stale = @($deps | Where-Object { -not $keepUnits.Contains((Get-UnitKey $_.Name)) -and -not $keepFiles.Contains($_.FullName) })
            foreach ($unit in $stale | Group-Object { Get-UnitKey $_.Name }) {
                # Units without an executable or rlib are cargo check/clippy metadata, which is not part of any build.
                if (-not ($unit.Group | Where-Object { $_.Extension -in '.exe', '.rlib' })) { continue }
                foreach ($file in $unit.Group) { Add-Plan 'target' "superseded $flavor outputs" $file.FullName $file.Length }
            }
            foreach ($build in Get-ChildItem -LiteralPath "$dir\build" -Directory -ErrorAction SilentlyContinue) {
                if (-not $keepDirs.Contains($build.FullName)) { Add-Plan 'target' "superseded $flavor build scripts" $build.FullName (Get-Usage $build.FullName).Bytes }
            }
        }
    }

    if ($areas -contains 'dev' -and (Test-Path -LiteralPath '.dev')) {
        # Each OpenOCD candidate is staged as its own bundle of patched sources; only the newest is current.
        $bundles = @(Get-ChildItem -LiteralPath '.dev' -Directory -Filter 'openocd-windows-*' | Sort-Object {
            $zip = "$($_.FullName)\corresponding-source.zip"
            if (Test-Path -LiteralPath $zip) { (Get-Item -LiteralPath $zip).LastWriteTime } else { $_.LastWriteTime }
        })
        if ($bundles.Count -gt 1) {
            foreach ($bundle in $bundles | Select-Object -First ($bundles.Count - 1)) { Add-Plan 'dev' 'superseded OpenOCD bundles' $bundle.FullName (Get-Usage $bundle.FullName).Bytes }
        }
    }

    if (-not $plan.Count) { Write-Output 'Nothing to clean.'; return }
    $total = [long]($plan | Measure-Object Bytes -Sum).Sum
    foreach ($group in $plan | Group-Object Area, Reason) {
        Write-Output ('{0,-9} {1,5} x {2,-34} {3,9}' -f $group.Group[0].Area, $group.Count, $group.Group[0].Reason, (Format-Size ([long]($group.Group | Measure-Object Bytes -Sum).Sum)))
    }
    foreach ($entry in $plan) { Write-Verbose ('{0,9}  {1}' -f (Format-Size $entry.Bytes), $entry.Path) }
    if (-not $Apply) { Write-Output "Dry run: $(Format-Size $total) would be removed. Re-run with -Apply to delete it."; return }

    $busy = @(Get-Process -Name cargo, rustc, debugtui, openocd, arm-none-eabi-gdb, JLinkGDBServerCL -ErrorAction SilentlyContinue)
    if ($busy) { throw "Stop running builds, tests and debug sessions first: $(($busy.ProcessName | Sort-Object -Unique) -join ', ')" }
    $failed = 0
    foreach ($entry in $plan) {
        try { Remove-Tree $entry.Path } catch { Write-Warning $_.Exception.Message; $failed++ }
    }
    if ($failed) { throw "$failed of $($plan.Count) items could not be removed." }
    Write-Output "Removed $($plan.Count) items, $(Format-Size $total)."
} finally { Pop-Location }
