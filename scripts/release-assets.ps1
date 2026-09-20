param([switch]$SkipBuild, [switch]$IncludeTools)
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
if (Test-Path -LiteralPath "$verified\tools") { throw 'Environment tools leaked into standalone ZIP' }
if ((& $exe --version) -ne "debugtui $version") { throw 'ZIP version check failed' }
& $exe --snapshot "$artifactRoot\release-demo.txt"
if ($LASTEXITCODE -ne 0) { throw 'ZIP renderer check failed' }
$tgzPath = Join-Path $artifactRoot $package.filename
# Each release keeps this exact name, enabling /releases/latest/download/debugtui-cli.tgz.
$stableTgzPath = Join-Path $artifactRoot 'debugtui-cli.tgz'
Copy-Item -LiteralPath $tgzPath -Destination $stableTgzPath -Force
if ((Get-FileHash -LiteralPath $stableTgzPath).Hash -ne (Get-FileHash -LiteralPath $tgzPath).Hash) { throw 'Stable npm asset differs from versioned package' }
# Preserve the direct EXE download used by previous releases.
$exePath = Join-Path $artifactRoot "debugtui-windows-x64-$version.exe"
Copy-Item -LiteralPath (Join-Path $projectRoot 'bin/debugtui.exe') -Destination $exePath -Force
if ((Get-FileHash -LiteralPath $exePath).Hash -ne (Get-FileHash -LiteralPath $exe).Hash) { throw 'Direct EXE asset differs from verified ZIP' }
$assets = @($zipPath, $exePath, $tgzPath, $stableTgzPath)
if ($IncludeTools) {
    & "$projectRoot\tools\package.ps1" -OutputDirectory $artifactRoot
    $assets += Join-Path $artifactRoot 'debugtui-tools-stm32-jlink-win-x64.zip'
}
$hashes = @($assets | ForEach-Object { ((Get-FileHash -LiteralPath $_ -Algorithm SHA256).Hash.ToLowerInvariant()) + '  ' + (Split-Path $_ -Leaf) })
[IO.File]::WriteAllText((Join-Path $artifactRoot 'SHA256SUMS.txt'), ($hashes -join "`n") + "`n", [Text.UTF8Encoding]::new($false))
@{version=$version;zip=$zipPath;exe=$exePath;tgz=$tgzPath;stableTgz=$stableTgzPath;assets=$assets;sha256sums=(Join-Path $artifactRoot 'SHA256SUMS.txt');verifiedDirectory=$verified} | ConvertTo-Json | Set-Content "$artifactRoot\release-assets.json" -Encoding utf8
Write-Output 'PASS standalone ZIP: executable, no bundled tools, version and TUI renderer.'
$assets | ForEach-Object { $file = Get-Item -LiteralPath $_; Write-Output "$($file.Name): $($file.Length) bytes" }
