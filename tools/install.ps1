# Install these tools into PROJECT\.vscode and point PROJECT\debug.toml at
# them, so `debugtui` started in PROJECT debugs with the bundled GDB/OpenOCD.
# Runs under Windows PowerShell 5.1 and PowerShell 7.
# Usage: install.ps1 PROJECT_DIR [auto|jlink|cmsis-dap|stlink] [ELF]
param([Parameter(ValueFromRemainingArguments = $true)][string[]]$Arguments)
$ErrorActionPreference = 'Stop'
$tools = $PSScriptRoot
$profiles = @{ 'jlink' = 'debug-env.toml'; 'cmsis-dap' = 'debug-env-cmsis-dap.toml'; 'stlink' = 'debug-env-stlink.toml' }

# cmd.exe does not treat single quotes as quoting and splits 'C:\my project'
# at the space; rejoin such pieces, and accept "path" as well as 'path'.
$values = [Collections.Generic.List[string]]::new()
for ($i = 0; $i -lt $Arguments.Count; $i++) {
    $value = $Arguments[$i]
    if ($value.StartsWith("'")) {
        while (-not ($value.Length -gt 1 -and $value.EndsWith("'")) -and $i + 1 -lt $Arguments.Count) {
            $i++
            $value += ' ' + $Arguments[$i]
        }
    }
    $values.Add($value.Trim().Trim("'"))
}
if ($values.Count -lt 1 -or $values.Count -gt 3) { throw 'Usage: install.bat PROJECT_DIR [auto|jlink|cmsis-dap|stlink] [ELF]' }
$Project = $values[0]
$Probe = if ($values.Count -gt 1) { $values[1] } else { 'auto' }
$Elf = if ($values.Count -gt 2) { $values[2] } else { $null }
if (@('auto', 'jlink', 'cmsis-dap', 'stlink') -notcontains $Probe) { throw "Unknown probe '$Probe'; use auto, jlink, cmsis-dap or stlink." }
if (-not (Test-Path -LiteralPath $Project -PathType Container)) { throw "Project directory not found: $Project" }
$projectRoot = (Resolve-Path -LiteralPath $Project).Path.TrimEnd('\')
$target = Join-Path $projectRoot '.vscode'

function Get-ProjectPath([string]$Path) {
    $full = [IO.Path]::GetFullPath($Path)
    if ($full.StartsWith($projectRoot + '\', [StringComparison]::OrdinalIgnoreCase)) {
        return './' + $full.Substring($projectRoot.Length + 1).Replace('\', '/')
    }
    return $full.Replace('\', '/')
}

function Get-FileSha256([string]$Path) {
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

# The string value of KEY in [TABLE], or $null when it is not set.
function Get-TomlString([string[]]$Lines, [string]$Table, [string]$Key) {
    $inTable = $false
    foreach ($line in $Lines) {
        if ($line -match '^\s*\[') {
            $inTable = $line -match ('^\s*\[' + [regex]::Escape($Table) + '\]\s*(#.*)?$')
        } elseif ($inTable -and $line -match ('^\s*' + [regex]::Escape($Key) + '\s*=\s*(?:"([^"]*)"|''([^'']*)'')')) {
            return [string]$Matches[1] + [string]$Matches[2]
        }
    }
    return $null
}

# Set KEY = "VALUE" inside [TABLE], adding the key or the table when missing.
function Set-TomlString([string[]]$Lines, [string]$Table, [string]$Key, [string]$Value) {
    $entry = $Key + ' = "' + $Value + '"'
    $result = [Collections.Generic.List[string]]::new()
    $inTable = $false
    $done = $false
    foreach ($line in $Lines) {
        if ($line -match '^\s*\[') {
            if ($inTable -and -not $done) { $result.Add($entry); $done = $true }
            $inTable = $line -match ('^\s*\[' + [regex]::Escape($Table) + '\]\s*(#.*)?$')
        } elseif ($inTable -and -not $done -and $line -match ('^\s*' + [regex]::Escape($Key) + '\s*=')) {
            $result.Add($entry)
            $done = $true
            continue
        }
        $result.Add($line)
    }
    if (-not $done) {
        if (-not $inTable) {
            if ($result.Count -and $result[$result.Count - 1].Trim()) { $result.Add('') }
            $result.Add('[' + $Table + ']')
        }
        $result.Add($entry)
    }
    return , $result.ToArray()
}

# Probe: from the argument, or from the USB devices present.
if ($Probe -eq 'auto') {
    $devices = @(Get-CimInstance Win32_PnPEntity -ErrorAction SilentlyContinue | Where-Object { $_.DeviceID -like 'USB\VID_*' })
    $found = @()
    if ($devices | Where-Object { $_.DeviceID -match 'VID_1366' }) { $found += 'jlink' }
    if ($devices | Where-Object { $_.DeviceID -match 'VID_0483&PID_(3744|3748|374B|374D|374E|374F|3752|3753|3754|3755|3757)' }) { $found += 'stlink' }
    # CMSIS-DAP probes come from many vendors; like OpenOCD, recognize them by
    # "CMSIS-DAP" in the USB product string, which Windows keeps as the
    # bus-reported description (e.g. ATK-FS-HID-CMSIS-DAP).
    $dap = $devices | Where-Object {
        $reported = try {
            (Invoke-CimMethod -InputObject $_ -MethodName GetDeviceProperties -Arguments @{ devicePropertyKeys = @('DEVPKEY_Device_BusReportedDeviceDesc') } -ErrorAction Stop).deviceProperties[0].Data
        } catch { '' }
        "$($_.Name) $reported" -match 'CMSIS-DAP'
    }
    if ($dap) { $found += 'cmsis-dap' }
    if ($found.Count -eq 1) {
        $Probe = $found[0]
        Write-Output "Detected probe: $Probe"
    } else {
        $Probe = 'jlink'
        $why = if ($found.Count) { "Several probe types are connected ($($found -join ', '))" } else { 'No debug probe detected' }
        Write-Output "$why; using jlink. Pass jlink, cmsis-dap or stlink as the second argument to choose."
    }
}

# The binaries must match the lock before anything is copied.
$lockPath = Join-Path $tools 'dependencies.lock.json'
$lock = Get-Content -LiteralPath $lockPath -Raw | ConvertFrom-Json
$binaries = @($lock.files.PSObject.Properties | ForEach-Object { $_.Name })
foreach ($entry in $lock.files.PSObject.Properties) {
    $source = Join-Path $tools $entry.Name
    if (-not (Test-Path -LiteralPath $source -PathType Leaf)) { throw "Missing tool file: $($entry.Name)" }
    if ((Get-FileSha256 $source) -ne $entry.Value.sha256) { throw "Tool checksum mismatch: $($entry.Name)" }
}

# A previous install's lock lists what it copied; drop files this version no longer has.
$installedLock = Join-Path $target 'dependencies.lock.json'
if (Test-Path -LiteralPath $installedLock) {
    $previous = Get-Content -LiteralPath $installedLock -Raw | ConvertFrom-Json
    foreach ($name in @($previous.files.PSObject.Properties | ForEach-Object { $_.Name })) {
        $stale = Join-Path $target $name
        if ($binaries -notcontains $name -and (Test-Path -LiteralPath $stale -PathType Leaf)) {
            Remove-Item -LiteralPath $stale -Force
        }
    }
}

# Copy with the same layout as tools/, so every relative path in the profiles holds.
$settings = @($profiles.Values) + @(Get-ChildItem -LiteralPath (Join-Path $tools 'config') -File | ForEach-Object { 'config/' + $_.Name })
$chipRoot = Join-Path $tools 'chip'
$chipFiles = @(Get-ChildItem -LiteralPath $chipRoot -File -Recurse | ForEach-Object { 'chip/' + $_.FullName.Substring($chipRoot.Length + 1).Replace('\', '/') })
foreach ($name in $binaries + $settings + $chipFiles + @('dependencies.lock.json')) {
    $source = Join-Path $tools $name
    $destination = Join-Path $target $name
    New-Item -ItemType Directory -Path (Split-Path -Parent $destination) -Force | Out-Null
    # Keep a copy of a profile or board config the user edited.
    if ($settings -contains $name -and (Test-Path -LiteralPath $destination) -and (Get-FileSha256 $destination) -ne (Get-FileSha256 $source)) {
        Copy-Item -LiteralPath $destination -Destination ($destination + '.bak') -Force
        Write-Output "Kept your edited $name as $name.bak"
    }
    Copy-Item -LiteralPath $source -Destination $destination -Force
}
foreach ($entry in $lock.files.PSObject.Properties) {
    if ((Get-FileSha256 (Join-Path $target $entry.Name)) -ne $entry.Value.sha256) { throw "Installed file differs from the lock: $($entry.Name)" }
}

# The chip description the project uses: the one the selected profile names
# relative to itself, or else the only .svd in chip/, seen from the project
# under .vscode. The bundled profiles leave it out, since DebugTUI 0.10.0 and
# earlier reject [program] in a profile.
$svdValue = $null
$profileSvd = Get-TomlString ([IO.File]::ReadAllLines((Join-Path $target $profiles[$Probe]))) 'program' 'svd'
if ($profileSvd -and $profileSvd -match '^(\./|\$\{profile_dir\}/)') {
    $svdValue = './.vscode/' + ($profileSvd -replace '^(\./|\$\{profile_dir\}/)', '')
} elseif (-not $profileSvd) {
    $chipSvds = @(Get-ChildItem -LiteralPath (Join-Path $target 'chip') -Filter '*.svd' -File -ErrorAction SilentlyContinue)
    if ($chipSvds.Count -eq 1) { $svdValue = './.vscode/chip/' + $chipSvds[0].Name }
}
if ($svdValue -and -not (Test-Path -LiteralPath (Join-Path $projectRoot $svdValue) -PathType Leaf)) { $svdValue = $null }

# ELF: from the argument or, for a new debug.toml, the newest one built in the project.
$debugToml = Join-Path $projectRoot 'debug.toml'
$elfValue = $null
if ($Elf) {
    $elfPath = if ([IO.Path]::IsPathRooted($Elf)) { $Elf } else { Join-Path $projectRoot $Elf }
    if (-not (Test-Path -LiteralPath $elfPath -PathType Leaf)) { Write-Warning "ELF not found yet: $elfPath" }
    $elfValue = Get-ProjectPath $elfPath
} elseif (-not (Test-Path -LiteralPath $debugToml)) {
    $candidates = @(Get-ChildItem -LiteralPath $projectRoot -Filter '*.elf' -File -Recurse -Depth 4 -ErrorAction SilentlyContinue |
        Where-Object { $_.FullName -notmatch '\\(\.vscode|\.git|node_modules)\\' } |
        Sort-Object LastWriteTime -Descending)
    if ($candidates.Count) {
        $elfValue = Get-ProjectPath $candidates[0].FullName
        Write-Output "ELF: $elfValue"
        if ($candidates.Count -gt 1) {
            Write-Output "  newest of $($candidates.Count); pass the ELF as the third argument to choose another."
        }
    } else {
        Write-Warning 'No ELF found in the project; build it, then set [program] elf in debug.toml or rerun with the ELF.'
    }
}

# debug.toml: create it, or point an existing one at the installed profile.
$profileValue = './.vscode/' + $profiles[$Probe]
$utf8 = [Text.UTF8Encoding]::new($false)
if (-not (Test-Path -LiteralPath $debugToml)) {
    $lines = @('version = 2', 'watch = []', '', '[tools]', ('profile = "' + $profileValue + '"'), '', '[program]')
    if ($elfValue) { $lines += 'elf = "' + $elfValue + '"' }
    $lines += 'source_root = "."'
    if ($svdValue) { $lines += 'svd = "' + $svdValue + '"' }
    $lines += @('', '[session]', 'on_exit = "detach"')
    [IO.File]::WriteAllText($debugToml, ($lines -join "`n") + "`n", $utf8)
    Write-Output "Created $debugToml"
} else {
    $text = [IO.File]::ReadAllText($debugToml).TrimStart([char]0xFEFF)
    if ($text -match '(?m)^\s*\[gdb\]') {
        Write-Warning "debug.toml has its own [gdb] section, which takes precedence over a tools profile; left unchanged. Remove [gdb] and set [tools] profile = `"$profileValue`" to use these tools."
    } else {
        $lines = $text -split "`r?`n"
        if ($lines.Count -and $lines[$lines.Count - 1] -eq '') { $lines = $lines[0..($lines.Count - 2)] }
        $lines = Set-TomlString $lines 'tools' 'profile' $profileValue
        if ($Elf) { $lines = Set-TomlString $lines 'program' 'elf' $elfValue }
        # An SVD that is this chip description kept elsewhere (for example in
        # the DebugTUI checkout) now uses the .vscode copy. Other SVDs and an
        # empty value, which disables SVD, stay as they are.
        $currentSvd = Get-TomlString $lines 'program' 'svd'
        if ($svdValue -and $currentSvd -and $currentSvd -ne $svdValue) {
            $currentPath = if ([IO.Path]::IsPathRooted($currentSvd)) { $currentSvd } else { Join-Path $projectRoot $currentSvd }
            $same = if (Test-Path -LiteralPath $currentPath -PathType Leaf) {
                (Get-FileSha256 $currentPath) -eq (Get-FileSha256 (Join-Path $projectRoot $svdValue))
            } else {
                [IO.Path]::GetFileName($currentSvd) -eq [IO.Path]::GetFileName($svdValue)
            }
            if ($same) { $lines = Set-TomlString $lines 'program' 'svd' $svdValue }
        }
        $updated = ($lines -join "`n") + "`n"
        if ($updated -ne ($text -replace "`r`n", "`n")) {
            # The first backup holds the project's own version; later runs keep it.
            $backup = $debugToml + '.bak'
            if (-not (Test-Path -LiteralPath $backup)) { Copy-Item -LiteralPath $debugToml -Destination $backup }
            [IO.File]::WriteAllText($debugToml, $updated, $utf8)
            Write-Output "Updated $debugToml (original kept in debug.toml.bak)"
        } else {
            Write-Output "debug.toml already uses $profileValue"
        }
    }
}

# Earlier installs copied the SVD to .vscode\svd. Drop files identical to the
# chip/ copies once debug.toml no longer refers to that folder.
$oldSvd = Join-Path $target 'svd'
$projectText = if (Test-Path -LiteralPath $debugToml) { [IO.File]::ReadAllText($debugToml) } else { '' }
if ((Test-Path -LiteralPath $oldSvd -PathType Container) -and $projectText -notmatch '\.vscode/svd/') {
    $chipFiles = @(Get-ChildItem -LiteralPath (Join-Path $target 'chip') -File -Recurse -ErrorAction SilentlyContinue)
    foreach ($old in @(Get-ChildItem -LiteralPath $oldSvd -File)) {
        $hash = Get-FileSha256 $old.FullName
        if ($chipFiles | Where-Object { $_.Name -eq $old.Name -and (Get-FileSha256 $_.FullName) -eq $hash }) {
            Remove-Item -LiteralPath $old.FullName -Force
        }
    }
    if (-not @(Get-ChildItem -LiteralPath $oldSvd -Force)) { Remove-Item -LiteralPath $oldSvd -Force }
}

Write-Output "Installed DebugTUI tools for $Probe into $target"
Write-Output "Start debugging: cd `"$projectRoot`"; debugtui   (Setup opens with this project; press F5)"
Write-Output "             or: debugtui --project `"$projectRoot`"   (connects immediately)"
Write-Output 'The .vscode\bin folder holds about 15 MB of binaries; add .vscode/bin/ to .gitignore if it should stay out of git.'
