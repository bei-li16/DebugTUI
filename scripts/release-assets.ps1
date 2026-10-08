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
foreach ($required in @('profiles/install.cjs','profiles/devices.toml','profiles/single-core-project.toml.example','profiles/local-program-project.toml.example','profiles/two-core-project.toml.example','profiles/registers/armv7m-common.toml','profiles/registers/cortex-m3.toml','profiles/registers/cortex-m4.toml','profiles/registers/cortex-m7.toml','profiles/registers/cortex-r52.toml','profiles/registers/cortex-r52+.toml','profiles/registers-readonly-multicore.toml.example','profiles/registers-readonly-r52.toml.example','docs/registers-readonly-guide.md','tests/cases/registers-readonly-release.md')) {
    $shipped = Join-Path $verified $required
    if (-not (Test-Path -LiteralPath $shipped -PathType Leaf) -or (Get-FileHash -LiteralPath $shipped).Hash -ne (Get-FileHash -LiteralPath (Join-Path $projectRoot $required)).Hash) {
        throw "ZIP register catalogue/profile asset differs or is missing: $required"
    }
}
foreach ($entry in $package.files | Where-Object { $_.path -like 'tools/*' }) {
    if ((Get-FileHash -LiteralPath (Join-Path $verified $entry.path)).Hash -ne (Get-FileHash -LiteralPath (Join-Path $projectRoot $entry.path)).Hash) { throw "ZIP bundled tool differs: $($entry.path)" }
}
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
    & "$PSScriptRoot\package-tools.ps1" -OutputDirectory $artifactRoot
    $assets += Join-Path $artifactRoot 'debugtui-tools-arm-win-x64.zip'
}
$hashes = @($assets | ForEach-Object { ((Get-FileHash -LiteralPath $_ -Algorithm SHA256).Hash.ToLowerInvariant()) + '  ' + (Split-Path $_ -Leaf) })
[IO.File]::WriteAllText((Join-Path $artifactRoot 'SHA256SUMS.txt'), ($hashes -join "`n") + "`n", [Text.UTF8Encoding]::new($false))
@{version=$version;zip=$zipPath;exe=$exePath;tgz=$tgzPath;stableTgz=$stableTgzPath;assets=$assets;sha256sums=(Join-Path $artifactRoot 'SHA256SUMS.txt');verifiedDirectory=$verified} | ConvertTo-Json | Set-Content "$artifactRoot\release-assets.json" -Encoding utf8
Write-Output 'PASS portable ZIP: executable, bundled tools, version and TUI renderer.'
$assets | ForEach-Object { $file = Get-Item -LiteralPath $_; Write-Output "$($file.Name): $($file.Length) bytes" }
