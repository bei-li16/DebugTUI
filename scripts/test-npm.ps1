param([string]$Package)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path $PSScriptRoot
$metadata = Get-Content "$projectRoot\package.json" -Raw | ConvertFrom-Json
$packageName = $metadata.name
$packageVersion = $metadata.version
if (-not $Package) {
    $latest = (Get-Content "$projectRoot\artifacts\npm-pack.json" -Raw | ConvertFrom-Json)[0]
    $Package = Join-Path "$projectRoot\artifacts" $latest.filename
}
$Package = (Resolve-Path -LiteralPath $Package).Path
$runRoot = Join-Path $projectRoot ('artifacts\npm test 工程 ' + (Get-Date -Format yyyyMMdd-HHmmss))
$prefix = Join-Path $runRoot 'prefix'
$fixture = Join-Path $runRoot 'old-version-fixture'
New-Item -ItemType Directory -Path "$fixture\bin",$prefix -Force | Out-Null
Copy-Item -LiteralPath "$projectRoot\bin\debugtui.exe" -Destination "$fixture\bin\debugtui.exe"
# The fixture exercises npm's package replacement, not a historical debugger release.
@{name=$packageName;version='0.0.0-fixture';bin=@{debugtui='bin/debugtui.exe'};files=@('bin/')} | ConvertTo-Json -Depth 4 | Set-Content "$fixture\package.json" -Encoding utf8
$userConfig = Join-Path $runRoot 'debug.toml'
"version = 1`nwatch = ['xTickCount']" | Set-Content $userConfig -Encoding utf8
$configHash = (Get-FileHash -LiteralPath $userConfig).Hash

Push-Location $fixture
try {
    $packed = & npm.cmd pack --json --pack-destination "$runRoot" | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0) { throw 'Fixture packaging failed' }
} finally { Pop-Location }
& npm.cmd install --global --prefix "$prefix" --ignore-scripts --no-audit --no-fund (Join-Path $runRoot $packed.filename)
if ($LASTEXITCODE -ne 0) { throw 'Fixture install failed' }
$installedRoot = Join-Path "$prefix\node_modules" $packageName
if ((Get-Content "$installedRoot\package.json" -Raw | ConvertFrom-Json).version -ne '0.0.0-fixture') { throw 'Fixture version mismatch' }

& npm.cmd install --global --prefix "$prefix" --ignore-scripts --no-audit --no-fund "$Package"
if ($LASTEXITCODE -ne 0) { throw 'Package upgrade failed' }
$installed = Get-Content "$installedRoot\package.json" -Raw | ConvertFrom-Json
if ($installed.version -ne $packageVersion) { throw 'Upgraded version mismatch' }
$version = & "$prefix\debugtui.cmd" --version
if ($LASTEXITCODE -ne 0 -or $version -ne "debugtui $packageVersion") { throw 'CMD entry failed' }
$psVersion = & "$prefix\debugtui.ps1" --version
if ($LASTEXITCODE -ne 0 -or $psVersion -ne "debugtui $packageVersion") { throw 'PowerShell entry failed' }
if ((Get-FileHash -LiteralPath $userConfig).Hash -ne $configHash) { throw 'Upgrade changed project configuration' }

if (Test-Path -LiteralPath "$installedRoot\tools") { throw 'Standalone package contains tools' }
& "$prefix\debugtui.cmd" --snapshot "$runRoot\installed-ui.txt"
if ($LASTEXITCODE -ne 0 -or -not (Test-Path "$runRoot\installed-ui.txt")) { throw 'Installed renderer failed' }
Write-Output "PASS npm: fixture install, upgrade to $packageName@$packageVersion, CMD/PowerShell entries, unchanged user configuration, no bundled environment, terminal renderer."

# Exercise uninstall only inside this test's dedicated prefix, then restore it for hardware testing.
& npm.cmd uninstall --global --prefix "$prefix" --ignore-scripts --no-audit --no-fund $packageName
if ($LASTEXITCODE -ne 0 -or (Test-Path "$prefix\debugtui.cmd")) { throw 'Isolated uninstall failed' }
& npm.cmd install --global --prefix "$prefix" --ignore-scripts --no-audit --no-fund "$Package"
if ($LASTEXITCODE -ne 0) { throw 'Clean reinstall failed' }
@{ prefix=$prefix; packageRoot=$installedRoot; binary="$installedRoot\bin\debugtui.exe"; config=$userConfig } | ConvertTo-Json | Set-Content "$projectRoot\artifacts\npm-test-latest.json" -Encoding utf8
Write-Output "PASS npm: isolated uninstall and clean reinstall. Artifacts: $runRoot"
