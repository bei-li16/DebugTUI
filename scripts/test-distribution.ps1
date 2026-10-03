param([string]$Binary = (Join-Path (Split-Path $PSScriptRoot) 'target/debug/debugtui.exe'))
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot
$Binary = (Resolve-Path -LiteralPath $Binary).Path
$out = Join-Path $root ('artifacts/distribution-' + (Get-Date -Format yyyyMMdd-HHmmss) + '-' + [guid]::NewGuid().ToString('N').Substring(0,8))
$stage = Join-Path $out 'stage'
$prefix = Join-Path $out 'prefix'
$fixture = Join-Path $out 'old-version-fixture'
New-Item -ItemType Directory -Path "$stage/bin", "$fixture/bin", $prefix -Force | Out-Null
$metadata = Get-Content -LiteralPath (Join-Path $root 'package.json') -Raw | ConvertFrom-Json
$cases = [System.Collections.Generic.List[object]]::new()
$oldDevicesConfig=$env:DEBUGTUI_CONFIG_DIR
$env:DEBUGTUI_CONFIG_DIR=Join-Path $out 'user-profile'
$deviceCatalogue=Join-Path $env:DEBUGTUI_CONFIG_DIR 'profiles/devices.toml'
$expectedHash = (Get-FileHash -LiteralPath $Binary).Hash
function Invoke-Npm([string[]]$NpmArguments) {
    # Windows PowerShell treats native stderr warnings as terminating errors
    # under Stop. Preserve npm diagnostics and decide success by its exit code.
    $previousPreference = $ErrorActionPreference
    try {
        $ErrorActionPreference = 'Continue'
        $output = & npm.cmd @NpmArguments 2>&1
        $exitCode = $LASTEXITCODE
    } finally { $ErrorActionPreference = $previousPreference }
    if ($exitCode -ne 0) { throw "npm failed (exit $exitCode): $output" }
    return $output
}
function Test-Case([string]$Id, [string]$Description, [scriptblock]$Body) {
    try { & $Body | Out-Null; $cases.Add(@{id=$Id;description=$Description;status='passed'}); Write-Output "PASS ${Id}: $Description" }
    catch { $cases.Add(@{id=$Id;description=$Description;status='failed';error=$_.Exception.Message}); throw }
}
try {
    Test-Case 'PKG-01' 'Package contains the tested EXE and notices, with no bundled debugger tools' {
        Copy-Item -LiteralPath $Binary -Destination "$stage/bin/debugtui.exe"
        Copy-Item -LiteralPath (Join-Path $root 'package.json') -Destination "$stage/package.json"
        foreach ($entry in $metadata.files) {
            if ($entry -eq 'bin/') { continue }
            $source = Join-Path $root $entry
            if (Test-Path -LiteralPath $source) { Copy-Item -LiteralPath $source -Destination $stage -Recurse }
        }
        $packed = (Invoke-Npm @('pack', $stage, '--json', '--pack-destination', $out)) -join "`n" | ConvertFrom-Json
        $script:packageFile = Join-Path $out $packed[0].filename
        $files = @($packed[0].files | ForEach-Object path)
        if ($files -notcontains 'bin/debugtui.exe' -or $files -notcontains 'LICENSE' -or $files -notcontains 'THIRD_PARTY_NOTICES.md') { throw 'Required executable/license files missing' }
        if ($files -match '^(tools|tests|scripts|target|\.dev)/') { throw 'Development/environment files leaked into package' }
    }
    Test-Case 'PKG-02' 'Install an old package fixture inside a private prefix' {
        Copy-Item -LiteralPath $Binary -Destination "$fixture/bin/debugtui.exe"
        @{name=$metadata.name;version='0.0.0-fixture';bin=@{debugtui='bin/debugtui.exe'};files=@('bin/')} | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath "$fixture/package.json" -Encoding utf8
        $old = (Invoke-Npm @('pack',$fixture,'--json','--pack-destination',$out)) -join "`n" | ConvertFrom-Json
        Invoke-Npm @('install','--global','--prefix',$prefix,'--no-audit','--no-fund',(Join-Path $out $old[0].filename))
        $script:installedRoot = Join-Path "$prefix/node_modules" $metadata.name
        if ((Get-Content -LiteralPath "$installedRoot/package.json" -Raw | ConvertFrom-Json).version -ne '0.0.0-fixture') { throw 'Old fixture was not installed' }
    }
    $userConfig = Join-Path $out 'user-project.toml'
    "version=2`nwatch=['keep_this']`n" | Set-Content -LiteralPath $userConfig -Encoding utf8
    $configHash = (Get-FileHash -LiteralPath $userConfig).Hash
    Test-Case 'PKG-03' 'Upgrade preserves user config and installs the exact tested binary' {
        Invoke-Npm @('install','--global','--prefix',$prefix,'--no-audit','--no-fund',$packageFile)
        if ((Get-Content -LiteralPath "$installedRoot/package.json" -Raw | ConvertFrom-Json).version -ne $metadata.version) { throw 'Upgrade version mismatch' }
        if ((Get-FileHash -LiteralPath "$installedRoot/bin/debugtui.exe").Hash -ne $expectedHash) { throw 'Installed EXE differs from tested EXE' }
        if ((Get-FileHash -LiteralPath $userConfig).Hash -ne $configHash) { throw 'User config was changed' }
    }
    if (-not (Test-Path -LiteralPath $deviceCatalogue)) { throw 'Install hook did not create device catalogue' }
    Add-Content -LiteralPath $deviceCatalogue -Value "`n[devices.s32k144]`ncores=[0]`nbackend='generic'"
    $deviceHash=(Get-FileHash -LiteralPath $deviceCatalogue).Hash
    Test-Case 'PKG-04' 'CMD/PowerShell entry points and installed snapshot renderer work' {
        foreach ($entry in @('debugtui.cmd','debugtui.ps1')) {
            if ((& "$prefix/$entry" --version) -ne "debugtui $($metadata.version)" -or $LASTEXITCODE -ne 0) { throw "Entry point failed: $entry" }
        }
        & "$prefix/debugtui.cmd" --snapshot "$out/installed-ui.txt"
        if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath "$out/installed-ui.txt")) { throw 'Installed renderer failed' }
    }
    Test-Case 'PKG-05' 'Repeated install is idempotent and retains the tool separation boundary' {
        Invoke-Npm @('install','--global','--prefix',$prefix,'--no-audit','--no-fund',$packageFile)
        if ((Get-FileHash -LiteralPath $deviceCatalogue).Hash -ne $deviceHash) { throw 'Upgrade changed customer chip catalogue' }
        if (Test-Path -LiteralPath "$installedRoot/tools") { throw 'Bundled tools were installed' }
        if ((Get-FileHash -LiteralPath "$installedRoot/bin/debugtui.exe").Hash -ne $expectedHash) { throw 'Repeated install changed executable' }
    }
    Test-Case 'PKG-06' 'Private-prefix uninstall removes launcher but preserves user config' {
        Invoke-Npm @('uninstall','--global','--prefix',$prefix,'--no-audit','--no-fund',$metadata.name)
        if (Test-Path -LiteralPath "$prefix/debugtui.cmd") { throw 'Launcher survived uninstall' }
        if ((Get-FileHash -LiteralPath $userConfig).Hash -ne $configHash) { throw 'Uninstall changed user config' }
    }
    Test-Case 'PKG-07' 'Clean reinstall succeeds with the same binary hash' {
        Invoke-Npm @('install','--global','--prefix',$prefix,'--no-audit','--no-fund',$packageFile)
        if ((Get-FileHash -LiteralPath "$installedRoot/bin/debugtui.exe").Hash -ne $expectedHash) { throw 'Reinstall hash mismatch' }
    }
} finally {
    $env:DEBUGTUI_CONFIG_DIR=$oldDevicesConfig
    $failed = @($cases | Where-Object status -eq failed).Count
    $passed = @($cases | Where-Object status -eq passed).Count
    @{binary=$Binary;sha256=$expectedHash;layer='isolated npm distribution';counts=@{passed=$passed;failed=$failed;skipped=(7-$cases.Count)};passed=($passed -eq 7);cases=@($cases.ToArray());prefix=$prefix;package=$packageFile;scope='Local package lifecycle; does not validate public GitHub release/CDN publishing.'} | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $out 'report.json') -Encoding utf8
    Write-Output "RESULT passed=$passed failed=$failed $out"
}
