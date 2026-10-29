param([Parameter(Mandatory)][string]$Binary,[Parameter(Mandatory)][string]$Project)
$ErrorActionPreference='Stop'
$root=Split-Path $PSScriptRoot
$Binary=(Resolve-Path -LiteralPath $Binary).Path
$Project=(Resolve-Path -LiteralPath $Project).Path
$bao=(Split-Path $Project).Replace('\','/')
$out=Join-Path $root ('artifacts/symbol-images-setup-'+[guid]::NewGuid().ToString('N').Substring(0,8))
New-Item -ItemType Directory -Path $out | Out-Null
$text=[IO.File]::ReadAllText($Project)
$text=[regex]::Replace($text,'(?m)^\[images\.sections\]\r?\n(?=\s*(?:\[|$))','')
$text=[regex]::Replace($text,'(?m)^elf = "(?![A-Za-z]:|/)','elf = "'+$bao+'/').Replace('source_root = "."','source_root = "'+$bao+'"')
# Existing advanced fields must survive the UI edit/serialize/reload cycle.
$at=$text.IndexOf('[images.saved_context]')
if ($at -lt 0) { throw 'Requires the Bao/VM reference project with saved_context' }
$text=$text.Insert($at,"[images.sections]`n`".text`" = 0x10020000`n`n")
$fixture=Join-Path $out 'debug.toml';[IO.File]::WriteAllText($fixture,$text,[Text.UTF8Encoding]::new($false))
$userHash=(Get-FileHash -LiteralPath $Project).Hash
Add-Type -Path (Join-Path $root 'tests/conpty.cs')
$terminal=$null;$esc=[char]27;$cases=[Collections.Generic.List[object]]::new()
function Wait-Screen([string]$Pattern) {
    $deadline=[datetime]::UtcNow.AddSeconds(15)
    do { $screen=$terminal.Screen();if ($screen -match $Pattern) {return};Start-Sleep -Milliseconds 50 } while ([datetime]::UtcNow -lt $deadline)
    $screen | Set-Content -LiteralPath (Join-Path $out 'failure.screen.txt')
    throw "Missing terminal state: $Pattern"
}
function Send-Key([string]$Text) { $terminal.Send($Text);Start-Sleep -Milliseconds 200 }
function Open-Images {
    Select-Field 'Symbol images';Send-Key "`r";Wait-Screen 'Symbol images · project draft'
}
function Start-Setup {
    $script:terminal=[DebugTuiTerminal]::new($Binary,('--setup --project "'+$fixture+'"'),$out,120,36)
    Wait-Screen 'Launch setup';Send-Key "${esc}[A";Wait-Screen '› Symbol images';Send-Key "`r";Wait-Screen 'Symbol images · project draft'
}
function Select-Field([string]$Name) {
    $pattern='›\s+(?:\[\s+)?'+[regex]::Escape($Name)+'(?:\s|$)'
    for($i=0;$i -lt 24;$i++) {if($terminal.Screen() -match $pattern) {return};Send-Key "${esc}[B"}
    throw "Could not focus $Name"
}
function Edit-Field([string]$Name,[string]$Value) {
    Select-Field $Name;Send-Key "`r";Wait-Screen ('editing '+[regex]::Escape($Name));Send-Key ([string][char]21+$Value)
}
function Show-Advanced {
    Select-Field 'Advanced settings';Send-Key "`r";Wait-Screen 'Shown · Enter: hide'
}
function Stop-Setup {
    Send-Key ([string][char]17)
    if (-not $terminal.WaitForExit(15000) -or $terminal.ExitCode() -ne 0) {throw 'Setup did not exit cleanly'}
    $terminal.Transcript() | Set-Content -LiteralPath (Join-Path $out ('terminal-'+$cases.Count+'.vt.txt'))
    $terminal.Dispose();$script:terminal=$null
}
function Apply-Save {
    Send-Key ([string][char]19);Wait-Screen 'Saved '
}
function Normal-Path([string]$Path) {
    # Save/reload may canonicalize the same file to a Windows long-path alias.
    if ($Path.StartsWith('\\?\UNC\',[StringComparison]::OrdinalIgnoreCase)) {$Path='\\'+$Path.Substring(8)}
    elseif ($Path.StartsWith('\\?\',[StringComparison]::OrdinalIgnoreCase)) {$Path=$Path.Substring(4)}
    return [IO.Path]::GetFullPath($Path)
}
function Inventory([string]$Name) {
    $commands=Join-Path $out ($Name+'.jsonl')
    [IO.File]::WriteAllText($commands,"{`"id`":1,`"method`":`"images`"}`n{`"id`":2,`"method`":`"quit`"}`n",[Text.UTF8Encoding]::new($false))
    $lines=& $Binary --project $fixture --script $commands
    if ($LASTEXITCODE -ne 0) {throw 'Saved project could not be loaded'}
    $lines | Set-Content -LiteralPath (Join-Path $out ($Name+'-responses.jsonl'))
    $response=@($lines | ForEach-Object {$_ | ConvertFrom-Json} | Where-Object {$_.event -eq 'response' -and $_.id -eq 1})
    if ($response.Count -ne 1 -or -not $response[0].ok -or $response[0].result.target_io) {throw 'Image inventory failed or touched the target'}
    return $response[0].result.configured
}
function Test-Case([string]$Id,[string]$Description,[scriptblock]$Body) {
    try {& $Body | Out-Null;if ($terminal) {$terminal.Screen() | Set-Content -LiteralPath (Join-Path $out ($Id+'.screen.txt'))};$cases.Add(@{id=$Id;description=$Description;status='passed'});Write-Output "PASS ${Id}: $Description"}
    catch {$cases.Add(@{id=$Id;description=$Description;status='failed';error=$_.Exception.Message});throw}
}
try {
    $before=@(Inventory 'before')
    Test-Case 'IMAGE-EDIT-OPEN' 'Open the real Setup image dialog and choose VM0' {
        Start-Setup;Send-Key "${esc}[C";Send-Key "${esc}[B";Wait-Screen '› Id.*vm0'
        if($terminal.Screen() -match 'Source root|Section offset|Global memory channel') {throw 'Advanced fields were expanded by default'}
    }
    Test-Case 'IMAGE-EDIT-ID' 'Enter visibly edits text, Escape discards, and Enter accepts the new id' {
        Send-Key "`r";Wait-Screen 'editing Id';Wait-Screen 'vm0▏';Wait-Screen 'Editing · Enter: accept'
        Send-Key ([string][char]21+'vm0_discard');Wait-Screen 'vm0_discard▏';Send-Key ([string]$esc);Wait-Screen '› Id\s+vm0\s'
        Send-Key "`r";Send-Key ([string][char]21+'vm0_changed');Send-Key "`r";Wait-Screen '› Id\s+vm0_changed'
    }
    Test-Case 'IMAGE-EDIT-PATH' 'Long ELF paths follow Home/End and discarded edits never replace the ELF' {
        Send-Key "${esc}[B";Send-Key "`r";Wait-Screen 'editing ELF file';Wait-Screen 'guest0.elf▏'
        Send-Key "${esc}[H";Wait-Screen '▏D:/';Send-Key 'discard-';Wait-Screen 'discard-▏'
        Send-Key ([string]$esc);Send-Key "`r";Send-Key "${esc}[F";Wait-Screen 'guest0.elf▏';Send-Key ([string]$esc)
    }
    Test-Case 'IMAGE-EDIT-ERROR' 'Invalid maps retain editing; cancel and focus changes remove stale error text' {
        Show-Advanced;Select-Field 'Source maps';Send-Key "`r";Send-Key ([string][char]21+'invalid');Send-Key "`r";Wait-Screen 'Use from => to'
        Wait-Screen 'editing Source maps';Send-Key ([string]$esc);Select-Field 'Id';Wait-Screen 'Id: Enter edits'
        if ($terminal.Screen() -match 'Use from => to; separate') {throw 'A previous map error leaked into Id help'}
    }
    Test-Case 'IMAGE-EDIT-PERSIST' 'Apply/save persists the edited id and retains saved_context, sections and valid ELF paths' {
        Apply-Save;Stop-Setup;$script:after=@(Inventory 'after-edit')
        $vm=@($after | Where-Object id -eq 'vm0_changed');$old=@($before | Where-Object id -eq 'vm0')
        if ($vm.Count -ne 1 -or $vm[0].sections.'.text' -ne 0x10020000) {throw 'Edited id or advanced sections did not persist'}
        if (($vm[0].saved_context | ConvertTo-Json -Depth 10 -Compress) -ne ($old[0].saved_context | ConvertTo-Json -Depth 10 -Compress)) {throw 'Editing changed the saved Guest context'}
        if ((Normal-Path $vm[0].elf) -ne (Normal-Path $old[0].elf)) {throw 'Discarded path edits changed the ELF'}
    }
    Test-Case 'IMAGE-EDIT-RESIZE' 'Saved edits reopen; the insertion point stays visible at minimum size' {
        Start-Setup;Send-Key "${esc}[C";Send-Key "${esc}[B";Wait-Screen '› Id.*vm0_changed';Send-Key "`r"
        $terminal.Resize(45,12);Start-Sleep -Milliseconds 250;Wait-Screen 'Editing · Enter: accept';Wait-Screen '▏'
        Send-Key ([string][char]21+'vm0');Wait-Screen 'vm0▏';$terminal.Resize(120,36);Start-Sleep -Milliseconds 250;Send-Key "`r"
    }
    Test-Case 'IMAGE-EDIT-ADD-REMOVE' 'Add/remove remain reachable and editing changes stay in the project draft' {
        Select-Field 'Add image';Send-Key "`r";Wait-Screen 'image-1'
        Send-Key "`r";Wait-Screen 'editing ELF file';Send-Key 'discard.elf';Send-Key ([string]$esc)
        Select-Field 'Remove image';Send-Key "`r";Wait-Screen '3 / 3';Apply-Save
    }
    Test-Case 'IMAGE-EDIT-EXIT' 'Fresh file inventory contains the original three images and the user project is untouched' {
        Stop-Setup;$final=@(Inventory 'final');if ($final.Count -ne 3 -or @($final | Where-Object id -eq 'vm0').Count -ne 1) {throw 'Draft add/remove did not restore the three images'}
        if ((Get-FileHash -LiteralPath $Project).Hash -ne $userHash) {throw 'Test changed the user project'}
    }
} finally {
    if ($terminal) {$terminal.Screen() | Set-Content -LiteralPath (Join-Path $out 'last.screen.txt');$terminal.Send([string][char]17);[void]$terminal.WaitForExit(15000);$terminal.Transcript() | Set-Content -LiteralPath (Join-Path $out 'last.vt.txt');$terminal.Dispose()}
    $passed=@($cases | Where-Object status -eq passed).Count;$failed=@($cases | Where-Object status -eq failed).Count
    @{binary=$Binary;binary_sha256=(Get-FileHash -LiteralPath $Binary).Hash.ToLowerInvariant();board_tests_executed=$false;layer='Native ConPTY image editor and fresh offline project reload';passed=($passed -eq 8 -and $failed -eq 0);counts=@{passed=$passed;failed=$failed;skipped=(8-$cases.Count)};cases=$cases.ToArray();fixture=$fixture;user_project=$Project;user_project_sha256=$userHash} | ConvertTo-Json -Depth 7 | Set-Content -LiteralPath (Join-Path $out 'report.json')
    Write-Output "RESULT passed=$passed failed=$failed $out"
}
if ($failed -ne 0 -or $passed -ne 8) {exit 1}
