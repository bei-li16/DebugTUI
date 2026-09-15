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
    $manifest = Get-Content tools\dependencies.lock.json -Raw | ConvertFrom-Json
    foreach ($entry in $manifest.files.PSObject.Properties) {
        $path = Join-Path "$projectRoot\tools" $entry.Name
        if ((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash -ne $entry.Value.sha256) { throw "Dependency checksum mismatch: $path" }
    }
    $output = & npm.cmd pack --json --pack-destination artifacts
    if ($LASTEXITCODE -ne 0) { throw 'npm pack failed' }
    $package = ($output | ConvertFrom-Json)[0]
    $paths = @($package.files | ForEach-Object path)
    foreach ($entry in $manifest.files.PSObject.Properties) {
        if ($paths -notcontains ('tools/' + $entry.Name)) { throw "npm omitted required file: $($entry.Name)" }
    }
    if ($paths | Where-Object { $_ -match '(^|/)(\.dev|target|node_modules)/|\.py$|python' }) { throw 'Development dependencies leaked into npm package' }
    $output | Set-Content artifacts\npm-pack.json -Encoding utf8
    Write-Output "Package: $($package.filename), packed $($package.size) bytes, unpacked $($package.unpackedSize) bytes"
} finally { Pop-Location }
