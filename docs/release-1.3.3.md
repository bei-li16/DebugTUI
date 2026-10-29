# DebugTUI 1.3.3：寄存器访问策略与 F429 受控读取

Windows x64 正式版本，标签 `v1.3.3`，从 `main` 发布。基于 1.3.2（`24959e5`）整合寄存器策略与 Watch 修复，源码、Cargo 锁文件、npm 元数据及 EXE 版本统一为 1.3.3。

## 变更

- 通用 Host 支持 `access_rule.requires` 与 `access_rule.deny`，自动读取、手动读取、Probe、别名和矩阵共用访问门槛。`Access restricted` 单独分类和统计；显式禁止项在发出 I/O 前拒绝。
- F429 ETM LAR 保持禁止访问，分类修正为访问受限。DWT PCSR、ITM LSR 在 CPUID、ROM 表项和当前 DEMCR.TRCENA 观测满足条件时受控只读；读到值仍保留实现未知，不能推导为 PC 采样存在或软件锁状态已证明。
- MVFR2 与 SCS PID5～7 继续保留证据不足说明及未知保护。描述展开为 617 个定义、1466 个读取位域，既有 32 个写入位域和 16 个整数 GDB writer 未扩展。
- Watch 解析取消或上下文变化后，停止目标读取，继续删除临时 GDB variable object 并恢复 `may-call-functions` 策略。
- R52 GIC 离线回归使用随测试源码提交的、标明来源和局限的审计快照，去除对本地忽略产物的依赖。
- 固件一致性检查直接分块读取只读 Flash 节，避免 GDB `compare-sections` 的目标 CRC 路径使用应用 RAM。

Host 保持通用，F429 地址、能力证据及受控读取条件位于 TOML。新增字段需要新版 EXE 与描述成套使用，官方 1.3.2 EXE 不支持这些字段。无专用芯片描述时仍可使用 Generic GDB 的通用寄存器。

## 本地验证

上一轮整合的软件基线为 913 项 Rust 通过、3 项原有忽略，严格 Clippy、Host 7 项、多 ELF 10 项和分发 14 项通过。该基线报告见 `artifacts/stash-merge-latest-20261029/`；本轮通过 SHA256 核对对应源码、描述和测试内容，不将旧二进制的测试改记为新二进制实测。

本轮 1.3.3 Release 编译、软件验证及 STM32F429 / FreeRTOS_Project 实板结果记录在 `artifacts/release-candidate-1.3.3-20261030/REPORT.md`，以该报告的最终结果、EXE SHA256、固件摘要及未测范围为准。

发布前以经过 SHA256 校验的真实 1.3.2 包执行分发回归，**14/14 通过**：完整包内容、CMD/PowerShell 入口、芯片与每核目录解析、真实安装钩子、升级/重复安装/卸载及包外用户配置保留。最终发布包和公网 Latest 安装验证的结果汇总于 Release 附件 `validation-1.3.3.json`；安装验证采用隔离 prefix，不替换维护者已有的全局安装。

2026-10-30 实测结果：

| 验证 | 结果 |
| --- | --- |
| 新 Release EXE | `debugtui 1.3.3`；SHA256 `29a4cb41a843d1e99f2d95469a89da3d2eb84c7bb2598220dc30a59c47bd9ae8` |
| Host / 多 ELF / 固件比较辅助 | 分别 7 / 10 / 4 项通过 |
| FreeRTOS 基础调试 | 17 通过、0 失败、1 跳过；固件下载未执行 |
| F429 寄存器与保护 | 8 项通过，617 定义扫描：425 Valid、16 Not read、169 Unsupported、7 Unavailable，无读取错误 |
| SCB / NVIC / SysTick | SCB 31 项全部有效，合计 37 项与独立 OpenOCD 读值一致 |
| 压力及写入保护 / 源码 step、finish | 分别 8 / 4 项通过，含 30 次继续/暂停 |
| 快速连续搜索 | 真实终端 3 轮、36 项检查通过，搜索输入间隔为 0，未卡在 `registers_read` |
| 原生界面 | SCB/CPACR 位域、窗口缩放、Probe/访问受限统计及 DWT/ITM/ETM/TPIU 显示通过 |

Probe 为 41 定义、39 唯一请求、39 响应、2 no I/O。七项 Unavailable 分别为六项未启用的 ETM/TPIU 集成测试项与禁止访问的 ETM LAR；未为测试开启集成模式或解除保护。PCSR `0xffffffff`、ITM LSR `0x00000001` 与独立读取一致，仍保留实现未知；不据此断言有效 PC 采样或软件锁已解锁。

原工程、ELF、SVD 和工具 profile 摘要保持不变；测试后独立 OpenOCD 检查确认目标恢复运行。本轮未重建/下载 FreeRTOS 固件、未执行应用 RAM 或系统寄存器写入，未验证 Trace 输出及 THA/R52 实板。

## 安装与使用

退出现有 DebugTUI 后，在 PowerShell 执行：

```powershell
npm.cmd install -g --prefer-online --ignore-scripts=false --foreground-scripts --allow-scripts=@debugtui/cli "https://github.com/bei-li16/DebugTUI/releases/latest/download/debugtui-cli.tgz"
debugtui --version
# debugtui 1.3.3
```

固定本版可将 `latest/download` 换成 `download/v1.3.3`。便携用户下载完整 ZIP；单独 EXE 需要配套工具、描述文件与许可证。项目和包外用户覆盖配置在升级时保留。

源码工作区的验证 EXE 保留于 `bin/debugtui.exe`，可在仓库中运行：

```powershell
.\bin\debugtui.exe --version
.\bin\debugtui.exe --project "G:\Data\GitFiles\Keil\STM32_CubeIDE\FreeRTOS_Project\debug.toml"
```

Release 同时提供完整 ZIP、独立 EXE、版本化及固定名称 npm tgz、PDF/TeX 手册、`validation-1.3.3.json`、`corresponding-source.zip` 与 `SHA256SUMS.txt`。EXE 摘要与上板验证一致，OpenOCD 对应源码继续匹配随包 PROVENANCE；公网安装验证使用隔离 prefix，不修改维护者的全局安装。
