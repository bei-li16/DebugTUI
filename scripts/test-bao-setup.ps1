param(
    [Parameter(Mandatory=$true)][string]$ProjectRoot,
    [string]$Binary=(Join-Path (Split-Path $PSScriptRoot) 'target/release/debugtui.exe')
)
$ErrorActionPreference='Stop'
$repo=Split-Path $PSScriptRoot
$ProjectRoot=(Resolve-Path -LiteralPath $ProjectRoot).Path.Replace('\','/')
$Binary=(Resolve-Path -LiteralPath $Binary).Path
$out=Join-Path $repo ('artifacts/bao-setup-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $out | Out-Null
$text=[IO.File]::ReadAllText("$ProjectRoot/debug.toml")
$text=$text.Replace('profile = "scripts/tha6206/debug-env.toml"',"profile = `"$ProjectRoot/scripts/tha6206/debug-env.toml`"")
$text=$text.Replace('elf = "bin/tha6xxx/tha6206-smoke/bao.elf"',"elf = `"$ProjectRoot/bin/tha6xxx/tha6206-smoke/bao.elf`"")
$text=$text.Replace('source_root = "."',"source_root = `"$ProjectRoot`"")
$text=$text.Replace('svd = "../THA6XXX_MC_AS440/.vscode/THA6206/tha6206.svd"',"svd = `"$ProjectRoot/../THA6XXX_MC_AS440/.vscode/THA6206/tha6206.svd`"")
# The user's current core selection is not a test fixture default.
$text=[regex]::Replace($text,'(?ms)(^\[debug\]\r?\n(?:(?!^\[).)*?^cores\s*=\s*)\[[^\]]*\]','${1}[0]')
[IO.File]::WriteAllText((Join-Path $out 'debug.toml'),$text)
Add-Type -Path (Join-Path $repo 'tests/conpty.cs')
$terminal=$null
$esc=[char]27
function Wait-Screen([string]$Pattern) {
    $deadline=[datetime]::UtcNow.AddSeconds(20)
    do {
        $screen=$terminal.Screen()
        if ($screen -match $Pattern) { return $screen }
        Start-Sleep -Milliseconds 50
    } while ([datetime]::UtcNow -lt $deadline)
    $screen | Set-Content -LiteralPath (Join-Path $out 'failure.screen.txt')
    throw "Missing screen: $Pattern"
}
try {
    # No arguments, as in the user's screenshot. Never press Start in this suite.
    $terminal=[DebugTuiTerminal]::new($Binary,'',$out,120,36)
    Wait-Screen 'Cores: core\.0' | Out-Null
    Wait-Screen 'Debug cores\s+\[0\]' | Out-Null
    Wait-Screen '\u2502Save config.*Ctrl\+S\u2502' | Out-Null
    if ($terminal.Screen() -match 'Save to project') { throw 'Removed save toggle is still visible' }
    $terminal.Screen() | Set-Content -LiteralPath (Join-Path $out 'core0.screen.txt')
    $terminal.Send(("`t"*3)+"`r") # Project -> Debug cores
    Wait-Screen 'Debug cores: tha6206' | Out-Null
    $terminal.Send('n'+"${esc}[B"+' '+"`r")
    Wait-Screen 'Debug cores\s+\[1\]' | Out-Null
    $terminal.Send([string][char]19)
    Wait-Screen 'Saved ' | Out-Null
    $terminal.Screen() | Set-Content -LiteralPath (Join-Path $out 'core1.screen.txt')
    $terminal.Send("`r"+'a'+"`r")
    Wait-Screen 'Debug cores\s+\[0, 1\]' | Out-Null
    $terminal.Send([string][char]19)
    Wait-Screen 'Saved ' | Out-Null
    $terminal.Screen() | Set-Content -LiteralPath (Join-Path $out 'both.screen.txt')
    if ($terminal.Screen() -match 'requires backend|Check Tools / profile|Error:') { throw 'Setup still rejects Bao tools profile' }
    $terminal.Send([string][char]17)
    if (-not $terminal.WaitForExit(10000) -or $terminal.ExitCode() -ne 0) { throw 'Exit failed' }
    @{passed=$true;project=$ProjectRoot;binary=$Binary;scope='no-argument Setup, core0/core1/dual selection and save; no board connection'} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $out 'report.json')
    Write-Output "PASS Bao Setup and all core selections: $out"
} finally {
    if ($terminal) { $terminal.Send([string][char]17); $null=$terminal.WaitForExit(3000); $terminal.Dispose() }
}
