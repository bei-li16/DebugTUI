param([string]$OutputDirectory = (Join-Path (Split-Path $PSScriptRoot) 'artifacts'))
$ErrorActionPreference = 'Stop'
New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
$outputRoot = (Resolve-Path -LiteralPath $OutputDirectory).Path
$manifest = Get-Content "$PSScriptRoot\dependencies.lock.json" -Raw | ConvertFrom-Json
foreach ($entry in $manifest.files.PSObject.Properties) {
    if ((Get-FileHash -LiteralPath (Join-Path $PSScriptRoot $entry.Name)).Hash -ne $entry.Value.sha256) { throw "Tool checksum mismatch: $($entry.Name)" }
}
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem
$zipPath = Join-Path $outputRoot 'debugtui-tools-stm32-jlink-win-x64.zip'
$stream = [IO.File]::Open($zipPath,[IO.FileMode]::Create)
$zip = [IO.Compression.ZipArchive]::new($stream,[IO.Compression.ZipArchiveMode]::Create,$false)
try {
    foreach ($file in Get-ChildItem -LiteralPath $PSScriptRoot -File -Recurse -Force) {
        if ($file.FullName.StartsWith($outputRoot + [IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)) { continue }
        if ($file.Name -in @('.gitignore','.gitattributes','package.ps1')) { continue }
        $relative = 'tools/' + $file.FullName.Substring($PSScriptRoot.Length + 1).Replace('\','/')
        $null = [IO.Compression.ZipFileExtensions]::CreateEntryFromFile($zip,$file.FullName,$relative,[IO.Compression.CompressionLevel]::Optimal)
    }
} finally { $zip.Dispose(); $stream.Dispose() }
Write-Output "PASS separate environment package: $zipPath"
