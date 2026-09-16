$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path $PSScriptRoot
if (Test-Path "$projectRoot\.dev\cargo\bin\cargo.exe") {
    $env:CARGO_HOME = "$projectRoot\.dev\cargo"
    $env:RUSTUP_HOME = "$projectRoot\.dev\rustup"
    $env:PATH = "$env:CARGO_HOME\bin;$env:PATH"
}
Push-Location $projectRoot
try {
    $metadata = & cargo metadata --format-version 1 --locked --offline --filter-platform x86_64-pc-windows-gnu | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0) { throw 'cargo metadata failed; build first' }
    $ids = @($metadata.resolve.nodes | ForEach-Object id)
    $packages = @($metadata.packages | Where-Object { $_.id -in $ids -and $_.name -ne 'debugtui' } | Sort-Object name,version)
    $text = [Text.StringBuilder]::new()
    $null = $text.AppendLine("# Third-party notices`n`nLicense inventory for the locked Windows GNU Cargo dependency graph, including build-time dependencies. Original notices are reproduced below. Rust standard-library notices are in licenses/rust/. Optional GDB and probe servers are distributed separately with their environment notices.`n")
    foreach ($package in $packages) {
        $null = $text.AppendLine("## $($package.name) $($package.version)`n`nDeclared license: $($package.license)`nSource: $($package.repository)`n")
        $crateRoot = Split-Path $package.manifest_path
        $licenseFiles = @(Get-ChildItem -LiteralPath $crateRoot -File | Where-Object Name -match '^(LICENSE|LICENCE|COPYING|COPYRIGHT)([-_.]|$)')
        if ($package.license_file) { $licenseFiles += Get-Item -LiteralPath (Join-Path $crateRoot $package.license_file) }
        if (-not $licenseFiles.Count -and $package.name -eq 'winapi-x86_64-pc-windows-gnu') {
            # This import-library subcrate omits license files; use its same-repository parent crate.
            $parent = $packages | Where-Object { $_.name -eq 'winapi' -and $_.repository -eq $package.repository } | Select-Object -First 1
            if ($parent) { $licenseFiles = @(Get-ChildItem -LiteralPath (Split-Path $parent.manifest_path) -File -Filter 'LICENSE-*') }
        }
        if (-not $licenseFiles.Count) { throw "No license text for $($package.name)" }
        foreach ($file in $licenseFiles | Sort-Object FullName -Unique) {
            $null = $text.AppendLine("### $($file.Name)`n")
            $null = $text.AppendLine([IO.File]::ReadAllText($file.FullName))
        }
    }
    [IO.File]::WriteAllText("$projectRoot\THIRD_PARTY_NOTICES.md", $text.ToString().Replace("`r`n","`n"))
    $sysroot = & rustc --print sysroot
    $rustDocs = Join-Path $sysroot 'share\doc\rust'
    New-Item -ItemType Directory -Path "$projectRoot\licenses\rust\licenses" -Force | Out-Null
    Copy-Item -LiteralPath "$rustDocs\COPYRIGHT-library.html" -Destination "$projectRoot\licenses\rust\COPYRIGHT-library.html" -Force
    Get-ChildItem -LiteralPath "$rustDocs\licenses" -File | Copy-Item -Destination "$projectRoot\licenses\rust\licenses" -Force
    Write-Output "Generated notices for $($packages.Count) locked crates and the Rust standard library."
} finally { Pop-Location }
