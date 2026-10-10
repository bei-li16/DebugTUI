$ErrorActionPreference = 'Stop'
$repositoryRoot = Split-Path $PSScriptRoot
$out = Join-Path $repositoryRoot ('artifacts/cleanup-tests-storage-' + [Guid]::NewGuid().ToString('N'))
$helper = Join-Path $PSScriptRoot 'cleanup-test-artifacts.ps1'
New-Item -ItemType Directory -Path $out -Force | Out-Null
$results = [Collections.Generic.List[string]]::new()
$body = '# generated catalogue ' + ('abc123' * 180000)
function Check([bool]$Condition, [string]$Message) { if (-not $Condition) { throw $Message } }
function Fixture([string]$Name, [bool]$Passed=$true, [bool]$Hardware=$false, [int]$Owner=0) {
    $directory = Join-Path $out $Name
    New-Item -ItemType Directory -Path $directory -Force | Out-Null
    [IO.File]::WriteAllText((Join-Path $directory 'a.toml'), $body)
    [IO.File]::WriteAllText((Join-Path $directory 'b.toml'), $body)
    [IO.File]::WriteAllText((Join-Path $directory 'unique.toml'), $body + 'unique')
    Set-Content -LiteralPath (Join-Path $directory 'debug.toml') -Value 'version=2'
    Set-Content -LiteralPath (Join-Path $directory 'raw-board.json') -Value '{"sample":42}'
    @{schema=1;pid=$Owner;state='completed'} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $directory '.test-run.json')
    @{passed=$Passed;board_tests_executed=$Hardware;counts=@{passed=if($Passed){1}else{0};failed=if($Passed){0}else{1};skipped=0};cases=@(@{status=if($Passed){'passed'}else{'failed'}})} |
        ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $directory 'report.json')
    return $directory
}
function Invoke-Storage([string]$Storage, [string[]]$Arguments, [bool]$ShouldPass=$true) {
    $messages = & pwsh -NoProfile -File $helper -Root $Storage @Arguments 2>&1
    $code = $LASTEXITCODE
    $messages | Out-String | Add-Content -LiteralPath (Join-Path $out 'process.log')
    Check (($code -eq 0) -eq $ShouldPass) "Unexpected exit code $code : $messages"
    if ($ShouldPass -and $Arguments -notcontains '-RestorePath') { return ($messages -join "`n" | ConvertFrom-Json) }
}
try {
    $previewRoot = Join-Path $out 'preview'; New-Item -ItemType Directory -Path $previewRoot | Out-Null
    # Move only freshly generated fixtures after checking their destination stays in this test root.
    $run = Fixture 'preview-run'
    $destination = Join-Path $previewRoot 'run'
    Check ([IO.Path]::GetFullPath($run).StartsWith($out + '\') -and [IO.Path]::GetFullPath($destination).StartsWith($out + '\')) 'Fixture move escapes test root'
    Move-Item -LiteralPath $run -Destination $destination
    $plan = Invoke-Storage $previewRoot @()
    Check ($plan.planned -eq 1 -and (Test-Path -LiteralPath (Join-Path $destination 'b.toml'))) 'Preview changed payloads or missed duplicate'
    Check (-not (Test-Path -LiteralPath (Join-Path $previewRoot '.fixture-store'))) 'Preview created an archive store'
    $results.Add('Preview is read-only and only selects duplicates')
    $protected = @('debug.toml','raw-board.json','report.json','unique.toml') | ForEach-Object { @{path=(Join-Path $destination $_);sha=(Get-FileHash -LiteralPath (Join-Path $destination $_)).Hash} }
    $originalHash = (Get-FileHash -LiteralPath (Join-Path $destination 'b.toml')).Hash
    $clean = Invoke-Storage $previewRoot @('-Apply')
    Check ($clean.removed -eq 1 -and -not (Test-Path -LiteralPath (Join-Path $destination 'b.toml'))) 'Duplicate not removed'
    Check (Test-Path -LiteralPath (Join-Path $destination 'a.toml')) 'Canonical original removed'
    foreach ($entry in $protected) { Check ((Get-FileHash -LiteralPath $entry.path).Hash -eq $entry.sha) 'Unique configuration/report/board evidence modified' }
    $results.Add('Deduplication retains one original, unique configs, reports and raw captures')
    $null = Invoke-Storage $previewRoot @('-RestorePath',(Join-Path $destination 'b.toml'))
    Check ((Get-FileHash -LiteralPath (Join-Path $destination 'b.toml')).Hash -eq $originalHash) 'Restored bytes differ'
    $null = Invoke-Storage $previewRoot @('-RestorePath',(Join-Path $destination 'b.toml')) $false
    $results.Add('SHA256-verified restoration is exact and never overwrites')
    $null = Invoke-Storage $previewRoot @('-Apply')
    $again = Invoke-Storage $previewRoot @('-Apply')
    Check ($again.removed -eq 0) 'Repeated cleanup is not idempotent'
    $results.Add('Repeated cleanup is idempotent')
    $failed = Fixture 'failed' $false
    $null = Invoke-Storage $out @('-RunDirectory',$failed,'-Apply') $false
    Check (Test-Path -LiteralPath (Join-Path $failed 'b.toml')) 'Failed fixture removed'
    $hardware = Fixture 'hardware' $true $true
    $null = Invoke-Storage $out @('-RunDirectory',$hardware,'-Apply') $false
    Check (Test-Path -LiteralPath (Join-Path $hardware 'b.toml')) 'Hardware configuration removed'
    $results.Add('Automatic cleanup refuses failed and hardware runs')
    $active = Fixture 'active-lease' $true $false $PID
    $result = Invoke-Storage $out @('-RunDirectory',$active,'-Apply')
    Check ($result.removed -eq 0 -and $result.skipped.Count -eq 1) 'Live test lease ignored'
    $results.Add('Live owner lease prevents cleanup even with a passing report')
    $unfinished = Fixture 'unfinished'
    @{schema=1;pid=0;state='running'} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $unfinished '.test-run.json')
    $result = Invoke-Storage $out @('-RunDirectory',$unfinished,'-Apply') $false
    Check (Test-Path -LiteralPath (Join-Path $unfinished 'b.toml')) 'Unfinished fixture removed'
    $results.Add('Unfinished lease retained even if its owner has exited')
    $progressRun = Fixture 'ongoing-audit'
    @{status='in_progress'} | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $progressRun 'progress.json')
    $result = Invoke-Storage $out @('-RunDirectory',$progressRun,'-Apply')
    Check ($result.removed -eq 0 -and $result.skipped.Count -eq 1) 'Ongoing audit without a debugger process was cleaned'
    $results.Add('In-progress audit is preserved without requiring a Cargo/debugger process')
    $activeChild = Fixture 'active-child'
    $child = Start-Process -FilePath (Get-Command node).Source -ArgumentList @('-e','"setTimeout(() => {}, 30000)"',$activeChild) -WindowStyle Hidden -PassThru
    try {
        $result = Invoke-Storage $out @('-RunDirectory',$activeChild,'-Apply')
        Check ($result.removed -eq 0 -and $result.skipped.Count -eq 1) 'Child still using run was ignored'
        Check (Test-Path -LiteralPath (Join-Path $activeChild 'b.toml')) 'Active child payload deleted'
    } finally { if (-not $child.HasExited) { Stop-Process -Id $child.Id; $child.WaitForExit() } }
    $results.Add('Passing report cannot override a live child using the directory')
    $linked = Fixture 'linked'
    $junction = Join-Path $linked 'link'
    New-Item -ItemType Junction -Path $junction -Target $destination | Out-Null
    try {
        $result = Invoke-Storage $out @('-RunDirectory',$linked,'-Apply')
        Check ($result.removed -eq 0 -and $result.skipped.Count -eq 1) 'Nested junction was followed'
        Check (Test-Path -LiteralPath (Join-Path $linked 'b.toml')) 'Linked run was partially cleaned'
    } finally { Remove-Item -LiteralPath $junction -Force }
    $results.Add('Nested junction skips the entire run before mutation')
    $null = Invoke-Storage $out @('-RestorePath',(Join-Path (Split-Path $out) 'outside.toml')) $false
    $null = Invoke-Storage $out @('-RunDirectory',$destination,'-Apply') $false
    $results.Add('Out-of-root restore and nested/unowned automatic runs refused')
    $formal = Join-Path $previewRoot 'debugtui-official.exe'
    [IO.File]::WriteAllText($formal, $body)
    $formalHash = (Get-FileHash -LiteralPath $formal).Hash
    $null = Invoke-Storage $previewRoot @('-Apply')
    Check ((Get-FileHash -LiteralPath $formal).Hash -eq $formalHash) 'Root release attachment removed'
    $results.Add('Formal root attachments are retained even when they match test copies')
    $blob = Get-ChildItem -LiteralPath (Join-Path $previewRoot '.fixture-store') -Filter '*.gz' | Select-Object -First 1
    [IO.File]::WriteAllText($blob.FullName, 'corrupt archive')
    [IO.File]::WriteAllText((Join-Path $destination 'b.toml'), $body)
    $null = Invoke-Storage $previewRoot @('-Apply') $false
    Check (Test-Path -LiteralPath (Join-Path $destination 'b.toml')) 'Corrupt archive caused original deletion'
    $results.Add('Corrupt archive retains original payloads and fails visibly')
    @{passed=$true;count=$results.Count;cases=@($results.ToArray())} | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $out 'report.json')
    Write-Output "PASS artifact storage: $($results.Count)/$($results.Count); $out"
} catch {
    @{passed=$false;cases=@($results.ToArray());error=$_.ToString()} | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $out 'report.json')
    throw
}
