# STM32F429 系统模块实板回归

工程绝对路径：`G:\Data\GitFiles\Keil\STM32_CubeIDE\FreeRTOS_Project\debug.toml`，固件 `Debug/FreeRTOS_Project.elf`。使用 Cortex-M4、CMSIS-DAP 和 OpenOCD；不构建或烧录固件，要求没有其他调试会话。

```powershell
node scripts/test-register-modules-stm32-hardware.cjs --run --binary target/debug/debugtui.exe --project G:/Data/GitFiles/Keil/STM32_CubeIDE/FreeRTOS_Project/debug.toml
```

`--catalogue profiles/registers/cortex-m4.toml` 可让旧安装版明确加载开发目录；省略则验收二进制的内置目录。没有 `--run` 时只记录 skipped，不连接板卡。

## 已准备的 15 个阶段

- 身份与容量：CPUID、16 个 D / 32 个 S 寄存器、DWT 四组比较器、FPB v1 的六个指令和两个 literal 比较器；SVD 声明的 IRQ 和优先级位数与 ICTR 上限分开记录。
- 浮点：7 项 PPB 配置/身份，D0–D15、S0–S31、FPSCR 共 49 项存储定义；Tcl `get_reg -force` 独立核验物理原值和 D/S 别名。
- NVIC：ICTR、三组 ISER/ICER/ISPR/ICPR/IABR 和 IPR0–95，对照独立 Tcl 原始值；检查读别名和越界项无目标访问。末五个优先级位置是保留位置的原始读数，不表示存在 IRQ91–95。
- DWT：CTRL、六个计数器、PCSR、四组 COMP/MASK/FUNCTION。FUNCTION 的 MATCHED 读后清除，不自动轮询；对照时屏蔽该位，保留首次手动采样。
- FPB：CTRL、REMAP、COMP0–7 的原值和布局；不启用新的断点或 flash patch。
- 可恢复写入：FPDSCR.FZ，已声明且 disabled/nonpending/nonactive IRQ 的优先级字节，disabled DWT/FPB 比较器地址，DEMCR.TRCENA。每项 Preview/Cancel 不发送、Apply 一次、Tcl 独立回读、系统目录再次读取、恢复原值并验证。优先级相邻字节与比较器控制保持不变。
- TRCENA 清零时 DWT 返回 FeatureDisabled、零 DWT 数据访问；恢复后 Probe/Read 可用。
- 未声明 D/S/FPSCR、NVIC enable/pending、DWT/FPB control writer 时明确拒绝；读者和 RW 标记不授予写权限。
- 所有临时写恢复后才 Continue/Pause，检查真实 Stale 和四组重新读取的 Valid。
- 关闭自建会话，保持板卡暂停；核对原项目/ELF SHA256。

## 写入与证据边界

MMIO 通过原厂 SVD 的临时副本追加 `VERIFY` 定义，并在临时 TOML 中声明单核、暂停、core-tcl 路由和精确字宽。原用户配置及 SVD 不修改。这证明生产 SVD/MMIO 写通道可用，不表示内置 System Regs 目录已有这些命名 writer。

GDB 回读可能来自 OpenOCD 的 write-back cache。`outcome=verified` 的 GDB writer 仅证明 `verification_basis=gdb_register_view`，`physical_storage_verified=false`；预览和结果均说明这一限制。不能因为 D0 缓存回读一致，就为 Cortex-M4 浮点存储添加通用整数 writer。

写入前将原值持久化到 `write-journal.json`。任何失败停止后续阶段；恢复失败报告 RESTORE-REQUIRED，保持暂停。须独立检查物理值，不从一次失败回执猜测目标状态。未执行阶段记 skipped，失败证据不覆盖。

## UI 和软件回归

报告目录的 `ui-replay.json` 包含初始同一暂停代次的分批采样、真实 Continue/Pause 后的快照及重新读取。传入 `DEBUGTUI_REGISTER_UI_REPLAY`，显式运行 `stm32_board_capture_replays_valid_stale_refresh_and_rendering`，检查真实 App/cache/Ratatui；它不替代终端鼠标验收。

```powershell
$env:CARGO_INCREMENTAL = '0'
$env:CARGO_PROFILE_DEV_DEBUG = '0'
$env:CARGO_PROFILE_TEST_DEBUG = '0'
$env:DEBUGTUI_REGISTER_UI_REPLAY = 'G:\Data\GitFiles\DebugTUI\artifacts\register-modules-stm32-实际目录\ui-replay.json'
cargo test --locked --offline --lib stm32_board_capture_replays_valid_stale_refresh_and_rendering -- --ignored --nocapture
pwsh -NoProfile -File scripts/cleanup-build.ps1 -Apply
```

软件回归覆盖 M4 地址/字段、NUMCOMP/NOCYCCNT/NOPRFCNT/TRCENA、FPB revision/容量与 literal 偏移、FUNCTION 手动策略、仅 FUNCTION0 的 CYCMATCH、仅 FUNCTION1 的数据匹配字段、MATCHED/LNK1ENA 只读属性、M3/M7/R52 目录隔离，以及 GDB 回执和界面的验证来源说明。

## 尚未覆盖

浮点运算/异常/lazy stacking、D/S/FPSCR 的物理写入；NVIC enable/pending/active 改写及真实抢占；DWT 运行计数、计数器改写、watchpoint 命中与 MATCHED=1 的清除；FPB 真实断点命中、literal/remap 写入；其他 MPU/SCB/SysTick 全部操作、FPB v2、M3/M7、R52/多核实板、完整终端交互。保留位置的原始读取不构成 IRQ 可用性证明。
