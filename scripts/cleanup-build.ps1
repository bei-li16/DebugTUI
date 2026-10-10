param([switch]$Apply)
$ErrorActionPreference = 'Stop'
$projectRoot = [IO.Path]::GetFullPath((Split-Path $PSScriptRoot))
$target = [IO.Path]::GetFullPath((Join-Path $projectRoot 'target'))
if ((Split-Path $target -Parent) -ne $projectRoot -or (Split-Path $target -Leaf) -ne 'target') {
    throw 'Unexpected build output directory'
}
if (-not (Test-Path -LiteralPath $target)) { Write-Output 'No Cargo build outputs'; return }
$ancestor = Get-Item -LiteralPath $target -Force
while ($ancestor) {
    if ($ancestor.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Linked build directory refused' }
    $ancestor = $ancestor.Parent
}
if (Get-ChildItem -LiteralPath $target -Recurse -Force | Where-Object { $_.Attributes -band [IO.FileAttributes]::ReparsePoint }) {
    throw 'Linked files in build directory refused'
}
$bytes = (Get-ChildItem -LiteralPath $target -File -Recurse -Force | Measure-Object Length -Sum).Sum
Write-Output ("Cargo outputs: {0:N2} GiB. Source, tools, sessions and artifacts are retained." -f ($bytes/1GB))
if (-not $Apply) { Write-Output 'Preview only. Run with -Apply after verification and packaging finish.'; return }
if (Get-Process cargo,rustc,debugtui,openocd,arm-none-eabi-gdb -ErrorAction SilentlyContinue) {
    throw 'Build/debug processes are still running; retry after they finish'
}
$release = Join-Path $target 'release/debugtui.exe'
if (Test-Path -LiteralPath $release) {
    $preserved = Join-Path $projectRoot 'bin/debugtui.exe'
    if (-not (Test-Path -LiteralPath $preserved) -or
        (Get-FileHash -LiteralPath $release).Hash -ne (Get-FileHash -LiteralPath $preserved).Hash) {
        throw 'Package the tested release EXE into bin before cleaning build outputs'
    }
}
$cargo = if (Test-Path -LiteralPath (Join-Path $projectRoot '.dev/cargo/bin/cargo.exe')) {
    $env:CARGO_HOME = Join-Path $projectRoot '.dev/cargo'
    $env:RUSTUP_HOME = Join-Path $projectRoot '.dev/rustup'
    Join-Path $projectRoot '.dev/cargo/bin/cargo.exe'
} else { (Get-Command cargo -ErrorAction Stop).Source }
Push-Location $projectRoot
try {
    & $cargo clean --target-dir $target
    if ($LASTEXITCODE -ne 0) { throw "Cargo cleanup failed: $LASTEXITCODE" }
} finally { Pop-Location }
