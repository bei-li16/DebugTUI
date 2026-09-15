param([switch]$Release,[switch]$Test)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path $PSScriptRoot
Push-Location $projectRoot
try {
    if (Test-Path "$projectRoot\.dev\cargo\bin\cargo.exe") {
        $env:CARGO_HOME = "$projectRoot\.dev\cargo"
        $env:RUSTUP_HOME = "$projectRoot\.dev\rustup"
        $env:PATH = "$env:CARGO_HOME\bin;$env:PATH"
    }
    if ($env:DEBUGTUI_GCC_DIR) { $env:PATH = "$env:DEBUGTUI_GCC_DIR;$env:PATH" }
    $buildArgs = @('build','--locked')
    if ($Test) { $buildArgs[0] = 'test' }
    if ($Release) { $buildArgs += '--release' }
    & cargo @buildArgs
    if ($LASTEXITCODE -ne 0) { throw "cargo exited $LASTEXITCODE" }
} finally { Pop-Location }
