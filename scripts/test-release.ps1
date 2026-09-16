param(
    [string]$Repository = 'bei-li16/DebugTUI',
    [string]$PreviousVersion
)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path $PSScriptRoot
$metadata = Get-Content "$projectRoot\package.json" -Raw | ConvertFrom-Json
$version = $metadata.version
$runRoot = Join-Path $projectRoot ('artifacts\public-release-' + (Get-Date -Format yyyyMMdd-HHmmss))
$prefix = Join-Path $runRoot 'prefix'
New-Item -ItemType Directory -Path $prefix -Force | Out-Null
$url = "https://github.com/$Repository/releases/latest/download/debugtui-cli.tgz"
$latest = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repository/releases/latest" -Headers @{'User-Agent'='DebugTUI-release-test'}
if ($latest.tag_name -ne "v$version") { throw "Latest release is $($latest.tag_name), expected v$version" }
if (@($latest.assets | Where-Object name -eq 'debugtui-cli.tgz').Count -ne 1) { throw 'Latest release is missing the fixed npm asset' }
$download = Join-Path $runRoot 'debugtui-cli.tgz'
Invoke-WebRequest -Uri $url -OutFile $download
if ((Get-FileHash -LiteralPath $download).Hash -ne (Get-FileHash -LiteralPath "$projectRoot\artifacts\debugtui-cli.tgz").Hash) { throw 'Public latest download differs from local release package' }

$config = Join-Path $runRoot 'debug.toml'
"version=2`nwatch=['keep_this']" | Set-Content -LiteralPath $config -Encoding utf8
$configHash = (Get-FileHash -LiteralPath $config).Hash
if ($PreviousVersion) {
    $oldUrl = "https://github.com/$Repository/releases/download/v$PreviousVersion/debugtui-cli-$PreviousVersion.tgz"
    & npm.cmd install --global --prefix $prefix --prefer-online --ignore-scripts --no-audit --no-fund $oldUrl
    if ($LASTEXITCODE -ne 0) { throw 'Previous public release installation failed' }
    if ((& "$prefix\debugtui.cmd" --version) -ne "debugtui $PreviousVersion") { throw 'Previous public version mismatch' }
}
for ($attempt = 1; $attempt -le 2; $attempt++) {
    & npm.cmd install --global --prefix $prefix --prefer-online --ignore-scripts --no-audit --no-fund $url
    if ($LASTEXITCODE -ne 0) { throw 'Public latest URL installation failed' }
    if ((& "$prefix\debugtui.cmd" --version) -ne "debugtui $version") { throw 'Installed public version mismatch' }
    if ((& "$prefix\debugtui.ps1" --version) -ne "debugtui $version") { throw 'Installed PowerShell entry mismatch' }
    if ((Get-FileHash -LiteralPath $config).Hash -ne $configHash) { throw 'Upgrade changed project configuration' }
}
$installedRoot = Join-Path "$prefix\node_modules" $metadata.name
if (Test-Path -LiteralPath "$installedRoot\tools") { throw 'Old bundled tools survived the standalone-package upgrade' }
if ((Get-FileHash -LiteralPath "$installedRoot\bin\debugtui.exe").Hash -ne (Get-FileHash -LiteralPath "$projectRoot\bin\debugtui.exe").Hash) { throw 'Installed public binary differs from tested binary' }
& "$prefix\debugtui.cmd" --snapshot "$runRoot\public-ui.txt"
if ($LASTEXITCODE -ne 0) { throw 'Public package renderer failed' }
& npm.cmd uninstall --global --prefix $prefix --ignore-scripts --no-audit --no-fund $metadata.name
if ($LASTEXITCODE -ne 0 -or (Test-Path "$prefix\debugtui.cmd")) { throw 'Public package uninstall failed' }
@{version=$version;previousVersion=$PreviousVersion;url=$url;artifacts=$runRoot;sha256=(Get-FileHash -LiteralPath $download).Hash} | ConvertTo-Json | Set-Content "$runRoot\result.json" -Encoding utf8
Write-Output "PASS public latest: checksum, install/upgrade, repeated install from the same URL, CMD/PowerShell, renderer, preserved project config, standalone tools boundary, uninstall. Artifacts: $runRoot"
