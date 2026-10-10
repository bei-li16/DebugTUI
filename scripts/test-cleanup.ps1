$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path $PSScriptRoot
$out = Join-Path $projectRoot ('artifacts/cleanup-tests-' + [Guid]::NewGuid().ToString('N'))
$helper = Join-Path $PSScriptRoot 'test-support/cleanup-artifacts.ps1'
New-Item -ItemType Directory -Path $out -Force | Out-Null
$results = @()
function Fixture([string]$Name, [bool]$Passed = $true) {
    $directory = Join-Path $out $Name
    New-Item -ItemType Directory -Path (Join-Path $directory 'direct EXE'),(Join-Path $directory 'project 工程/artifacts'),(Join-Path $directory 'direct config') -Force | Out-Null
    Set-Content -LiteralPath (Join-Path $directory 'direct EXE/debugtui.exe') -Value 'generated copy'
    Set-Content -LiteralPath (Join-Path $directory 'project 工程/artifacts/npm-pack.json') -Value '{"version":"fixture"}'
    Set-Content -LiteralPath (Join-Path $directory 'direct config/customer.toml') -Value '# retained configuration'
    Set-Content -LiteralPath (Join-Path $directory 'process-1.stdout.txt') -Value 'retained process log'
    $cases = @(1..14 | ForEach-Object { @{status=if($Passed){'passed'}else{'failed'}} })
    @{passed=$Passed;counts=@{passed=if($Passed){14}else{0};failed=if($Passed){0}else{14};skipped=0};cases=$cases} |
        ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $directory 'report.json')
    return $directory
}
function Invoke-Cleanup([string[]]$Arguments, [bool]$ShouldPass) {
    $messages = & pwsh -NoProfile -File $helper @Arguments 2>&1
    $code = $LASTEXITCODE
    $messages | Out-String | Add-Content -LiteralPath (Join-Path $out 'process.log')
    if (($code -eq 0) -ne $ShouldPass) { throw "Unexpected cleanup exit code $code" }
}
function Check([bool]$Condition, [string]$Message) { if (-not $Condition) { throw $Message } }
try {
    $run = Fixture 'register-distribution-1-aaaaaaaa'
    $beforeReport = (Get-FileHash -LiteralPath (Join-Path $run 'report.json')).Hash
    Invoke-Cleanup @('-Root',$run,'-Kind','Distribution') $true
    Check (Test-Path -LiteralPath (Join-Path $run 'direct EXE/debugtui.exe')) 'Preview deleted a payload'
    $results += 'Preview preserves all payloads'
    Invoke-Cleanup @('-Root',$run,'-Kind','Distribution','-Apply') $true
    Check (-not (Test-Path -LiteralPath (Join-Path $run 'direct EXE'))) 'Payload not removed'
    foreach ($retained in @('direct config/customer.toml','process-1.stdout.txt','report.json','packaging-evidence/npm-pack.json','cleanup.json')) {
        Check (Test-Path -LiteralPath (Join-Path $run $retained)) "Missing evidence/config: $retained"
    }
    Check ((Get-FileHash -LiteralPath (Join-Path $run 'report.json')).Hash -eq $beforeReport) 'Report changed'
    Invoke-Cleanup @('-Root',$run,'-Kind','Distribution','-Apply') $true
    Check (@(Get-Content -LiteralPath (Join-Path $run 'cleanup-history.jsonl')).Count -eq 2) 'Previous cleanup history lost'
    $results += 'Passing run removes only disposable payloads, retains evidence/config and is idempotent'
    $failed = Fixture 'register-distribution-2-bbbbbbbb' $false
    Invoke-Cleanup @('-Root',$failed,'-Kind','Distribution','-Apply') $false
    Check (Test-Path -LiteralPath (Join-Path $failed 'direct EXE/debugtui.exe')) 'Failed-run payload removed'
    $results += 'Failed run refused'
    $wrong = Fixture 'user-project'
    Invoke-Cleanup @('-Root',$wrong,'-Kind','Distribution','-Apply') $false
    Check (Test-Path -LiteralPath (Join-Path $wrong 'direct EXE/debugtui.exe')) 'Arbitrary project removed'
    $results += 'Non-test project refused'
    $outside = Join-Path $out ('release-staging-' + [Guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $outside | Out-Null
    Set-Content -LiteralPath (Join-Path $outside 'keep.txt') -Value 'outside root'
    Invoke-Cleanup @('-Root',$run,'-Kind','Packaging','-PackagingDirectories',$outside,'-Apply') $false
    Check (Test-Path -LiteralPath (Join-Path $outside 'keep.txt')) 'Outside directory removed'
    $results += 'Packaging path outside declared root refused'
    $linked = Fixture 'register-distribution-3-cccccccc'
    $junction = Join-Path $linked 'direct EXE/link'
    New-Item -ItemType Junction -Path $junction -Target $outside | Out-Null
    try {
        Invoke-Cleanup @('-Root',$linked,'-Kind','Distribution','-Apply') $false
        Check (Test-Path -LiteralPath (Join-Path $outside 'keep.txt')) 'Link target modified'
        Check (Test-Path -LiteralPath (Join-Path $linked 'direct EXE/debugtui.exe')) 'Plan was partially applied before link rejection'
    } finally { Remove-Item -LiteralPath $junction -Force }
    $results += 'Nested junction refused before any deletion; external data preserved'
    $active = Fixture 'register-distribution-4-dddddddd'
    $child = Start-Process -FilePath (Get-Command node -ErrorAction Stop).Source -ArgumentList @('-e','"setTimeout(() => {}, 30000)"',$active) -WindowStyle Hidden -PassThru
    try {
        Invoke-Cleanup @('-Root',$active,'-Kind','Distribution','-Apply') $false
        Check (Test-Path -LiteralPath (Join-Path $active 'direct EXE/debugtui.exe')) 'Active-run payload removed'
    } finally { if (-not $child.HasExited) { Stop-Process -Id $child.Id -ErrorAction Stop; $child.WaitForExit() } }
    $results += 'Active process using the run prevents cleanup'
    @{passed=$true;count=$results.Count;cases=$results} | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $out 'report.json')
    Write-Output "PASS cleanup guards: $($results.Count)/$($results.Count); $out"
} catch {
    @{passed=$false;cases=$results;error=$_.ToString()} | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $out 'report.json')
    throw
}
