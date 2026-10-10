param([Parameter(ValueFromRemainingArguments)][string[]]$CargoArguments = @('test','--locked','--offline'))
$ErrorActionPreference = 'Stop'
if (-not $CargoArguments.Count -or $CargoArguments[0] -ne 'test') { throw 'This wrapper only accepts cargo test arguments' }
$repositoryRoot = Split-Path $PSScriptRoot
$cargo = if (Test-Path -LiteralPath (Join-Path $repositoryRoot '.dev/cargo/bin/cargo.exe')) {
    $env:CARGO_HOME = Join-Path $repositoryRoot '.dev/cargo'
    $env:RUSTUP_HOME = Join-Path $repositoryRoot '.dev/rustup'
    Join-Path $repositoryRoot '.dev/cargo/bin/cargo.exe'
} else { (Get-Command cargo -ErrorAction Stop).Source }
Push-Location $repositoryRoot
try {
    $artifactRoot = if ($env:DEBUGTUI_TEST_ARTIFACT_ROOT) { $env:DEBUGTUI_TEST_ARTIFACT_ROOT } else { Join-Path $repositoryRoot 'artifacts' }
    New-Item -ItemType Directory -Path $artifactRoot -Force | Out-Null
    $before = @(Get-ChildItem -LiteralPath $artifactRoot -Directory -Force | ForEach-Object Name)
    & $cargo @CargoArguments
    if ($LASTEXITCODE -ne 0) { throw "Cargo test failed ($LASTEXITCODE); fixtures and build outputs retained for diagnosis" }
    if ($env:DEBUGTUI_KEEP_TEST_PAYLOADS -ne '1') {
        foreach ($run in @(Get-ChildItem -LiteralPath $artifactRoot -Directory -Force | Where-Object { $_.Name -notin $before -and $_.Name -ne '.fixture-store' })) {
            & pwsh -NoProfile -File (Join-Path $PSScriptRoot 'cleanup-test-artifacts.ps1') -Root $artifactRoot -DirectoryName $run.Name -Apply
            if ($LASTEXITCODE -ne 0) { throw 'Artifact cleanup failed; inspect its diagnostics before retrying' }
        }
        & pwsh -NoProfile -File (Join-Path $PSScriptRoot 'cleanup-build.ps1') -Apply
        if ($LASTEXITCODE -ne 0) { throw 'Build cleanup deferred; inspect cleanup-build diagnostics (active process or unrecognized build directory)' }
    }
} finally { Pop-Location }
