param([switch]$KeepIntermediate)
$ErrorActionPreference = 'Stop'
$docsRoot = [IO.Path]::GetFullPath($PSScriptRoot)
$source = Join-Path $docsRoot 'usermanual.tex'
$buildDir = Join-Path $docsRoot ('.usermanual-build-' + [guid]::NewGuid().ToString('N'))
$null = Get-Command xelatex -ErrorAction Stop
New-Item -ItemType Directory -Path $buildDir | Out-Null
Push-Location $docsRoot
try {
    foreach ($pass in 1..3) {
        $output = & xelatex -interaction=nonstopmode -halt-on-error -file-line-error "-output-directory=$buildDir" $source 2>&1
        $output | Set-Content -LiteralPath (Join-Path $buildDir "pass-$pass.txt") -Encoding utf8
        if ($LASTEXITCODE -ne 0) {
            $output | Select-Object -Last 35 | Write-Output
            throw "XeLaTeX pass $pass failed. Logs retained in $buildDir"
        }
    }
    $log = Get-Content -LiteralPath (Join-Path $buildDir 'usermanual.log') -Raw
    if ($log -match 'undefined references|undefined citations|Missing character:') {
        throw "Unresolved references or missing glyphs. Logs retained in $buildDir"
    }
    Copy-Item -LiteralPath (Join-Path $buildDir 'usermanual.pdf') -Destination (Join-Path $docsRoot 'usermanual.pdf') -Force
    $log -split "`r?`n" | Where-Object { $_ -match 'Overfull|Underfull|Output written' } | Write-Output
    if (-not $KeepIntermediate) {
        $resolvedBuild = (Resolve-Path -LiteralPath $buildDir).Path
        if (-not $resolvedBuild.StartsWith($docsRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
            throw 'Refusing to remove a build directory outside docs.'
        }
        Remove-Item -LiteralPath $resolvedBuild -Recurse -Force
    }
    Write-Output (Join-Path $docsRoot 'usermanual.pdf')
} finally {
    Pop-Location
}
