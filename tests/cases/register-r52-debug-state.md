# R52 当前 Debug 状态位的延后硬件 case

本 Goal 不执行上板。以下四项全部 **SKIPPED**；软件模型通过不改变此状态，也不升级 hardware verified。开始前使用客户工程副本和专用正常固件，记录真实 R52 revision、每核 AP/target/endpoint、独立状态基线、固件/项目/EXE/OpenOCD 的版本与 SHA256。实际后端须已完成本版构建及命令入口验收；本批未构建的新补丁不能冒充已安装工具。

独立预期来自 `G:\Data\GitFiles\ARM\File\Arm® Cortex®-R52 Processor Technical Reference Manual.pdf` 和 `G:\Data\GitFiles\ARM\File\ARM Architecture Reference Manual Supplement - ARMv8, for the ARMv8-R AArch32 architecture profile.pdf` 的 G2.1.8。原目标参考仍包括 `G:\Data\GitFiles\DebugTUI\tui-debugger-spec.md` 与 `G:\Data\GitFiles\ARM\File\Armv8-R AArch32.pdf`。按实际板级映射取得外部 EDSCR，不猜测 AP 或 dbgbase；不能用 DebugTUI 的读值复制为独立期望。

| ID | 步骤与独立预期 | 状态 |
| --- | --- | --- |
| R52-STATE-H01-RES1 | 使用已确认当前 Debug EL2 的独立外部 EDSCR 基线；分别运行现有 Banked/VFP/Timer/PMU/GIC 延后驱动并读取无副作用、条件满足的代表项。响应保留真实 EDSCR，bit16/18 为1不导致受限；数据与独立固件/另一调试器基线一致。模块未实现或未使能时按原 case 拒绝，不自动使能 | SKIPPED |
| R52-STATE-H02-SAVED-STATE | 专用固件在 User 或 EL1 正常停止，独立核对实际 Debug EL、HDD 与保存 DSPSR。已证明 EL2 时读取合法项，保存的低 EL 不构成拒绝依据；无法证明当前权限时在数据指令前 Unknown/Restricted，不在调试态切换模式或改权限 | SKIPPED |
| R52-STATE-H03-LOWER-EL | 使用预先安排的正常固件及板级调试授权环境获得较低 Debug EL/HDD，记录独立真实状态。逐类核对原支持边界：PMU/GIC 的低 EL 保持未知，Hyp 访问不授权；已有合法 EL1 Timer 视图按独立访问规则检查。不得在 EL2 人为写只读 HDD，也不把“全部拒绝”当成功路径证明 | SKIPPED |
| R52-STATE-H04-MULTICORE | 两核独立获取外部状态及数据基线，交替 `select_core`、`registers_list`、`registers_read`，请求绑定当时 context。五类证据保留各自核/target/endpoint/时间，另一核状态与控制不变；过期请求不得覆盖当前核值，失败保留原样本来源并标为旧值 | SKIPPED |

使用现有实际接口和相应类别驱动，显式传入 `--run` 及板级参数后才执行；默认检查保持零目标 I/O。每项保存请求/响应、MI/Tcl 日志、独立基线、pass/fail/skipped 与工具摘要。运行结束检查目标控制、scratch/selector、项目文件与原基线一致；恢复不确定时停止注入并保留 FAULT/隔离，按该板的既定恢复流程处理，不盲目重试。

HDD 在同一次正常 Debug state 内不改变。软件 C 故障模型的 HDD 变化用例检验损坏/不一致证据隔离，不要求在实板制造不符合架构的状态。
