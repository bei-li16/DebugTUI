param(
    [string]$Repository = 'bei-li16/DebugTUI',
    [string]$PreviousVersion,
    [string]$ReleaseTag
)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path $PSScriptRoot
$metadata = Get-Content "$projectRoot\package.json" -Raw | ConvertFrom-Json
$version = $metadata.version
$runRoot = Join-Path $projectRoot ('artifacts\public-release-' + (Get-Date -Format yyyyMMdd-HHmmss))
$prefix = Join-Path $runRoot 'prefix'
New-Item -ItemType Directory -Path $prefix -Force | Out-Null
$oldConfigDirectory = $env:DEBUGTUI_CONFIG_DIR
$userDirectory = Join-Path $runRoot 'user-profile'
$env:DEBUGTUI_CONFIG_DIR = $userDirectory
try {
if ($ReleaseTag) {
    if ($ReleaseTag -notmatch '^v\d+\.\d+\.\d+([+-][0-9A-Za-z.-]+)?$') { throw 'Invalid release tag' }
    $url = "https://github.com/$Repository/releases/download/$ReleaseTag/debugtui-cli.tgz"
    $releaseApi = "https://api.github.com/repos/$Repository/releases/tags/$ReleaseTag"
} else {
    $url = "https://github.com/$Repository/releases/latest/download/debugtui-cli.tgz"
    $releaseApi = "https://api.github.com/repos/$Repository/releases/latest"
}
$published = Invoke-RestMethod -Uri $releaseApi -Headers @{'User-Agent'='DebugTUI-release-test'}
if ($published.tag_name -ne "v$version" -or $published.draft) { throw "Published release is $($published.tag_name), expected v$version" }
if ($ReleaseTag -and $published.tag_name -ne $ReleaseTag) { throw 'Requested release tag differs from package version' }
if (@($published.assets | Where-Object name -eq 'debugtui-cli.tgz').Count -ne 1) { throw 'Published release is missing the fixed npm asset' }
$download = Join-Path $runRoot 'debugtui-cli.tgz'
Invoke-WebRequest -Uri $url -OutFile $download
if ((Get-FileHash -LiteralPath $download).Hash -ne (Get-FileHash -LiteralPath "$projectRoot\artifacts\debugtui-cli.tgz").Hash) { throw 'Public download differs from local release package' }

$config = Join-Path $runRoot 'debug.toml'
"version=2`nwatch=['keep_this']" | Set-Content -LiteralPath $config -Encoding utf8
$configHash = (Get-FileHash -LiteralPath $config).Hash
if ($PreviousVersion) {
    $oldUrl = "https://github.com/$Repository/releases/download/v$PreviousVersion/debugtui-cli-$PreviousVersion.tgz"
    & npm.cmd install --global --prefix $prefix --prefer-online --ignore-scripts=false --foreground-scripts --no-audit --no-fund $oldUrl
    if ($LASTEXITCODE -ne 0) { throw 'Previous public release installation failed' }
    if ((& "$prefix\debugtui.cmd" --version) -ne "debugtui $PreviousVersion") { throw 'Previous public version mismatch' }
}
$customerDirectory = Join-Path $userDirectory 'profiles/chips'
New-Item -ItemType Directory -Path $customerDirectory -Force | Out-Null
"# Customer-owned override; preserve even when not selected.`n" | Set-Content -LiteralPath (Join-Path $customerDirectory 'customer.toml') -Encoding utf8
$preserved = @{}
Get-ChildItem -LiteralPath $userDirectory -File -Recurse | ForEach-Object { $preserved[$_.FullName] = (Get-FileHash -LiteralPath $_.FullName).Hash }
for ($attempt = 1; $attempt -le 2; $attempt++) {
    & npm.cmd install --global --prefix $prefix --prefer-online --ignore-scripts=false --foreground-scripts --no-audit --no-fund $url
    if ($LASTEXITCODE -ne 0) { throw 'Public release URL installation failed' }
    if ((& "$prefix\debugtui.cmd" --version) -ne "debugtui $version") { throw 'Installed public version mismatch' }
    if ((& "$prefix\debugtui.ps1" --version) -ne "debugtui $version") { throw 'Installed PowerShell entry mismatch' }
    if ((Get-FileHash -LiteralPath $config).Hash -ne $configHash) { throw 'Upgrade changed project configuration' }
    foreach ($file in $preserved.Keys) {
        if ((Get-FileHash -LiteralPath $file).Hash -ne $preserved[$file]) { throw "Upgrade changed customer configuration: $file" }
    }
    if (-not (Test-Path -LiteralPath (Join-Path $userDirectory 'profiles/devices.toml'))) { throw 'Real postinstall did not initialize the catalogue' }
}
$installedRoot = Join-Path "$prefix\node_modules" $metadata.name
if (-not (Test-Path -LiteralPath "$installedRoot\tools\bin\openocd\bin\openocd.exe")) { throw 'Bundled OpenOCD is missing after upgrade' }
if ((Get-FileHash -LiteralPath "$installedRoot\bin\debugtui.exe").Hash -ne (Get-FileHash -LiteralPath "$projectRoot\bin\debugtui.exe").Hash) { throw 'Installed public binary differs from tested binary' }
& "$prefix\debugtui.cmd" --snapshot "$runRoot\public-ui.txt"
if ($LASTEXITCODE -ne 0) { throw 'Public package renderer failed' }
& npm.cmd uninstall --global --prefix $prefix --ignore-scripts --no-audit --no-fund $metadata.name
if ($LASTEXITCODE -ne 0 -or (Test-Path "$prefix\debugtui.cmd")) { throw 'Public package uninstall failed' }
foreach ($file in $preserved.Keys) {
    if ((Get-FileHash -LiteralPath $file).Hash -ne $preserved[$file]) { throw "Uninstall changed customer configuration: $file" }
}
@{version=$version;previousVersion=$PreviousVersion;tag=$published.tag_name;prerelease=$published.prerelease;url=$url;artifacts=$runRoot;sha256=(Get-FileHash -LiteralPath $download).Hash;lifecycleScriptsExecuted=$true;preservedCustomerFiles=$preserved.Count} | ConvertTo-Json | Set-Content "$runRoot\result.json" -Encoding utf8
Write-Output "PASS public release $($published.tag_name): checksum, install/upgrade, repeated install, CMD/PowerShell, renderer, preserved project config, bundled tools, uninstall. Artifacts: $runRoot"
} finally { $env:DEBUGTUI_CONFIG_DIR = $oldConfigDirectory }
