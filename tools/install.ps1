# Install these tools into PROJECT\.vscode and point PROJECT\debug.toml at
# them, so `debugtui` started in PROJECT debugs with the bundled GDB/OpenOCD.
# Runs under Windows PowerShell 5.1 and PowerShell 7.
# Usage: install.ps1 PROJECT_DIR [auto|jlink|cmsis-dap|stlink] [ELF]
param([Parameter(ValueFromRemainingArguments = $true)][string[]]$Arguments)
$ErrorActionPreference = 'Stop'
$tools = $PSScriptRoot
$profileName = 'debug-env.toml'

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
if ($values.Count -lt 1 -or $values.Count -gt 3) { throw 'Usage: install.ps1 PROJECT_DIR [auto|jlink|cmsis-dap|stlink] [ELF]' }
$Project = $values[0]
$Probe = if ($values.Count -gt 1) { $values[1] } else { 'auto' }
$Elf = if ($values.Count -gt 2) { $values[2] } else { $null }
if (@('auto', 'jlink', 'cmsis-dap', 'stlink') -notcontains $Probe) { throw "Unknown probe '$Probe'; use auto, jlink, cmsis-dap or stlink." }
if (-not (Test-Path -LiteralPath $Project -PathType Container)) { throw "Project directory not found: $Project" }
$projectRoot = (Resolve-Path -LiteralPath $Project).Path.TrimEnd('\')
$target = Join-Path $projectRoot '.vscode'

function Get-ProjectPath([string]$Path) {
    $full = [IO.Path]::GetFullPath($Path)
    if (-not [string]::Equals([IO.Path]::GetPathRoot($full), [IO.Path]::GetPathRoot($projectRoot), [StringComparison]::OrdinalIgnoreCase)) {
        throw 'ELF must be on the same drive/share as the project to use a relative path. Copy it into the project first.'
    }
    $baseUri = [Uri]::new($projectRoot + '\')
    $relative = [Uri]::UnescapeDataString($baseUri.MakeRelativeUri([Uri]::new($full)).ToString())
    if ($relative.StartsWith('../')) { return $relative }
    return './' + $relative
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
    $escaped = $Value.Replace('\', '\\').Replace('"', '\"').Replace("`r", '\r').Replace("`n", '\n').Replace("`t", '\t')
    $entry = $Key + ' = "' + $escaped + '"'
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

function Remove-TomlKey([string[]]$Lines, [string]$Table, [string]$Key) {
    $inTable = $false
    $result = foreach ($line in $Lines) {
        if ($line -match '^\s*\[') { $inTable = $line -match ('^\s*\[' + [regex]::Escape($Table) + '\]\s*(#.*)?$') }
        if (-not ($inTable -and $line -match ('^\s*' + [regex]::Escape($Key) + '\s*='))) { $line }
    }
    return , @($result)
}

function Get-InstallPath([string]$Name) {
    $full = [IO.Path]::GetFullPath((Join-Path $target $Name))
    if (-not $full.StartsWith($target + '\', [StringComparison]::OrdinalIgnoreCase)) { throw "Invalid installed path: $Name" }
    return $full
}

# Validate an explicitly supplied ELF before changing any project files.
$elfValue = $null
if ($Elf) {
    $elfPath = if ([IO.Path]::IsPathRooted($Elf)) { $Elf } else { Join-Path $projectRoot $Elf }
    $elfValue = Get-ProjectPath $elfPath
    if (-not (Test-Path -LiteralPath $elfPath -PathType Leaf)) { Write-Warning "ELF not found yet: $elfPath" }
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
        $Probe = 'cmsis-dap'
        $existingProfile = Join-Path $target $profileName
        if (Test-Path -LiteralPath $existingProfile -PathType Leaf) {
            $previousProbe = Get-TomlString ([IO.File]::ReadAllLines($existingProfile)) 'probe' 'config'
            if ($previousProbe -match '(?:^|/)(cmsis-dap|jlink|stlink)\.cfg$') { $Probe = $Matches[1] }
        }
        $why = if ($found.Count) { "Several probe types are connected ($($found -join ', '))" } else { 'No debug probe detected' }
        Write-Output "$why; using $Probe. Pass jlink, cmsis-dap or stlink as the second argument to choose."
    }
}

# Discover the bundled tools directly; no installation manifest is required.
foreach ($name in @('bin/gdb/bin/arm-none-eabi-gdb.exe', 'bin/openocd/bin/openocd.exe', 'debug.toml', $profileName)) {
    if (-not (Test-Path -LiteralPath (Join-Path $tools $name) -PathType Leaf)) { throw "Missing tool file: $name" }
}
$binRoot = Join-Path $tools 'bin'
$binaries = @(Get-ChildItem -LiteralPath $binRoot -File -Recurse -Force | ForEach-Object { 'bin/' + $_.FullName.Substring($binRoot.Length + 1).Replace('\', '/') })

# Copy with the same layout as tools/, so every relative path in the profiles holds.
$settings = @($profileName)
foreach ($directory in @('devices', 'openocd')) {
    $settings += @(Get-ChildItem -LiteralPath (Join-Path $tools $directory) -File -Recurse | ForEach-Object { $_.FullName.Substring($tools.Length + 1).Replace('\', '/') })
}
$svdRoot = Join-Path $tools 'svd'
$svdFiles = @(Get-ChildItem -LiteralPath $svdRoot -File -Recurse | ForEach-Object { 'svd/' + $_.FullName.Substring($svdRoot.Length + 1).Replace('\', '/') })
foreach ($name in $binaries + $settings + $svdFiles) {
    $source = Join-Path $tools $name
    $destination = Get-InstallPath $name
    New-Item -ItemType Directory -Path (Split-Path -Parent $destination) -Force | Out-Null
    if ($svdFiles -contains $name -and (Test-Path -LiteralPath $destination -PathType Leaf) -and (Get-FileSha256 $destination) -ne (Get-FileSha256 $source)) {
        Write-Output "Preserved your edited $name"
        continue
    }
    # Keep a copy of a profile or board config the user edited.
    if ($settings -contains $name -and (Test-Path -LiteralPath $destination) -and (Get-FileSha256 $destination) -ne (Get-FileSha256 $source)) {
        Copy-Item -LiteralPath $destination -Destination ($destination + '.bak') -Force
        Write-Output "Kept your edited $name as $name.bak"
    }
    Copy-Item -LiteralPath $source -Destination $destination -Force
}

# Every probe uses the same entry and STM32 board script.
$installedProfile = Join-Path $target $profileName
$profileLines = Set-TomlString ([IO.File]::ReadAllLines($installedProfile)) 'probe' 'config' "./openocd/probes/$Probe.cfg"
[IO.File]::WriteAllText($installedProfile, ($profileLines -join "`n") + "`n", [Text.UTF8Encoding]::new($false))

# ELF: from the argument or, for a new debug.toml, the newest one built in the project.
$debugToml = Join-Path $projectRoot 'debug.toml'
if (-not $Elf -and -not (Test-Path -LiteralPath $debugToml)) {
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
$profileValue = './.vscode/' + $profileName
$utf8 = [Text.UTF8Encoding]::new($false)
if (-not (Test-Path -LiteralPath $debugToml)) {
    $lines = [IO.File]::ReadAllLines((Join-Path $tools 'debug.toml'))
    $lines = Set-TomlString $lines 'tools' 'profile' $profileValue
    $lines = Set-TomlString $lines 'tools' 'probe' $Probe
    if ($elfValue) { $lines = Set-TomlString $lines 'program' 'elf' $elfValue }
    [IO.File]::WriteAllText($debugToml, ($lines -join "`n") + "`n", $utf8)
    Write-Output "Created $debugToml"
} else {
    $text = [IO.File]::ReadAllText($debugToml).TrimStart([char]0xFEFF)
    if ($text -match '(?m)^\s*\[gdb\]') {
        Write-Warning "debug.toml has its own [gdb] section, which takes precedence over a tools profile; left unchanged. Remove [gdb] and set [tools] profile = `"$profileValue`" to use these tools."
    } else {
        # Upgrade only the root format version; retain existing chip/core and preferences.
        $firstTable = [regex]::Match($text, '(?m)^[ \t]*\[')
        $rootLength = if ($firstTable.Success) { $firstTable.Index } else { $text.Length }
        $rootText = $text.Substring(0, $rootLength)
        $versionPattern = '(?m)^([ \t]*version[ \t]*=[ \t]*)[012]([ \t]*(?:#.*)?\r?$)'
        if ($rootText -match $versionPattern) {
            $rootText = [regex]::Replace($rootText, $versionPattern, '${1}3${2}')
        } elseif ($rootText -notmatch '(?m)^[ \t]*version[ \t]*=') {
            $rootText = "version = 3`n" + $rootText
        }
        $text = $rootText + $text.Substring($rootLength)
        $lines = $text -split "`r?`n"
        if ($lines.Count -and $lines[$lines.Count - 1] -eq '') { $lines = $lines[0..($lines.Count - 2)] }
        $lines = Set-TomlString $lines 'tools' 'profile' $profileValue
        if ($Elf) { $lines = Set-TomlString $lines 'program' 'elf' $elfValue }
        # Remove only a known generated SVD override whose content is still stock.
        # Custom paths, modified files and explicit empty values are preserved.
        $currentSvd = Get-TomlString $lines 'program' 'svd'
        if ($currentSvd -and $currentSvd.Replace('\','/') -match '^\./\.vscode/(chip|svd)/([^/]+\.svd)$') {
            $bundledSvd = Join-Path $svdRoot $Matches[2]
            $currentPath = Join-Path $projectRoot $currentSvd
            if ((Test-Path -LiteralPath $currentPath -PathType Leaf) -and (Test-Path -LiteralPath $bundledSvd -PathType Leaf) -and (Get-FileSha256 $currentPath) -eq (Get-FileSha256 $bundledSvd)) {
                $lines = Remove-TomlKey $lines 'program' 'svd'
                Write-Output 'Removed the generated stock SVD override; SVD now follows Chip.'
            }
        }
        $updated = ($lines -join "`n") + "`n"
        if ($updated -ne ([IO.File]::ReadAllText($debugToml).TrimStart([char]0xFEFF) -replace "`r`n", "`n")) {
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

# Remove only the obsolete manifest left by an earlier installer.
$obsoleteManifest = Get-InstallPath 'dependencies.lock.json'
if (Test-Path -LiteralPath $obsoleteManifest -PathType Leaf) { Remove-Item -LiteralPath $obsoleteManifest -Force }

Write-Output "Installed DebugTUI tools for $Probe into $target"
Write-Output 'Select Chip and Debug cores in Setup before Start; the installer does not guess the board.'
Write-Output "Start debugging: cd `"$projectRoot`"; debugtui   (Setup opens with this project; press F5)"
Write-Output "             or: debugtui --project `"$projectRoot`" --setup"
Write-Output 'The .vscode\bin folder holds about 15 MB of binaries; add .vscode/bin/ to .gitignore if it should stay out of git.'
