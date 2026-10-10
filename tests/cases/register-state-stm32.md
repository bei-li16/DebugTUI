# 系统寄存器状态与保存回归（1.1.3 / 1.1.4）

软件回归运行 `cargo test --locked`。缓存用例位于 `src/ui/registers/capability_cache_tests.rs`；协调器测试位于 `src/coordinator/control.rs`；`tests/register_display.rs` 对旧单核及双核协调入口各执行六项真实二进制协议检查，不启动 GDB。

## STM32F429 上板

接好已烧录且 ELF 匹配的 STM32F429/CMSIS-DAP，关闭其他调试会话：

```powershell
node scripts/test-register-state-stm32-hardware.cjs --run --binary target/release/debugtui.exe --project G:/Data/GitFiles/Keil/STM32_CubeIDE/FreeRTOS_Project/debug.toml
```

不传 `--run` 只生成 skipped 报告，不访问目标。实际运行使用项目 TOML 的临时副本，解析 ELF/源码相对路径，保留 Chip/Core 和 PPB 路由；临时副本退出策略改为 disconnect、清空 before_disconnect，测试结束保持暂停。已有显式 `[actions]` 的工程先人工审查，不自动覆盖。脚本不构建、不烧录固件、不写原工程配置。

九项验收包含连接/身份、分批 MPU/FPU 读取及独立 GDB 交叉比对、Core/FPU 全部当前定义及 D/S 别名、MPU 区域及独立 Tcl 比对/恢复、视图偏好持久化/重开/零目标 I/O、R0 Preview/Cancel/Apply/独立读回/防重放/恢复、Continue/Pause 的真实过期及刷新、退出和原配置摘要不变。R0 在暂停中短暂写入一个模式，恢复并独立核验后才继续；失败时不继续执行。若报告出现 WRITE-RESTORE-REQUIRED，保持暂停并核验报告原值。报告保存原始失败，不覆盖为通过。

1.1.4 另核对 R0 预览和结果明确包含 `verification_basis=gdb_register_view`、`physical_storage_verified=false` 及缓存警告。这里的 R0 回读通过另一条 GDB 求值命令核对寄存器视图，不能据此声称物理存储已独立验证；跨模块 PPB 写入的独立 Tcl 参考与恢复见 [模块回归](register-modules-stm32.md)。

## 实板数据 UI 回放

上板报告打印目录，将其 `ui-replay.json` 传入：

```powershell
$env:DEBUGTUI_REGISTER_UI_REPLAY = 'G:\Data\GitFiles\DebugTUI\artifacts\register-state-stm32-实际目录\ui-replay.json'
cargo test --locked --lib stm32_board_capture_replays_valid_stale_refresh_and_rendering -- --ignored --nocapture
```

这项平常标记 ignored，必须显式运行。它调用真实 App 的响应/快照处理和 Ratatui 渲染器，检查分批读取不误灰、所有采样的 UI 状态与 raw 值、真实暂停代次变化导致过期及重读恢复，另生成三个渲染文本。独立 headless 后端成功不能证明 UI 通过；此回放也不替代终端交互或 R52 实板验收。
