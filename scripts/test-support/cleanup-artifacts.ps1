param(
    [Parameter(Mandatory)][string]$Root,
    [Parameter(Mandatory)][ValidateSet('Distribution','Packaging')][string]$Kind,
    [string[]]$PackagingDirectories = @(),
    [switch]$Apply
)
$ErrorActionPreference = 'Stop'
$cleanupRoot = [IO.Path]::GetFullPath($Root).TrimEnd('\','/')
if ($cleanupRoot -eq [IO.Path]::GetPathRoot($cleanupRoot).TrimEnd('\','/')) {
    throw 'A filesystem root is never a cleanup directory'
}

function Assert-NoLinks([string]$Path, [switch]$Tree) {
    $item = Get-Item -LiteralPath $Path -Force
    while ($item) {
        if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) {
            throw "Refusing reparse point: $($item.FullName)"
        }
        $item = if ($item.PSIsContainer) { $item.Parent } else { $item.Directory }
    }
    $pending = [Collections.Generic.Stack[string]]::new()
    if ($Tree -and (Test-Path -LiteralPath $Path -PathType Container)) { $pending.Push($Path) }
    while ($pending.Count) {
        foreach ($child in Get-ChildItem -LiteralPath $pending.Pop() -Force) {
            if ($child.Attributes -band [IO.FileAttributes]::ReparsePoint) {
                throw "Refusing nested reparse point: $($child.FullName)"
            }
            if ($child.PSIsContainer) { $pending.Push($child.FullName) }
        }
    }
}
Assert-NoLinks $cleanupRoot
$payloads = @()
if ($Kind -eq 'Distribution') {
    if ((Split-Path $cleanupRoot -Leaf) -notmatch '^register-distribution-\d+-[0-9a-f]{8}$') {
        throw 'Only a named distribution test run can be cleaned'
    }
    $report = Get-Content -LiteralPath (Join-Path $cleanupRoot 'report.json') -Raw | ConvertFrom-Json
    if ($report.passed -ne $true -or $report.counts.passed -ne 14 -or $report.counts.failed -ne 0 -or
        $report.counts.skipped -ne 0 -or @($report.cases).Count -ne 14 -or
        @($report.cases | Where-Object status -ne 'passed').Count) {
        throw 'Distribution cleanup requires the completed 14/14 passing report'
    }
    foreach ($name in @('direct EXE','EXE upgrade','ZIP install','ZIP upgrade','executable case leak',
            'failed prefix','npm-cache','old fixture','old package bytes','omitted docs---md',
            'omitted docs-','omitted profiles-','omitted tests-cases-','private prefix','project 工程','unpacked npm')) {
        $payload = Join-Path $cleanupRoot $name
        if (Test-Path -LiteralPath $payload) { $payloads += $payload }
    }
    $payloads += @(Get-ChildItem -LiteralPath $cleanupRoot -File -Filter '*.tgz' | ForEach-Object FullName)
} else {
    foreach ($directory in $PackagingDirectories) {
        $payload = [IO.Path]::GetFullPath($directory)
        if ((Split-Path $payload -Leaf) -notmatch '^release-(staging|verify)-[0-9a-f]{32}$' -or
            (Split-Path $payload -Parent) -ne $cleanupRoot) {
            throw "Only immediate generated packaging directories are allowed: $payload"
        }
        if (Test-Path -LiteralPath $payload) { $payloads += $payload }
    }
}
$plan = @($payloads | Sort-Object -Unique | ForEach-Object {
    $resolved = [IO.Path]::GetFullPath($_)
    if (-not $resolved.StartsWith($cleanupRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Cleanup path escapes its root: $resolved"
    }
    Assert-NoLinks $resolved -Tree
    $files = if (Test-Path -LiteralPath $resolved -PathType Container) {
        @(Get-ChildItem -LiteralPath $resolved -File -Recurse -Force)
    } else { @(Get-Item -LiteralPath $resolved) }
    [pscustomobject]@{path=$resolved; bytes=($files | Measure-Object Length -Sum).Sum; files=$files.Count}
})
if ($Apply) {
    # Check the entire plan before any deletion. Refuse processes still using this run.
    $busy = @(Get-CimInstance Win32_Process | Where-Object {
        $_.ProcessId -ne $PID -and
        ($_.Name -match '^(cargo|rustc|debugtui|openocd|arm-none-eabi-gdb|node|npm|tar)(\.exe)?$') -and
        (($_.ExecutablePath -and $_.ExecutablePath.StartsWith($cleanupRoot + '\', [StringComparison]::OrdinalIgnoreCase)) -or
         ($_.CommandLine -and $_.CommandLine.IndexOf($cleanupRoot, [StringComparison]::OrdinalIgnoreCase) -ge 0))
    })
    if ($busy.Count) { throw 'A process is still using the generated test/package directory' }
    if ($Kind -eq 'Distribution') {
        # Keep the production manifest, checksums and renderer evidence before dropping its source copy.
        $evidence = Join-Path $cleanupRoot 'packaging-evidence'
        New-Item -ItemType Directory -Path $evidence -Force | Out-Null
        foreach ($name in @('npm-pack.json','release-assets.json','SHA256SUMS.txt','release-demo.txt')) {
            $source = Join-Path $cleanupRoot "project 工程/artifacts/$name"
            if (Test-Path -LiteralPath $source -PathType Leaf) {
                Copy-Item -LiteralPath $source -Destination (Join-Path $evidence $name) -Force
            }
        }
    }
    foreach ($entry in $plan) { Remove-Item -LiteralPath $entry.path -Recurse -Force }
}
$result = [pscustomobject]@{kind=$Kind; root=$cleanupRoot; applied=[bool]$Apply; logical_bytes=($plan | Measure-Object bytes -Sum).Sum; removed_payloads=$plan; retained='Reports, process logs, customer configuration fixtures and packaging evidence'}
if ($Apply -and $Kind -eq 'Distribution') {
    $result | ConvertTo-Json -Depth 5 -Compress | Add-Content -LiteralPath (Join-Path $cleanupRoot 'cleanup-history.jsonl') -Encoding utf8
    $result | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $cleanupRoot 'cleanup.json') -Encoding utf8
}
$result | ConvertTo-Json -Depth 5
