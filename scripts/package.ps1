param([switch]$SkipBuild)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path $PSScriptRoot
Push-Location $projectRoot
try {
    if (-not $SkipBuild) { & "$PSScriptRoot\build.ps1" -Release }
    New-Item -ItemType Directory -Path bin,artifacts -Force | Out-Null
    Copy-Item -LiteralPath target\release\debugtui.exe -Destination bin\debugtui.exe -Force
    $metadata = Get-Content package.json -Raw | ConvertFrom-Json
    if ((& .\bin\debugtui.exe --version) -ne "debugtui $($metadata.version)") { throw 'npm and native versions differ' }
    $output = & npm.cmd pack --json --pack-destination artifacts
    if ($LASTEXITCODE -ne 0) { throw 'npm pack failed' }
    $package = ($output | ConvertFrom-Json)[0]
    $paths = @($package.files | ForEach-Object path)
    if ($paths -notcontains 'bin/debugtui.exe') { throw 'npm omitted the TUI executable' }
    foreach ($required in @('profiles/install.cjs','profiles/devices.toml','profiles/registers/armv7m-common.toml','profiles/registers/cortex-m3.toml','profiles/registers/cortex-m4.toml','profiles/registers/cortex-m7.toml','profiles/registers/cortex-r52.toml','profiles/registers/cortex-r52+.toml','profiles/registers-readonly-multicore.toml.example','profiles/registers-readonly-r52.toml.example','docs/registers-readonly-guide.md','tests/cases/registers-readonly-release.md')) {
        if ($paths -cnotcontains $required) { throw "Register catalogue/profile asset omitted from npm package: $required" }
    }
    if ($paths | Where-Object { $_ -cne 'tests/README.md' -and $_ -cnotmatch '^tests/cases/[^/]+\.md$' -and $_ -match '(^|/)(tools|\.dev|target|node_modules|tests|scripts)/|\.py$|python' }) { throw 'Environment or development dependencies leaked into npm package' }
    $output | Set-Content artifacts\npm-pack.json -Encoding utf8
    Write-Output "Package: $($package.filename), packed $($package.size) bytes, unpacked $($package.unpackedSize) bytes"
} finally { Pop-Location }
