param(
    [string]$OutputDirectory = (Join-Path (Split-Path $PSScriptRoot) 'artifacts'),
    # Rewrite dependencies.lock.json from the files now under bin/, then stop.
    [switch]$UpdateLock
)
$ErrorActionPreference = 'Stop'
$lockPath = Join-Path $PSScriptRoot 'dependencies.lock.json'
if ($UpdateLock) {
    $manifest = Get-Content $lockPath -Raw | ConvertFrom-Json
    $binRoot = Join-Path $PSScriptRoot 'bin'
    $entries = foreach ($file in Get-ChildItem -LiteralPath $binRoot -File -Recurse -Force) {
        [pscustomobject]@{
            Path = 'bin/' + $file.FullName.Substring($binRoot.Length + 1).Replace('\','/')
            Bytes = $file.Length
            Sha256 = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        }
    }
    $paths = [string[]]@($entries.Path)
    [Array]::Sort($paths, [StringComparer]::Ordinal)
    $byPath = @{}
    foreach ($entry in $entries) { $byPath[$entry.Path] = $entry }
    $json = { param($text) '"' + $text.Replace('\','\\').Replace('"','\"') + '"' }
    $lines = [Collections.Generic.List[string]]::new()
    $lines.Add('{')
    $lines.Add('  "format": 2,')
    $lines.Add('  "profile": ' + (& $json $manifest.profile) + ',')
    $lines.Add('  "files": {')
    for ($i = 0; $i -lt $paths.Count; $i++) {
        $entry = $byPath[$paths[$i]]
        $lines.Add('    ' + (& $json $entry.Path) + ': {')
        $lines.Add('      "bytes": ' + $entry.Bytes + ',')
        $lines.Add('      "sha256": "' + $entry.Sha256 + '"')
        $lines.Add('    }' + $(if ($i -lt $paths.Count - 1) { ',' } else { '' }))
    }
    $lines.Add('  }')
    $lines.Add('}')
    [IO.File]::WriteAllText($lockPath, ($lines -join "`n") + "`n", [Text.UTF8Encoding]::new($false))
    Write-Output "Updated $lockPath ($($paths.Count) files)"
    return
}
New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
$outputRoot = (Resolve-Path -LiteralPath $OutputDirectory).Path
$manifest = Get-Content $lockPath -Raw | ConvertFrom-Json
foreach ($entry in $manifest.files.PSObject.Properties) {
    if ((Get-FileHash -LiteralPath (Join-Path $PSScriptRoot $entry.Name)).Hash -ne $entry.Value.sha256) { throw "Tool checksum mismatch: $($entry.Name)" }
}
# Runtime files only: profiles, board configs, the installer and the locked binaries.
$locked = [Collections.Generic.HashSet[string]]::new([string[]]@($manifest.files.PSObject.Properties.Name), [StringComparer]::Ordinal)
$files = foreach ($file in Get-ChildItem -LiteralPath $PSScriptRoot -File -Recurse -Force) {
    $relative = $file.FullName.Substring($PSScriptRoot.Length + 1).Replace('\','/')
    if ($relative.StartsWith('bin/')) {
        if (-not $locked.Contains($relative)) { throw "Tool file missing from dependencies.lock.json: $relative" }
    } elseif (-not ($relative -in @('README.md','dependencies.lock.json','install.bat','install.ps1') -or $relative -like 'debug-env*.toml' -or $relative -like 'config/*')) {
        continue
    }
    [pscustomobject]@{ Path = $file.FullName; Entry = 'tools/' + $relative }
}
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem
$zipPath = Join-Path $outputRoot 'debugtui-tools-arm-win-x64.zip'
$stream = [IO.File]::Open($zipPath,[IO.FileMode]::Create)
$zip = [IO.Compression.ZipArchive]::new($stream,[IO.Compression.ZipArchiveMode]::Create,$false)
try {
    foreach ($file in $files) {
        $null = [IO.Compression.ZipFileExtensions]::CreateEntryFromFile($zip,$file.Path,$file.Entry,[IO.Compression.CompressionLevel]::Optimal)
    }
} finally { $zip.Dispose(); $stream.Dispose() }
Write-Output "PASS separate environment package: $zipPath"
