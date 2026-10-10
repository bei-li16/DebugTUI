# DebugTUI 1.1.4：Cortex-M4 DWT/FPB 与写入验证说明

发布日期：2026-10-11。Windows x64 正式版本，从开发分支 `codex/register-state-fixes` 发布，标签 `v1.1.4`，设为 Latest。以下为相对于 **1.1.3** 的更新。

## 更新内容

- Cortex-M4 系统寄存器目录补齐 **28 项**：DWT 的六个计数器、PCSR、四组 COMP/MASK/FUNCTION，以及 FPB 的 REMAP、六个指令和两个 literal 比较器。目录定义总数由 **386 到 414**，EXE 内置目录与 npm/ZIP 中的 TOML 一起更新。
- 按实际 NUMCOMP、NOCYCCNT/NOPRFCNT、TRCENA、FPB revision 和容量判断读取资格；不能从一份通用目录推断所有条目都存在。DWT.FUNCTION 的 MATCHED 具有读后清除副作用，保留手动读取；只为 FUNCTION0 提供 CYCMATCH，只为 FUNCTION1 提供数据匹配字段，MATCHED/LNK1ENA 标为只读。软件不通过试写探测数据匹配能力。FPB 新定义适配 v1，未知或不匹配布局保持受限。
- 通用 GDB writer 的预览、结果和 UI 明确区分 GDB 寄存器视图回读与物理存储独立验证。回执包含 `verification_basis="gdb_register_view"`、`physical_storage_verified=false`，并提示服务器 write-back cache 可能暂存写入。
- 不为 Cortex-M4 D/S/FPSCR 新增未经物理验证的 writer；RW 标记、reader 可用或缓存回读一致都不授予写入权限。SVD/MMIO 写入继续要求显式域、通道、字宽和暂停条件。
- 补充容量/启用/版本/读取副作用的软件回归、可重跑的 FPU/NVIC/DWT/FPB 实板驱动和跨模块 UI 采样回放。沿用 1.1.3 的有效值不误灰、真实 Stale 和偏好保存修复。

## 安装与升级

退出正在运行的 DebugTUI，在 PowerShell 执行：

```powershell
npm.cmd install -g --prefer-online --ignore-scripts=false --foreground-scripts --allow-scripts=@debugtui/cli "https://github.com/bei-li16/DebugTUI/releases/latest/download/debugtui-cli.tgz"
debugtui --version
# debugtui 1.1.4
cd G:\Data\GitFiles\Keil\STM32_CubeIDE\FreeRTOS_Project
debugtui
```

固定本版可将 `latest/download` 换成 `download/v1.1.4`。便携用户完整解压 `debugtui-1.1.4-win-x64.zip` 后运行 EXE。npm 与 ZIP 包含现有 GDB/OpenOCD、芯片/探针配置、SVD、许可证和说明；本次不升级工具链。独立 EXE 附件不带工具资源。

工程继续使用已有 `debug.toml`，不必重新生成，也不会因升级修改 ELF、Build/Download 指令或用户目录。首次空目录启动仍生成最简通用 TOML，再选择 Probe、Chip、Debug cores 和 ELF。显式 catalogue 或包外同名用户目录优先于内置目录，升级保留自定义文件；若看不到新增条目，在 **Status** 核对目录来源，按需选择新版包内 `profiles/registers/cortex-m4.toml`，同时保留自己的修改。

## 系统寄存器操作

1. 在 Setup 选择 STM32F429、调试 core 0 和工程 ELF，确认工具 profile 为 `builtin:arm-openocd`，Probe 按实际探针选择。Start 后暂停当前核心，选择物理 frame 0。
2. 打开 **System Regs → Probe caps**，获取本次暂停的身份、实现容量与启用状态。
3. 通过 **Group / Find / Target / All** 选择浮点、NVIC、DWT 或 FPB，展开后按需 **Read**。`Total` 是目录定义数，`Shown` 是显示的寄存器行数，`Valid` 是当前上下文的有效采样；它们不表示全部硬件已验收。
4. `[Stale]` 是旧值过期，重新 **Read** 可刷新。`Not read`、`FeatureDisabled`、`Unsupported` 和访问错误含义不同，使用 **Status** 或 `t` 查看原因、来源和实际访问记录。软件不自动使能 DWT/FPU。
5. DWT.FUNCTION 仅手动读取；MATCHED 状态可能因读取被清除。NVIC 的 ICTR 是按 32 分组的位置上限，实际 IRQ 与优先级位数按芯片 SVD 判断。D/S 是别名，16 个 D 寄存器不代表支持双精度运算。
6. 已声明 writer 的对象使用 **Edit value → Preview → Apply**，Cancel 不发送。通用 GDB writer 的 Verified 只保证 GDB 视图回读一致，不能代替独立物理验证。Cortex-M4 D/S/FPSCR 暂无 writer；本次受控 MMIO 写入测试不自动向工程开放系统控制写权限。

## 验证与限制

本轮 STM32F429 / Cortex-M4 / CMSIS-DAP / OpenOCD 使用现有 FreeRTOS_Project 固件，不重新烧录。实板检查 FPU 56、NVIC 112、DWT 20、FPB 10 项定义，另检查 MPU.TYPE 与 DEMCR，共 **200 项目录定义**。PPB 与独立 Tcl 内存读取比较，D 寄存器/FPSCR 使用强制物理读参考并检查 S 别名。计数包含别名和 NVIC 保留位置，不表示 200 个独立硬件寄存器。ICTR 的 96 是上限，SVD 声明 91 个不同 IRQ，IPR91–95 不证明存在中断。

临时写入 FPDSCR.FZ、空闲 IRQ 优先级、未启用的 DWT/FPB 比较器地址以及 DEMCR.TRCENA，每项记录原值、独立回读、目录回读和恢复；所有写入恢复后再 Continue/Pause，检查 Stale 与刷新。MMIO 使用隔离 SVD 副本和明确的暂停 core-tcl 写域，原工程和固件保持不变。实际 App/cache/Ratatui 回放检查界面状态，但不替代完整终端鼠标验收。

未验收浮点运算/异常/lazy stacking、D/S/FPSCR 物理写入；NVIC 触发/抢占及 enable/pending/active 改写；DWT 运行计数和 watchpoint 命中；FPB 真实断点/literal/remap 写入；其他 M/R 核与多核实板。R52 软件范围沿用既有版本，不因本次 M4 增补扩大。

最终发行 EXE 的软件、实板、目录交付、旧版升级和本地安装验收见 [TESTING.md](../TESTING.md)，机器可读摘要随 Release 附件提供。可重跑命令及后续用例见 [STM32 模块回归](../tests/cases/register-modules-stm32.md)。

## 发布附件

- `debugtui-cli.tgz` / `debugtui-cli-1.1.4.tgz`：固定下载名称和版本化 npm 安装包。
- `debugtui-1.1.4-win-x64.zip`：完整便携包。
- `debugtui-windows-x64-1.1.4.exe`：独立程序。
- `corresponding-source.zip`：随包修改版 OpenOCD 的固定对应源码，摘要与 PROVENANCE 一致。
- `validation-1.1.4.json` / `SHA256SUMS.txt`：验证摘要与附件校验和。

完整使用说明见 [USER_GUIDE.md](../USER_GUIDE.md)。
