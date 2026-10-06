# R52 EDSCR 的 R-profile 定义与当前 Debug 状态

2026-10-06。本批修复已有 Banked、VFP、Timer、PMU、GIC 证明中的架构位定义；是 C03 的前置子批次，C03～C06 仍待完整生产验收。实时 feature 状态与最终证据见 [36 项账本](registers-readonly-goal.md)，本批不增加完成数。

## 来源与缺陷

三个核心参考的绝对路径保留：

```text
G:\Data\GitFiles\DebugTUI\tui-debugger-spec.md
G:\Data\GitFiles\ARM\File\Arm® Cortex®-R52 Processor Technical Reference Manual.pdf
G:\Data\GitFiles\ARM\File\Armv8-R AArch32.pdf
```

指定架构介绍是 `DEN0130_0100_en`，不能代替 External Debug 详细定义。本批规则来自以下实际补充手册的 `ARM DDI 0568A.c`、G2.1.8、PDF 物理页 213/215：

```text
G:\Data\GitFiles\ARM\File\ARM Architecture Reference Manual Supplement - ARMv8, for the ARMv8-R AArch32 architecture profile.pdf
```

| EDSCR 字段 | R-profile 定义 | 本批处理 |
| --- | --- | --- |
| HDD[15] | Hyp debug disabled | 当前 EL2 且 HDD=1 不发布数据；较低 EL 保留各 reader 已有权限规则 |
| bit 16、bit 18 | RES1 | 合法的 1 不得作为访问限制；不套用 Armv8-A 的 SDD 定义 |
| EL[9:8] | Debug state 下 PE 的当前 Exception level | 与停止前 DSPSR.M 分开；实际访问条件仍需逐类证明 |
| ITE[24]、错误状态 | 指令完成和调试错误 | 沿用完成/故障检查；不把 halt 单独当成权限 |
| TXfull[29]、RXfull[30] | DTR 传输状态 | 允许正常传输造成变化，不能误判为 EL 或授权变化 |

旧实现使用 bit 16 检查 Hyp 调试限制，因而会拒绝带真实 RES1 位的 R52 响应，并漏掉 HDD[15]。旧样本 `0x01000200` 未带 RES1 位，无法暴露这个问题。手册独立构造的 `0x01050213` 包含 ITE、RES1[18,16]、EL2 和 External debug request 状态；`0x01058213` 增加 HDD[15]，不能作为合法 EL2 访问证明。

## 实现与验证边界

Rust 五类现有 physical proof parser 复用 `registers::r52_debug::current_el`，严格限制原始值宽度、R52 身份、EL、完成与故障状态，并使用 HDD[15]。各 reader 的 HCPTR、容量、trap 和数据宽度等校验继续独立执行。该函数是当前状态解码，不能单独授权任意 CP15 指令；没有新增低 EL 支持或 writer。

OpenOCD 的 Banked/Timer 状态掩码改为 `0x0005bf00`，PMU/GIC 的拒绝分类使用 HDD[15]。实际 DPM 的十处状态捕获/比较加入 HDD[15]，保留原有执行状态检查。正常注入仍由 OpenOCD 承担，TUI 不实现第二套指令注入。Python 延后 VFP writer 驱动仅修复同一证明位和稳定性掩码。

新增单元覆盖真实 RES1、HDD、传输标志、身份/宽度/完成/故障，以及五个实际解析器。实际 MI/TCP/Tcl worker 测试核对五类成功值的原始 EDSCR 与保存的 User DSPSR，并验证 PMU/GIC 的 HDD 拒绝不会覆盖旧成功值及来源。GIC/PMU Tcl 夹具同步修正；没有放宽 parser 位宽或把失败改为跳过。

七个原生 C 套件编译并执行实际生产事务头文件，仅物理 I/O 是模型；正向基线带真实 RES1，权限反例使用 HDD[15]。补充 EL0 Banked 与 EL1 Timer 的事务内 HDD 变化用例：两种稳定值原本均合法，但前后变化仍须拒绝并保留未知恢复状态。这是故障注入模型；手册说明 HDD 在正常 Debug state 内保持不变，不能把模型中的变化当成正常硬件行为。

## 构建与未完成工作

本批生产源补丁摘要为 `2e59520c6de50aa349efd7b0c8044ef08c01122a7cb7484b41eae9580b276133`，固定 OpenOCD revision 为 `d3ebb8d2b9adbfd9a13072e8e446f424b5ff3c0e`。十一项 patched source 摘要与测试源码逐项匹配。

**本批尚未构建新的完整 OpenOCD 候选**，`source.lock.json` 的 Windows/Linux build verified 为 false、candidate SHA256 为 null；旧候选只记录在 previous_candidates。原生事务测试不是实际后端命令入口验收，报告 `backend_commands_passed=false`。前序说明中的候选构建与命令通过只适用于其当时的旧补丁。

普通 CP15 reader 仍未接通新鲜当前 Debug 证明，R52 MPU 和 selector 仍使用保存的 CPSR。后续 C03 必须在同一受保护事务接通三条生产成功/拒绝路径，C04/C05 完成恢复与 selector 整体验收，再对稳定的后端源码构建和验证现有平台候选。未发布 Release，未上板，也不升级 hardware verified。

[延后硬件 case](../tests/cases/register-r52-debug-state.md) 全部 SKIPPED；软件测试日志、适用源码与报告摘要在账本迭代 13 中记录。
