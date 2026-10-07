param([string]$OutputDirectory = (Join-Path (Split-Path $PSScriptRoot) 'artifacts'))
$ErrorActionPreference = 'Stop'
$toolsRoot = Join-Path (Split-Path $PSScriptRoot) 'tools'
foreach ($name in @('bin/gdb/bin/arm-none-eabi-gdb.exe', 'bin/openocd/bin/openocd.exe')) {
    if (-not (Test-Path -LiteralPath (Join-Path $toolsRoot $name) -PathType Leaf)) { throw "Missing tool file: $name" }
}
New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
$outputRoot = (Resolve-Path -LiteralPath $OutputDirectory).Path
# Runtime files only: profiles, board configs, chip descriptions, installer and bin/.
$files = foreach ($file in Get-ChildItem -LiteralPath $toolsRoot -File -Recurse -Force) {
    $relative = $file.FullName.Substring($toolsRoot.Length + 1).Replace('\','/')
    if (-not ($relative -in @('README.md','install.ps1','debug.toml','debug-env.toml') -or $relative -like 'bin/*' -or $relative -like 'openocd/*' -or $relative -like 'svd/*' -or $relative -like 'devices/*')) {
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
