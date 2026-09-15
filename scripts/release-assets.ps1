param([switch]$SkipBuild)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path $PSScriptRoot
& "$PSScriptRoot\package.ps1" -SkipBuild:$SkipBuild
$package = (Get-Content "$projectRoot\artifacts\npm-pack.json" -Raw | ConvertFrom-Json)[0]
$version = $package.version
if ($version -notmatch '^\d+\.\d+\.\d+([+-][0-9A-Za-z.-]+)?$') { throw 'Invalid release version' }
$artifactRoot = Join-Path $projectRoot 'artifacts'
$staging = Join-Path $artifactRoot ('release-staging-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $staging -Force | Out-Null
foreach ($entry in $package.files) {
    if ($entry.path -eq 'package.json') { continue }
    $relative = if ($entry.path -eq 'bin/debugtui.exe') { 'debugtui.exe' } else { $entry.path }
    $destination = Join-Path $staging $relative
    New-Item -ItemType Directory -Path (Split-Path $destination) -Force | Out-Null
    Copy-Item -LiteralPath (Join-Path $projectRoot $entry.path) -Destination $destination
}

Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem
$zipPath = Join-Path $artifactRoot "debugtui-$version-win-x64.zip"
$stream = [IO.File]::Open($zipPath, [IO.FileMode]::Create)
$zip = [IO.Compression.ZipArchive]::new($stream, [IO.Compression.ZipArchiveMode]::Create, $false)
try {
    foreach ($file in Get-ChildItem -LiteralPath $staging -File -Recurse -Force | Sort-Object FullName) {
        $relative = $file.FullName.Substring($staging.Length + 1).Replace('\','/')
        $null = [IO.Compression.ZipFileExtensions]::CreateEntryFromFile($zip, $file.FullName, $relative, [IO.Compression.CompressionLevel]::Optimal)
    }
} finally { $zip.Dispose(); $stream.Dispose() }

$verified = Join-Path $artifactRoot ('release-verify-' + [Guid]::NewGuid().ToString('N'))
[IO.Compression.ZipFile]::ExtractToDirectory($zipPath, $verified)
$exe = Join-Path $verified 'debugtui.exe'
if ((Get-FileHash -LiteralPath $exe).Hash -ne (Get-FileHash -LiteralPath "$projectRoot\bin\debugtui.exe").Hash) { throw 'ZIP executable differs from tested executable' }
$dependencies = Get-Content "$verified\tools\dependencies.lock.json" -Raw | ConvertFrom-Json
foreach ($entry in $dependencies.files.PSObject.Properties) {
    if ((Get-FileHash -LiteralPath (Join-Path "$verified\tools" $entry.Name) -Algorithm SHA256).Hash -ne $entry.Value.sha256) { throw "ZIP dependency mismatch: $($entry.Name)" }
}
if ((& $exe --version) -ne "debugtui $version") { throw 'ZIP version check failed' }
& $exe --snapshot "$artifactRoot\release-demo.txt"
if ($LASTEXITCODE -ne 0) { throw 'ZIP renderer check failed' }
$tgzPath = Join-Path $artifactRoot $package.filename
$assets = @($zipPath, $tgzPath)
$hashes = @($assets | ForEach-Object { ((Get-FileHash -LiteralPath $_ -Algorithm SHA256).Hash.ToLowerInvariant()) + '  ' + (Split-Path $_ -Leaf) })
[IO.File]::WriteAllText((Join-Path $artifactRoot 'SHA256SUMS.txt'), ($hashes -join "`n") + "`n", [Text.UTF8Encoding]::new($false))
@{version=$version;zip=$zipPath;tgz=$tgzPath;sha256sums=(Join-Path $artifactRoot 'SHA256SUMS.txt');verifiedDirectory=$verified} | ConvertTo-Json | Set-Content "$artifactRoot\release-assets.json" -Encoding utf8
Write-Output 'PASS portable ZIP: executable, complete dependency hashes, version and TUI renderer.'
$assets | ForEach-Object { $file = Get-Item -LiteralPath $_; Write-Output "$($file.Name): $($file.Length) bytes" }
