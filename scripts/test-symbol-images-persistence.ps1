param([Parameter(Mandatory)][string]$Binary,[Parameter(Mandatory)][string]$Project)
$ErrorActionPreference='Stop'
$root=Split-Path $PSScriptRoot
$Binary=(Resolve-Path -LiteralPath $Binary).Path
$Project=(Resolve-Path -LiteralPath $Project).Path
$bao=(Split-Path $Project).Replace('\','/')
$out=Join-Path $root ('artifacts/symbol-images-persistence-'+[guid]::NewGuid().ToString('N').Substring(0,8))
New-Item -ItemType Directory -Path $out | Out-Null
$text=[IO.File]::ReadAllText($Project)
$text=[regex]::Replace($text,'(?m)^\[images\.sections\]\r?\n(?=\s*(?:\[|$))','')
$text=[regex]::Replace($text,'(?m)^elf = "(?![A-Za-z]:|/)','elf = "'+$bao+'/')
$text=$text.Replace('source_root = "."','source_root = "'+$bao+'"')
$at=$text.IndexOf('[images.saved_context]')
if($at -lt 0) {throw 'Requires the Bao reference saved_context project'}
$text=$text.Insert($at,"[images.sections]`n`".text`" = 0x10020000`n`n")
$fixture=Join-Path $out 'debug.toml';[IO.File]::WriteAllText($fixture,$text,[Text.UTF8Encoding]::new($false))
$userHash=(Get-FileHash -LiteralPath $Project).Hash
$inputs=Join-Path $out 'input images';New-Item -ItemType Directory -Path $inputs | Out-Null
Copy-Item -LiteralPath (Join-Path $bao 'bin/tha6xxx/tha6206-dual/bao.elf') -Destination (Join-Path $inputs 'bao updated.elf')
Copy-Item -LiteralPath (Join-Path $bao 'build/tha6xxx/tha6206-dual/guest/guest0.elf') -Destination (Join-Path $inputs 'guest zero.elf')
$customRoot=Join-Path $out 'source 目录';New-Item -ItemType Directory -Path $customRoot | Out-Null
Add-Type -Path (Join-Path $root 'tests/conpty.cs')
$terminal=$null;$esc=[char]27;$cases=[Collections.Generic.List[object]]::new()
function Wait-Screen([string]$Pattern) {
    $deadline=[datetime]::UtcNow.AddSeconds(15)
    do {if($terminal.Screen() -match $Pattern) {return};Start-Sleep -Milliseconds 50} while([datetime]::UtcNow -lt $deadline)
    $terminal.Screen() | Set-Content -LiteralPath (Join-Path $out 'failure.screen.txt');throw "Missing terminal state: $Pattern"
}
function Send-Key([string]$Text) {$terminal.Send($Text);Start-Sleep -Milliseconds 220}
function Select-Field([string]$Name) {
    $pattern='›\s+(?:\[\s+)?'+[regex]::Escape($Name)+'(?:\s|$)'
    for($i=0;$i -lt 24;$i++) {if($terminal.Screen() -match $pattern) {return};Send-Key "${esc}[B"}
    throw "Could not focus $Name"
}
function Start-Setup {
    $script:terminal=[DebugTuiTerminal]::new($Binary,('--setup --project "'+$fixture+'"'),$out,120,36)
    Wait-Screen 'Launch setup';Send-Key "${esc}[A";Send-Key "`r";Wait-Screen 'Symbol images · project draft'
}
function Open-Images {Select-Field 'Symbol images';Send-Key "`r";Wait-Screen 'Symbol images · project draft'}
function Choose-VM {Send-Key "${esc}[C";Wait-Screen '2 / 3'}
function Advanced {Select-Field 'Advanced settings';Send-Key "`r";Wait-Screen 'Shown · Enter: hide'}
function Edit([string]$Name,[string]$Value) {
    Select-Field $Name;Send-Key "`r";Wait-Screen ('editing '+[regex]::Escape($Name));Send-Key ([string][char]21+$Value)
}
function Save {Send-Key ([string][char]19);Wait-Screen 'Saved '}
function Stop-Setup {
    Send-Key ([string][char]17)
    if(-not $terminal.WaitForExit(15000) -or $terminal.ExitCode() -ne 0) {throw 'Setup did not exit cleanly'}
    $terminal.Transcript() | Set-Content -LiteralPath (Join-Path $out ('terminal-'+$cases.Count+'.vt.txt'))
    $terminal.Dispose();$script:terminal=$null
}
function Inventory([string]$Name) {
    $commands=Join-Path $out ($Name+'.jsonl')
    [IO.File]::WriteAllText($commands,"{`"id`":1,`"method`":`"images`"}`n{`"id`":2,`"method`":`"quit`"}`n",[Text.UTF8Encoding]::new($false))
    $lines=& $Binary --project $fixture --script $commands
    if($LASTEXITCODE -ne 0) {throw 'Saved project failed to load'}
    $lines | Set-Content -LiteralPath (Join-Path $out ($Name+'-responses.jsonl'))
    $response=@($lines | ForEach-Object {$_ | ConvertFrom-Json} | Where-Object {$_.event -eq 'response' -and $_.id -eq 1})
    if($response.Count -ne 1 -or -not $response[0].ok -or $response[0].result.target_io) {throw 'Inventory failed or accessed a target'}
    return $response[0].result.configured
}
function Normal([string]$Path) {
    if($Path.StartsWith('\\?\UNC\',[StringComparison]::OrdinalIgnoreCase)) {$Path='\\'+$Path.Substring(8)}
    elseif($Path.StartsWith('\\?\',[StringComparison]::OrdinalIgnoreCase)) {$Path=$Path.Substring(4)}
    return [IO.Path]::GetFullPath($Path)
}
function VM([string]$Name) {return @(Inventory $Name | Where-Object id -eq 'vm0_saved')[0]}
function Case([string]$Id,[string]$Description,[scriptblock]$Body) {
    try {& $Body | Out-Null;if($terminal) {$terminal.Screen() | Set-Content -LiteralPath (Join-Path $out ($Id+'.screen.txt'))};$cases.Add(@{id=$Id;description=$Description;status='passed'});Write-Output "PASS ${Id}: $Description"}
    catch {$cases.Add(@{id=$Id;description=$Description;status='failed';error=$_.Exception.Message});throw}
}
try {
    $before=@(Inventory 'before')
    Case 'PERSIST-PRIMARY' 'Editing primary ELF with Ctrl+S saves both image and Program paths immediately' {
        Start-Setup;Edit 'ELF file' '.\input images\bao updated.elf';Save
        Wait-Screen 'Program / ELF\s+input images/bao updated.elf'
        $r=@(Inventory 'primary' | Where-Object id -eq bao)[0]
        if((Normal $r.elf) -ne (Normal (Join-Path $inputs 'bao updated.elf'))) {throw 'Primary image path not saved'}
    }
    Case 'PERSIST-ID' 'Id saves directly from active editing and survives a fresh process' {
        Open-Images;Choose-VM;Edit 'Id' 'vm0_saved';Save;Stop-Setup
        if(-not (VM 'id')) {throw 'Id did not persist'};Start-Setup;Choose-VM
    }
    Case 'PERSIST-ELF' 'Guest ELF accepts backslashes and a relative path containing spaces' {
        Edit 'ELF file' '.\input images\guest zero.elf';Save
        if((Normal (VM 'elf').elf) -ne (Normal (Join-Path $inputs 'guest zero.elf'))) {throw 'Guest ELF not saved'}
    }
    Case 'PERSIST-CORES' 'Core restrictions survive Ctrl+S and offline reload' {
        Open-Images;Choose-VM;Edit 'Cores' 'core.0, core.1';Save
        if(((VM 'cores').cores -join ',') -ne 'core.0,core.1') {throw 'Core restrictions not saved'}
    }
    Case 'PERSIST-ROOT' 'Advanced source root override saves a Chinese path with spaces' {
        Open-Images;Choose-VM;Advanced;Edit 'Source root' './source 目录';Save
        if((Normal (VM 'root').source_root) -ne (Normal $customRoot)) {throw 'Source override not saved'}
    }
    Case 'PERSIST-INHERIT' 'Clearing the override inherits Setup Source root and remains blank when reopened' {
        Open-Images;Choose-VM;Advanced;Edit 'Source root' '';Save
        if((Normal (VM 'inherit').source_root) -ne (Normal $bao)) {throw 'Blank root did not inherit Setup'}
        Open-Images;Choose-VM;Advanced;Select-Field 'Source root';Wait-Screen 'inherit Setup Source root';Send-Key ([string]$esc)
    }
    Case 'PERSIST-OFFSET' 'Hex relocation input persists its numeric value' {
        Open-Images;Choose-VM;Advanced;Edit 'Section offset' '0x20';Save
        if((VM 'offset').offset -ne 32) {throw 'Offset not saved'}
    }
    Case 'PERSIST-MAPS' 'Image source maps preserve multiple prefixes and slash styles' {
        Open-Images;Choose-VM;Advanced;Edit 'Source maps' 'invalid'
        $hash=(Get-FileHash -LiteralPath $fixture).Hash;Send-Key ([string][char]19);Wait-Screen 'Use from => to'
        if((Get-FileHash -LiteralPath $fixture).Hash -ne $hash) {throw 'Invalid input was written to the file'}
        Send-Key ([string][char]21+'/ci/build => ./source 目录 | C:\ci\src => .');Save
        $vm=VM 'maps';if($vm.source_map.Count -ne 2 -or $vm.source_map[1].from -ne 'C:\ci\src' -or (Normal $vm.source_map[0].to) -ne (Normal $customRoot)) {throw 'Source maps not saved'}
    }
    Case 'PERSIST-CHANNEL' 'Memory channel can be cleared and restored without losing saved Guest settings' {
        Open-Images;Choose-VM;Advanced;Edit 'Global memory channel' '';Save
        if((VM 'channel-empty').memory_channel) {throw 'Channel clear not saved'}
        Open-Images;Choose-VM;Advanced;Edit 'Global memory channel' 'ahb';Save
        if((VM 'channel').memory_channel -ne 'ahb') {throw 'Channel not saved'}
    }
    Case 'PERSIST-DOMAIN' 'Program domain cycle persists through the Save config button' {
        Open-Images;Select-Field 'Program domain';Send-Key "`r";Select-Field 'Save config';Send-Key "`r";Wait-Screen 'Saved '
        if(@(Inventory 'domain' | Where-Object id -eq bao)[0].domain -ne 'guest') {throw 'Domain not saved'}
    }
    Case 'PERSIST-APPLY-CANCEL' 'Apply updates only draft and Cancel leaves the file unchanged' {
        $hash=(Get-FileHash -LiteralPath $fixture).Hash;Open-Images;Choose-VM;Edit 'Id' 'discarded';Send-Key "`r";Select-Field 'Apply to draft';Send-Key "`r";Wait-Screen 'Symbol image draft updated'
        if((Get-FileHash -LiteralPath $fixture).Hash -ne $hash) {throw 'Apply unexpectedly wrote the file'}
        Open-Images;Select-Field 'Cancel';Send-Key "`r";Wait-Screen 'cancelled'
        Stop-Setup;Start-Setup;Choose-VM;Select-Field 'Id';Wait-Screen '› Id\s+vm0_saved'
    }
    Case 'PERSIST-FINAL' 'Restart retains all settings, sections and saved_context; original project stays untouched' {
        Stop-Setup;$vm=VM 'final';$old=@($before | Where-Object id -eq vm0)[0]
        if(($vm.saved_context | ConvertTo-Json -Depth 10 -Compress) -ne ($old.saved_context | ConvertTo-Json -Depth 10 -Compress) -or $vm.sections.'.text' -ne 0x10020000) {throw 'Advanced Guest state or section addresses changed'}
        if((Get-FileHash -LiteralPath $Project).Hash -ne $userHash) {throw 'User project was modified'}
    }
} finally {
    if($terminal) {$terminal.Screen() | Set-Content -LiteralPath (Join-Path $out 'last.screen.txt');$terminal.Send([string][char]17);[void]$terminal.WaitForExit(15000);$terminal.Transcript() | Set-Content -LiteralPath (Join-Path $out 'last.vt.txt');$terminal.Dispose()}
    $passed=@($cases | Where-Object status -eq passed).Count;$failed=@($cases | Where-Object status -eq failed).Count
    @{binary=$Binary;binary_sha256=(Get-FileHash -LiteralPath $Binary).Hash.ToLowerInvariant();board_tests_executed=$false;layer='Native ConPTY edits plus independent saved-project reload for every configurable field';passed=($passed -eq 12 -and $failed -eq 0);counts=@{passed=$passed;failed=$failed;skipped=(12-$cases.Count)};cases=$cases.ToArray();fixture=$fixture;user_project=$Project;user_project_sha256=$userHash} | ConvertTo-Json -Depth 7 | Set-Content -LiteralPath (Join-Path $out 'report.json')
    Write-Output "RESULT passed=$passed failed=$failed $out"
}
if($failed -ne 0 -or $passed -ne 12) {exit 1}
