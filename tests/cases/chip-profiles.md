# Independent chip and probe configurations

## Bundled tools (0.10.0-readonly.4)

Run `node scripts/test-bundled-tools.cjs --binary <installed-npm-or-portable-exe>` against the complete distribution. Its 14 cases check real bundled GDB/OpenOCD resource loading without target initialization, current-directory project discovery, minimal configuration creation, three independent probes, STM32 and R52 physical core selection, invalid probes, portable SVD references and traversal rejection, external user/project chip precedence, upgrade preservation, and relative paths after a project move. No tools are copied to the firmware project. Unit tests also cover Setup Probe editing/saving and chip-relative SVD inheritance. `scripts/test-register-distribution.cjs` verifies production npm/ZIP contents and isolated install/upgrade/uninstall/reinstall.

`./scripts/test-resource-picker.ps1 -Binary <installed-exe>` exercises seven cases in a real Windows ConPTY: old project load, legacy Probe selection/cancel, bundled profile and stock SVD migration, resolved package directory browsing, SVD choices, save with project fields preserved, and clean exit. It uses an isolated project and never starts a debugger or server. Matching unit tests also verify custom SVD preservation, Automatic/Disabled behavior and portable aliases when selecting files through the browser.

Custom R52 boards should use an external chip TOML that extends `builtin:devices/tha6206.toml` (or the appropriate chip) and references their reviewed board cfg. Do not edit the package payload: an upgrade replaces those defaults. Hardware cases below remain deferred; select the probe in Setup / `[tools] probe` and reference the custom cfg through `[tools] chip_profile`.

## Legacy project-local installer compatibility

Software checks (no target connection):

```powershell
cargo test --locked --lib config::chip_profiles::
cargo test --locked --lib devices::tests::
node scripts/test-chip-profiles.cjs --binary target/release/debugtui.exe
```

The CLI suite installs tools only in a new artifact fixture with Chinese/spaces in its path. It checks the single tools entry and STM32 board script, renamed devices/svd/openocd directories, all three probe selections, STM32F429 and THA6 single/multicore configuration, non-contiguous physical cores, a temporary legacy-syntax fixture, and separate OpenOCD adapter/board script parsing with `shutdown` and no `init`. Upgrade cases preserve Chip/Core, ELF and Watch, remove only generated stock SVD overrides, and retain modified SVD files. Project/profile and isolated user catalogue contents must be preserved during loading. The unfinished R52 board template must fail with its explicit configuration message. Unit tests cover declaring-file-relative paths, inheritance, project overrides/explicit clears, provenance, new chip registration without common-profile edits, missing files, malformed fields, backend mismatch, cycles and the 8-file depth limit.

Installation must put the shipped `tools/debug.toml` at the project root and tool files directly under `.vscode/`, with no duplicate project template or installer there. The suite moves the entire installed project before loading STM32/R52 using relative profile references from a different working directory. It checks PowerShell 5/7 installation, same-drive relative ELF conversion (including spaces, Chinese, `#` and `%`), rejection of cross-drive ELF arguments before writes, and preservation of the first project backup.

Installation enumerates `bin/` directly without a dependency manifest. The suite compares every installed tool file with its source and verifies that reinstalling removes the obsolete manifest without parsing it.

Hardware cases, deferred until a suitable board is available:

1. STM32F429: select the actual probe in the shared profile; use one Project/Tools path, Chip `stm32f429`, Core 0. Confirm SWD connection, symbols, halt/run/reset, flash policy, SVD and supported AHB memory access. Repeat with each available probe.
2. R52: first complete `.vscode/openocd/r52-template.cfg` with reviewed adapter transport, DAP/AP/CTI/debug bases and reset policy. Confirm it exposes `core.N` and port `3333 + N`. Select Core 1 alone, then all available cores; also `[1,3]` on a suitable four-core device. Confirm physical identity and register routing on each selected core.
3. Switch STM32 → R52 → STM32 while retaining the Project/Tools paths and using the matching firmware ELF. Confirm no stale SVD, memory channel, register route or reset/download action survives the switch. Keep Watch/breakpoint preferences scoped to their original chip/core.
4. Before each connection, inspect the resolved Chip/Core/probe and intended build/download actions. Software configuration parsing is not evidence of a valid board mapping or successful hardware access.
