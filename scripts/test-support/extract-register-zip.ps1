param([Parameter(Mandatory=$true)][string]$Archive,[Parameter(Mandatory=$true)][string]$Destination)
$ErrorActionPreference='Stop'
Add-Type -AssemblyName System.IO.Compression.FileSystem
if (Test-Path -LiteralPath $Destination) { throw 'Extraction destination must be new' }
[IO.Compression.ZipFile]::ExtractToDirectory((Resolve-Path -LiteralPath $Archive).Path,[IO.Path]::GetFullPath($Destination))
