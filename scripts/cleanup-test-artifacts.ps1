param(
    [string]$Root = (Join-Path (Split-Path $PSScriptRoot) 'artifacts'),
    [string]$RunDirectory,
    [string]$DirectoryName,
    [int]$OwnerProcessId = 0,
    [switch]$Apply,
    [string]$RestorePath
)
$ErrorActionPreference = 'Stop'
$storageRoot = [IO.Path]::GetFullPath($Root).TrimEnd('\','/')
$repositoryRoot = [IO.Path]::GetFullPath((Split-Path $PSScriptRoot))
$pool = Join-Path $storageRoot '.fixture-store'
$journal = Join-Path $pool 'manifest.jsonl'
$verifiedBlobs = @{}
$defaultArtifacts = Join-Path $repositoryRoot 'artifacts'
$configuredArtifacts = if ($env:DEBUGTUI_TEST_ARTIFACT_ROOT) { [IO.Path]::GetFullPath($env:DEBUGTUI_TEST_ARTIFACT_ROOT).TrimEnd('\','/') } else { $null }
if (-not ($storageRoot -eq $defaultArtifacts -or
    $storageRoot.StartsWith($defaultArtifacts + '\', [StringComparison]::OrdinalIgnoreCase) -or
    $storageRoot -eq $configuredArtifacts -or $RunDirectory)) { throw 'Only repository artifacts or the configured test artifact root can be maintained' }

function Assert-SafePath([string]$Path) {
    $full = [IO.Path]::GetFullPath($Path)
    if (-not $full.StartsWith($storageRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Path escapes artifact root: $full"
    }
    $itemPath = $full
    while (-not (Test-Path -LiteralPath $itemPath)) { $itemPath = Split-Path $itemPath -Parent }
    $item = Get-Item -LiteralPath $itemPath -Force
    while ($item) {
        if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Linked path refused: $($item.FullName)" }
        $item = if ($item.PSIsContainer) { $item.Parent } else { $item.Directory }
    }
    return $full
}
if ($storageRoot -eq [IO.Path]::GetPathRoot($storageRoot).TrimEnd('\','/')) { throw 'Filesystem root refused' }
if (-not (Test-Path -LiteralPath $storageRoot -PathType Container)) { throw 'Artifact root does not exist' }
$null = Assert-SafePath $pool

function Digest([string]$Path) { (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() }
function Gzip-Digest([string]$Path) {
    $inputFile = [IO.File]::OpenRead($Path)
    $gzip = [IO.Compression.GZipStream]::new($inputFile, [IO.Compression.CompressionMode]::Decompress)
    $sha = [Security.Cryptography.SHA256]::Create()
    try { [Convert]::ToHexString($sha.ComputeHash($gzip)).ToLowerInvariant() }
    finally { $sha.Dispose(); $gzip.Dispose(); $inputFile.Dispose() }
}
function Archive([string]$Path, [string]$Sha) {
    $blob = Assert-SafePath (Join-Path $pool "$Sha.gz")
    if ($verifiedBlobs.ContainsKey($Sha)) { return $blob }
    if (-not (Test-Path -LiteralPath $blob)) {
        $temporary = Assert-SafePath (Join-Path $pool ("$Sha-" + [Guid]::NewGuid().ToString('N') + '.tmp'))
        $source = [IO.File]::OpenRead($Path)
        $destination = [IO.File]::Create($temporary)
        $gzip = [IO.Compression.GZipStream]::new($destination, [IO.Compression.CompressionLevel]::Optimal)
        try { $source.CopyTo($gzip) }
        finally { $source.Dispose(); $gzip.Dispose(); $destination.Dispose() }
        if ((Gzip-Digest $temporary) -ne $Sha) { throw "Archive verification failed: $Path" }
        Move-Item -LiteralPath $temporary -Destination $blob
    }
    if ((Gzip-Digest $blob) -ne $Sha) { throw "Corrupt archive; original retained: $Path" }
    $verifiedBlobs[$Sha] = Digest $blob
    return $blob
}
if ($RestorePath) {
    $destination = Assert-SafePath $RestorePath
    if (Test-Path -LiteralPath $destination) { throw 'Restore never overwrites an existing file' }
    $entry = Get-Content -LiteralPath $journal | ForEach-Object { $_ | ConvertFrom-Json } |
        Where-Object { $_.path -eq $destination } | Select-Object -Last 1
    if (-not $entry) { throw 'Path has no restoration entry' }
    $blob = Assert-SafePath $entry.archive
    if ((Digest $blob) -ne $entry.archive_sha256 -or (Gzip-Digest $blob) -ne $entry.sha256) { throw 'Archive checksum mismatch' }
    New-Item -ItemType Directory -Path (Split-Path $destination) -Force | Out-Null
    $temporary = Assert-SafePath ($destination + '.' + [Guid]::NewGuid().ToString('N') + '.restore')
    $inputFile = [IO.File]::OpenRead($blob)
    $gzip = [IO.Compression.GZipStream]::new($inputFile, [IO.Compression.CompressionMode]::Decompress)
    $outputFile = [IO.File]::Open($temporary, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    try { $gzip.CopyTo($outputFile) }
    finally { $outputFile.Dispose(); $gzip.Dispose(); $inputFile.Dispose() }
    if ((Digest $temporary) -ne $entry.sha256) { throw 'Restored content checksum mismatch' }
    Move-Item -LiteralPath $temporary -Destination $destination
    Write-Output "Restored and verified: $destination"
    return
}

$lock = $null
if ($Apply) {
    New-Item -ItemType Directory -Path $pool -Force | Out-Null
    $lockPath = Assert-SafePath (Join-Path $pool '.lock')
    $deadline = [DateTime]::UtcNow.AddSeconds(30)
    do {
        try { $lock = [IO.File]::Open($lockPath, [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None) }
        catch {
            if ([DateTime]::UtcNow -ge $deadline) { throw 'Another cleanup owns the archive store; all original payloads retained' }
            Start-Sleep -Milliseconds 200
        }
    } while (-not $lock)
}
try {
$skipped = [Collections.Generic.List[object]]::new()
$processes = @(Get-CimInstance Win32_Process)
$parentId = ($processes | Where-Object ProcessId -eq $PID).ParentProcessId
if ($OwnerProcessId -and $OwnerProcessId -ne $parentId) { throw 'Only the calling test owner may finish its run' }
function Busy([string]$Directory, [switch]$Legacy) {
    $progress = Join-Path $Directory 'progress.json'
    if (Test-Path -LiteralPath $progress) {
        $null = Assert-SafePath $progress
        $activity = Get-Content -LiteralPath $progress -Raw | ConvertFrom-Json
        if ($activity.status -in @('in_progress','running','active','paused','blocked')) { return $true }
    }
    $marker = Join-Path $Directory '.test-run.json'
    if (Test-Path -LiteralPath $marker) {
        $null = Assert-SafePath $marker
        $lease = Get-Content -LiteralPath $marker -Raw | ConvertFrom-Json
        if ($lease.state -ne 'completed') { return $true }
        $owner = $processes | Where-Object ProcessId -eq $lease.pid | Select-Object -First 1
        # PID reuse can only delay cleanup; it can never make an active run eligible.
        if ($owner) {
            if (-not ($lease.state -eq 'completed' -and $lease.pid -eq $OwnerProcessId)) { return $true }
        }
    }
    foreach ($entry in $processes) {
        if ($entry.ProcessId -in @($PID,$OwnerProcessId)) { continue }
        if ($Legacy -and $entry.Name -match '^(cargo|rustc)(\.exe)?$') { return $true }
        if (($entry.ExecutablePath -and $entry.ExecutablePath.StartsWith($Directory + '\', [StringComparison]::OrdinalIgnoreCase)) -or
            ($entry.CommandLine -and $entry.CommandLine.IndexOf($Directory, [StringComparison]::OrdinalIgnoreCase) -ge 0)) { return $true }
    }
    return $false
}

$automatic = [bool]$RunDirectory
if ($automatic) {
    $run = Assert-SafePath $RunDirectory
    if ((Split-Path $run -Parent) -ne $storageRoot) { throw 'Only an immediate owned test run can be cleaned automatically' }
    $null = Assert-SafePath (Join-Path $run 'report.json')
    $null = Assert-SafePath (Join-Path $run '.test-run.json')
    $lease = Get-Content -LiteralPath (Join-Path $run '.test-run.json') -Raw | ConvertFrom-Json
    $report = Get-Content -LiteralPath (Join-Path $run 'report.json') -Raw | ConvertFrom-Json
    if ($lease.schema -ne 1 -or $lease.state -ne 'completed' -or $report.passed -ne $true -or
        $report.board_tests_executed -ne $false -or $report.counts.failed -ne 0 -or $report.counts.skipped -ne 0 -or
        $report.counts.passed -le 0 -or @($report.cases).Count -ne $report.counts.passed -or
        @($report.cases | Where-Object status -ne 'passed').Count) { throw 'Cleanup requires a completed, passing, software-only test report' }
    $directories = @(Get-Item -LiteralPath $run)
} else {
    if ($DirectoryName -and ($DirectoryName -ne [IO.Path]::GetFileName($DirectoryName) -or $DirectoryName -in @('.','..'))) { throw 'Invalid run directory name' }
    $directories = @(Get-ChildItem -LiteralPath $storageRoot -Directory -Force | Where-Object {
        $_.Name -notmatch '^(\.fixture-store|storage-audit|release-|register-.*stm32-|cleanup-tests-)' -and
        (-not $DirectoryName -or $_.Name -eq $DirectoryName)
    })
}

$files = [Collections.Generic.List[object]]::new()
foreach ($directory in $directories) {
    if ($directory.Attributes -band [IO.FileAttributes]::ReparsePoint) { $skipped.Add(@{path=$directory.FullName;reason='link'}); continue }
    if (Busy $directory.FullName -Legacy:($false -eq (Test-Path -LiteralPath (Join-Path $directory.FullName '.test-run.json')))) {
        $skipped.Add(@{path=$directory.FullName;reason='active process or test lease'}); continue
    }
    $reportPath = Join-Path $directory.FullName 'report.json'
    if (Test-Path -LiteralPath $reportPath) {
        $null = Assert-SafePath $reportPath
        $metadata = Get-Content -LiteralPath $reportPath -Raw | ConvertFrom-Json
        if ($metadata.board_tests_executed -eq $true -or $metadata.passed -eq $false) {
            $skipped.Add(@{path=$directory.FullName;reason='hardware evidence or failed fixture'}); continue
        }
    }
    # Preflight the whole run before selecting any payloads: never follow junctions.
    $pending = [Collections.Generic.Stack[string]]::new(); $pending.Push($directory.FullName)
    $runFiles = [Collections.Generic.List[object]]::new(); $linked = $false
    while ($pending.Count) {
        foreach ($child in Get-ChildItem -LiteralPath $pending.Pop() -Force) {
            if ($child.Attributes -band [IO.FileAttributes]::ReparsePoint) { $linked = $true; continue }
            if ($child.PSIsContainer) {
                if ($child.Name -in @('src','scripts','tests','reference','.git','.codex','.claude') -or
                    $child.Name -match '(^|[-_])(stm32|hardware|board)([-_]|$)') { continue }
                $nestedReport = Join-Path $child.FullName 'report.json'
                if (Test-Path -LiteralPath $nestedReport) {
                    $null = Assert-SafePath $nestedReport
                    $metadata = Get-Content -LiteralPath $nestedReport -Raw | ConvertFrom-Json
                    if ($metadata.board_tests_executed -eq $true -or $metadata.passed -eq $false) { continue }
                }
                $pending.Push($child.FullName)
            } else { $runFiles.Add($child) }
        }
    }
    if ($linked) { $skipped.Add(@{path=$directory.FullName;reason='nested link'}); continue }
    foreach ($file in $runFiles) {
        # Do not touch source, reports, captures, hardware data, or official attachments.
        $payload = $file.Extension -in @('.toml','.exe','.dll','.tgz','.zip') -and $file.Length -ge 1MB -and
            $file.Name -ne 'corresponding-source.zip' -and $file.FullName -notmatch '[\\/](release-assets|release-\d|reference|\.git)[\\/]'
        $log = $automatic -and $file.Length -ge 1MB -and $file.Name -match '^(events-.*\.jsonl|stdout\.jsonl|process-.*\.(stdout|stderr)\.txt)$'
        if ($payload -or $log) {
            $files.Add([pscustomobject]@{path=$file.FullName;bytes=$file.Length;sha256=(Digest $file.FullName);run=$directory.FullName;log=[bool]$log})
        }
    }
}

# Release assets and repository originals are references, never deletion targets.
$references = @{}
$referenceFiles = @(Get-ChildItem -LiteralPath $storageRoot -File -Force | Where-Object { $_.Extension -in @('.exe','.tgz','.zip') })
foreach ($name in @('profiles/registers','bin','tools/bin')) {
    $location = Join-Path $repositoryRoot $name
    if (Test-Path -LiteralPath $location) { $referenceFiles += @(Get-ChildItem -LiteralPath $location -File -Recurse | Where-Object { $_.Length -ge 1MB -and $_.Extension -in @('.toml','.exe','.dll') }) }
}
foreach ($file in $referenceFiles) { $references[(Digest $file.FullName)] = $file.FullName }
$plan = [Collections.Generic.List[object]]::new()
foreach ($group in @($files | Group-Object sha256)) {
    $copies = @($group.Group | Sort-Object path)
    $canonical = $references[$group.Name]
    # A prior verified canonical original makes repeated runs share one content blob.
    $indexPath = Join-Path $pool ($group.Name + '.json')
    if (-not $canonical -and (Test-Path -LiteralPath $indexPath)) {
        $null = Assert-SafePath $indexPath
        $index = Get-Content -LiteralPath $indexPath -Raw | ConvertFrom-Json
        if ($index.canonical.StartsWith($storageRoot + '\', [StringComparison]::OrdinalIgnoreCase) -and
            (Test-Path -LiteralPath $index.canonical -PathType Leaf)) {
            $null = Assert-SafePath $index.canonical
            if ((Digest $index.canonical) -eq $group.Name) { $canonical = $index.canonical }
        }
    }
    if (-not $canonical) { $canonical = $copies[0].path }
    foreach ($copy in $copies) {
        if ($copy.log -or $copy.path -ne $canonical) {
            $plan.Add([pscustomobject]@{path=$copy.path;bytes=$copy.bytes;sha256=$copy.sha256;run=$copy.run;canonical=$canonical;action=if($copy.log){'archive-software-log'}else{'deduplicate'}})
        }
    }
}

$removed = [Collections.Generic.List[object]]::new()
$addedBytes = 0L
if ($Apply) {
        # Refresh process state immediately before mutation, and validate every path first.
        $processes = @(Get-CimInstance Win32_Process)
        foreach ($entry in $plan) {
            $null = Assert-SafePath $entry.path
            if (Busy $entry.run -Legacy:($false -eq (Test-Path -LiteralPath (Join-Path $entry.run '.test-run.json')))) { throw 'A selected test became active; no files removed' }
            if ((Digest $entry.path) -ne $entry.sha256) { throw 'Payload changed since planning; no files removed' }
        }
        foreach ($entry in $plan) {
            $blobPath = Join-Path $pool ($entry.sha256 + '.gz')
            $newBlob = -not (Test-Path -LiteralPath $blobPath)
            $blob = Archive $entry.path $entry.sha256
            if ($newBlob) { $addedBytes += (Get-Item -LiteralPath $blob).Length }
            $indexPath = Assert-SafePath (Join-Path $pool ($entry.sha256 + '.json'))
            @{sha256=$entry.sha256;canonical=$entry.canonical;archive=$blob} | ConvertTo-Json | Set-Content -LiteralPath $indexPath -Encoding utf8
            $record = [ordered]@{at_utc=[DateTime]::UtcNow.ToString('o');action=$entry.action;path=$entry.path;bytes=$entry.bytes;sha256=$entry.sha256;canonical=$entry.canonical;archive=$blob;archive_sha256=$verifiedBlobs[$entry.sha256]}
            # Journal and verify before deleting: a crash leaves either an original or recoverable bytes.
            $record | ConvertTo-Json -Compress | Add-Content -LiteralPath $journal -Encoding utf8
            $null = Assert-SafePath $entry.path
            if ((Digest $entry.path) -ne $entry.sha256) { throw 'Payload changed during archival; original retained' }
            Remove-Item -LiteralPath $entry.path -Force
            $removed.Add([pscustomobject]$record)
        }
}
$result = [ordered]@{root=$storageRoot;run=$RunDirectory;applied=[bool]$Apply;planned=$plan.Count;removed=$removed.Count;removed_bytes=($removed | Measure-Object bytes -Sum).Sum;archive_bytes_added=$addedBytes;skipped=@($skipped.ToArray());entries=if($Apply){@($removed.ToArray())}else{@($plan.ToArray())};restoration_manifest=$journal}
if ($Apply -and $automatic) { $result | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $run 'storage-cleanup.json') -Encoding utf8 }
$result | ConvertTo-Json -Depth 6
} finally { if ($lock) { $lock.Dispose() } }
