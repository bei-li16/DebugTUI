# DebugTUI 1.1.3：系统寄存器状态与偏好保存修复

发布日期：2026-10-10。Windows x64 正式版本，标签 `v1.1.3`，从 `codex/register-state-fixes` 发布为 Latest。以下为相对于 **1.1.2** 的改动。

## 更新内容

- 修复 System Regs 中已经成功读取的 MPU/FPU 等寄存器，在读取新的能力或字段信息后误显示灰色 `[Stale]` 的问题。读取条件未改变的样本继续有效；条件变为禁止访问时仅使相关样本过期，条件解锁后允许再次读取。
- 保留会话、核心、暂停代次、栈帧和共享 owner 的真实失效规则。继续运行、切核、重连或换帧后，旧样本仍按上下文显示过期，不把历史值当作实时值。
- 修复 Chip/Core 工程保存寄存器组、字段、筛选和格式偏好时的 `scope must be all or core` 错误。寄存器视图的身份键与执行控制的 All/Core 校验分开；执行控制仍拒绝无效范围。
- 增补缓存/条件变更/核心隔离回归；显示偏好协议同时验证旧单核和多核协调入口，并覆盖两个客户端并发保存、重新合并、非法输入拒绝和零目标 I/O。
- 增加可重跑的 STM32F429 上板驱动和实板采样 UI 回放测试，证据保存在本地 `artifacts/`。

## 安装与使用

退出正在运行的 DebugTUI，在 PowerShell 执行：

```powershell
npm.cmd install -g --prefer-online "https://github.com/bei-li16/DebugTUI/releases/latest/download/debugtui-cli.tgz"
debugtui --version
# debugtui 1.1.3
cd G:\Data\GitFiles\Keil\STM32_CubeIDE\FreeRTOS_Project
debugtui
```

固定安装本版时，将 `latest/download` 换为 `download/v1.1.3`；便携用户下载并完整解压 `debugtui-1.1.3-win-x64.zip` 后运行 EXE。工程继续使用现有 `debug.toml`，不需要重新生成配置。首次空目录启动仍创建通用模板，需自行选择 Probe、Chip、Debug cores 和 ELF。

暂停当前核心并使用物理 frame 0，在 **System Regs → Probe caps** 获取实际实现信息，再展开 **MPU**、**FPU configuration / identity** 或浮点组，按需使用 **Read**。展开组、字段和筛选会保存到工程，下次启动恢复。**Status** 查看完整状态、原因和来源；**Find / Group / Target / All** 决定显示范围，`Total` 是目录定义数量，不代表全部已读取或已上板验证。

`[Stale]` 表示上一次采样已经过期，**Read** 可刷新；`Not read`、`Not implemented`、`Unavailable` 等各有独立含义。灰色本身不能判断硬件有故障。查看 **MPU regions** 时会临时切换区域选择器并恢复；编辑寄存器使用 **Edit value → Preview → Apply**，**Cancel** 不发送写入。不要以本次 R0 验证推断所有状态、控制或浮点寄存器均可安全写入。

## 验证与范围

STM32F429/Cortex-M4、CMSIS-DAP、OpenOCD 上运行现有 FreeRTOS_Project 固件，验证实际 CPUID、12 个 MPU/FPU 配置寄存器与独立 GDB 内存读取一致、23 个 Core 与 49 个浮点定义（D/S 别名）的读值、8 个 MPU 区域与独立 Tcl 读取一致、RNR 恢复及 MPU.CTRL 不变、显示偏好保存、R0 临时写入与恢复、运行/暂停后的真实过期与重读。工程原始配置的 SHA256 保持不变。本次没有重新烧录固件。

实板响应另外送入实际 Rust UI 缓存与渲染器，验证有效、过期、刷新三种状态。此项属于实板数据回放，不是终端鼠标操作验收。全量 Rust 回归、格式/Clippy、发行打包、安装升级和公网附件验证的记录见 [TESTING.md](../TESTING.md) 及附件 `validation-1.1.3.json`。

本次实板范围是 STM32F429 单核；R52/多核的软件回归继续覆盖，未宣称 R52 实板、全部 386 个目录定义或全部寄存器写通道已通过硬件验收。内置 GDB/OpenOCD、芯片/SVD 资源保持原有版本，修改版 OpenOCD 的固定对应源码随 Release 提供。

## 发布附件

- `debugtui-cli.tgz` / `debugtui-cli-1.1.3.tgz`：npm 安装包。
- `debugtui-1.1.3-win-x64.zip`：包含工具、配置、SVD 和说明的便携包。
- `debugtui-windows-x64-1.1.3.exe`：独立程序，不包含工具资源。
- `corresponding-source.zip`：修改版 OpenOCD 的固定对应源码。
- `validation-1.1.3.json` / `SHA256SUMS.txt`：验证摘要与全附件校验和。

完整使用说明见 [USER_GUIDE.md](../USER_GUIDE.md)。
