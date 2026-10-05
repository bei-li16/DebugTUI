# R52 Timer 目录与读取路径自检

内置 R52/R52+ Timer 有十五项描述、字段和实现条件，独立数据验证全部 MRC/MRRC 路径。生产后端现增加当前 Debug state 权限、外部身份及 PC/状态保持检查；REG-401/403 的完整低 EL 与跨时间验收继续保留，上板用例未执行。

## 手册与目录

编码依据用户提供的 Cortex-R52 TRM `100026_0104_01_en`，表 11-1、PDF 第 380–381 页。访问等级核对完整 Armv8-R AArch32 supplement `DDI0568A.c ID110520` 表 E1-1，印刷页 E1-70/71；基础 Timer 字段和禁用时语义核对 `DDI0406C.b ID073012` B4.1.21–35、B8.1。CNTFRQ 的 EL0 访问同时核对 R52 TRM 表 4-15 的 CNTKCTL 位 0/1 条件。用户另一份 `Armv8-R AArch32.pdf` 实际为 26 页 R-Profile 介绍 `DEN0130_0100_en`，不作为完整架构寄存器定义。

下表为读编码；三十二位列为 `coproc op1 CRn CRm op2`，六十四位列为 `coproc op1 CRm`。CPU 通用目录不证明具体芯片或 R52+ 专有实现。

| 条目 | 位宽 | 编码 | 字段与访问要点 |
| --- | --- | --- | --- |
| CNTFRQ | 32 | `15 0 14 0 0` | ClockFrequency[31:0]；软件提供的 Hz，不是测量值；EL1/2 读、EL2 写，EL0 受 CNTKCTL 位 0/1 控制 |
| CNTKCTL | 32 | `15 0 14 1 0` | PL0PCTEN[0]、PL0VCTEN[1]、EVNTEN[2]、EVNTDIR[3]、EVNTI[7:4]、PL0VTEN[8]、PL0PTEN[9]；EL1/2 |
| CNTP_TVAL | 32 | `15 0 14 2 0` | 有符号 TimerValue[31:0]；物理 Timer，EL1/0 受上层控制 |
| CNTP_CTL | 32 | `15 0 14 2 1` | ENABLE[0]、IMASK[1]、只读 ISTATUS[2] |
| CNTV_TVAL | 32 | `15 0 14 3 0` | 有符号 TimerValue[31:0]；当前虚拟 Timer 视图 |
| CNTV_CTL | 32 | `15 0 14 3 1` | ENABLE[0]、IMASK[1]、只读 ISTATUS[2]；EL0 受 PL0VTEN 控制 |
| CNTHCTL | 32 | `15 4 14 1 0` | PL1PCTEN[0]、PL1PCEN[1]、EVNTEN[2]、EVNTDIR[3]、EVNTI[7:4]；Hyp/EL2 |
| CNTHP_TVAL | 32 | `15 4 14 2 0` | 有符号 TimerValue[31:0]；Hyp/EL2 |
| CNTHP_CTL | 32 | `15 4 14 2 1` | ENABLE[0]、IMASK[1]、只读 ISTATUS[2]；Hyp/EL2 |
| CNTPCT | 64 | `15 0 14` | 完整只读物理计数；EL1/0 受 CNTHCTL 与 CNTKCTL 控制 |
| CNTVCT | 64 | `15 1 14` | 完整只读虚拟计数；EL0 受 CNTKCTL 控制 |
| CNTP_CVAL | 64 | `15 2 14` | 完整物理比较值；权限同物理 Timer |
| CNTV_CVAL | 64 | `15 3 14` | 完整虚拟比较值；权限同虚拟 Timer |
| CNTVOFF | 64 | `15 4 14` | 完整虚拟计数偏移；Hyp/EL2 |
| CNTHP_CVAL | 64 | `15 6 14` | 完整 Hyp 比较值；Hyp/EL2 |

元数据独立保存在 [生成输入](../scripts/register_timer_metadata.py)；[生成器](../scripts/generate-register-catalogues.py) 的 `--check` 在临时目录重新生成三份目录并逐字节比较，不修改工程文件。仅 R52/R52+ 的 Timer 部分发生变化，M4 保持原内容。

## 实现、权限与数值含义

全部十五项要求 `timer.present` 为 1。缺少当前有效观察和声明时为 Unknown，不自动发送任何 Timer MRC/MRRC；声明为 0 时自动与手动均不请求。当前有效 R52 Probe 的观察可覆盖声明，原声明和原始能力证据仍保存；运行、换帧、切核或重连后不得复用。显式手动 Unknown 沿用既有读取规则，仍需操作者和已核验后端保证合法访问。本批增加的是实现条件和访问说明，尚未增加覆盖所有 EL1/Guest/User 场景的运行时权限规划器。

实现存在不表示本次可访问。CNTHCTL、CNTHP、CNTVOFF 只适用于 Hyp/EL2；CNTP、CNTV、计数器各有独立控制。配置目录、CNTFRQ 的零值、CTL.ENABLE/IMASK 和权限错误都不能独自证明 Timer 缺失。不可读原因仍与 Implementation 分开；原始错误原因未知时保持 Unknown，不猜测为权限或未实现。

读取门禁需要实际 Debug state 证据。完整 R52 supplement `DDI0568A.c` F1.3.3–4（F1-189）说明 EDSCR.HDD 取决于 Hyp invasive debug 授权，HDD=1 时低 EL 的 EL2 trap 会变为 UNDEFINED。新增核对工作区完整 `DDI_0487_M.b_a-profile_architecture_reference_manual.pdf` H2.4.5/H2.4.8（PDF 15006/15012）：Armv8 系统寄存器保持当前 EL 权限和 trap；额外特权针对调试操作与 DTR，不能把 Armv7 的 CP15 特权提升/忽略 Hyp trap 规则套到 R52。正常权限表可用于规划，但必须匹配当前 EDSCR.EL/RW/HDD，停止前 CPSR、配置 EL 标签和旧 Probe 不能代替当前状态。HDD=0 也不能证明 Timer trap 被绕过。

## 专用 Timer 后端

显式配置 `registers.timer_command="aarch64 timer"` 后，十五项按精确 CP15 编码接入独立 Timer 协议；名称相同但编码不同的客户条目不会被强行重路由，位宽矛盾在访问前拒绝。最小约束是 [后端源码锁](../tools/openocd-adapter/source.lock.json) 的 Timer 协议；`0.12.0` 版本字符串不能代替协议。原 MRC/MRRC 配置可为空；选中 Timer 协议后错误不回退 GDB/旧命令。未配置仍保留旧工程行为。

生产后端通过当前 core debug AP 读取 EDSCR 和 MIDR（R52 TRM Table 12-5 的 0x088/0xD00），各 AP 操作检查 EDPRSR.HALT，避免低 EL 试读可能 trap 的 MIDR。实际 Arm D13/AArch32 和当前 EL2 允许十五项；EL1、HDD=0/1 允许 CNTFRQ、CNTKCTL 与四项 CNTV。DDI0568A.c H1-255 排除 CRn/CRm=14 的 HSTR coarse trap。EL1 的物理 Timer 需要不可从 EL1 读出的 CNTHCTL，返回权限 Unknown；Hyp-only 项为 Access restricted。EL0 对 CNTKCTL/Hyp-only 明确拒绝，其余上层 enable 未知时不注入 Timer 指令，不冒充硬件缺失或已证明 trap。完整低 EL 受控访问和 R52+ 实际身份仍待适配。

前后比较完整 DSPSR、DLR、外部身份与 EDSCR 状态，逐项保存/恢复/物理回读暂存 R0/R1。六项六十四位值各只有一次 MRRC；只在全部检查成功后发布固定宽度值，任一失败点立即停止并标记 unknown。没有 DCPS/DRPS、Timer/control 写入或旧异常恢复。每个成功 sample 的 `provenance.access.timer` 保存完整 MIDR/EDSCR/DSPSR/DLR；headless 与详情窗口可查，旧值保留自身的证据。服务锁、实际线程/物理 frame 0 前后校验、target 恢复、取消丢弃及 Scope All 选定核限制沿用生产适配器路径。

`tools/openocd-adapter/tests/timer-transfer.c` 编译生产头文件：独立十五项指令字、474 个失败点、12 个合法 EL1/HDD 组合、48 个拒绝、暂存恢复及状态/PC/身份变化。Windows/Linux 在全新目录构建并完成真实离线命令检查，Windows 还完成原生依赖/配置/源码包检查；全部使用 dummy 或配置期检查，未连接探针。候选证据分别为 `artifacts/openocd-timer-windows-native-final.log`、`artifacts/openocd-timer-linux-build.log` 和 `.dev/openocd-windows-timer/windows-tests/report.json`，候选尚未安装或发布。

TVAL 是三十二位有符号差值 `(CompareValue - counter)[31:0]`，可选择有符号格式查看，不能当成六十四位计数器。ENABLE=0 时 TVAL 读值和 ISTATUS 都是架构 UNKNOWN；目录保留原始位并说明限制，不为 ISTATUS 设置可能误导的条件枚举。Valid 仅说明完整原始值读取成功，不证明禁用 Timer 的这些位有有效含义。IMASK 独立于 ISTATUS。CVAL/计数/offset 的字段保留全部六十四位；不同条目分别采样，不能把它们视为同一时刻的原子快照，也不能从先后读取的 CNTPCT/CNTVCT 推导精确 CNTVOFF。

## 软件证据与自检

| 验证 | 实际检查 |
| --- | --- |
| `timer_adapter_routes_follow_encodings_and_require_independent_opt_in` / `timer_wire_rejects_truncation_and_unproven_current_el_permissions` | 独立 opt-in、严格命令、旧配置兼容、全部精确编码/位宽；完整高字、实际身份/当前 EL/RW/HDD/错误位、截断与矛盾证据拒绝；停止 DSPSR 模式不作为当前 EL |
| `timer_adapter_reads_all_native_widths_with_current_debug_evidence_and_single_owner` | 实际协调器/MI/Tcl，双核 Scope All 只读选定 core1，十五项不对称原始值、实际物理证据、owner/命令/target 恢复、旧 context 拒绝；不回退 MRRC |
| `timer_adapter_keeps_guest_permission_unknown_separate_from_retained_physical_evidence` | EL1/HDD 夹具下虚拟计数可读，物理权限 Unknown 与 Hyp-only 拒绝分开；失败不表示缺失，保留原值及原 EDSCR 来源 |
| `timer_adapter_rejects_protocol_identity_permissions_and_forged_or_truncated_evidence_without_fallback` | 错协议、身份拒绝、权限两种原因、截断/伪造 EL 各有完整断言；后端恢复未知进入 FAULT、停止后续访问与 Continue，均不回退 |
| `cancelled_timer_transaction_restores_target_and_discards_samples_without_retry` / `timer_context_change_discards_physical_value_and_stops_the_batch` | 原子事务后取消丢弃新样本；读取期间实际线程/帧变化丢弃物理原始值并停止批次，target 仍恢复；无重试 |
| `timer_catalogues_match_r52_trm_widths_encodings_fields_and_independent_access` | 两份内置目录全部十五项独立编码、位宽、字段、只读覆盖及 min/max=1；零、缺失、越界实现状态；无 Timer writer |
| `timer_fields_keep_signed_tval_full_high_words_and_disabled_status_semantics` | 三份 TVAL 的 0/最大正数/最小负数/-1，三个 CTL 的全部位组合、禁用说明和 ISTATUS 只读；三个 CVAL 的高位完整 |
| `timer_unknown_presence_never_dispatches_automatic_mrc_or_mrrc_reads` | 实际 worker 在 Unknown 条件下全部十五项 Not read，未访问目标、不创建原始值或请求来源 |
| `all_timer_routes_preserve_unknown_presence_exact_encodings_errors_and_selected_core` | 实际协调器/worker、MI 进程和 Tcl/TCP 夹具；Scope All 选 core1，两种声明起点，十五个独立不对称值；每项恰好一个正确编码请求、完整高位及 owner/target，恢复外部 target；一个错误不阻止后续计数器读取；Status 保留旧值及原条件依据；新 stop 的 Unknown/No 不发命令，旧 context 被拒绝 |

旧目录在 Unknown 下仍尝试读取的失败复现保存在 `artifacts/register-timer-before.log`。新集成测试开发中曾错误预期 `error` 而非 `unavailable`、在本次响应而非 Status 中寻找保留值、单步后未等待停止；失败日志分别保留为 `register-timer-worker-before.log`、`register-timer-worker-response-before.log`、`register-timer-worker-stop-before.log`。修正测试以符合生产协议，保留完整断言，未修改运行时代码迎合测试。最终聚焦结果见 `artifacts/register-timer-model.log`、`artifacts/register-timer-worker.log`，完整回归和严格 Clippy 结果见 [开发进度](registers-development-status.md)。

首次完整回归 `artifacts/functional-1791199829590-2342083e/report.json` 发现旧按需读取测试只声明 GDB 中的 CNTPCT 名称和字节，却未声明 Timer 已实现。新增实现条件因此正确阻止自动读取。该已知可读软件夹具现在显式声明 `timer.present=1`，保留原来的请求数量、完整六十四位和缓存失效断言；独立 Unknown 测试继续核对零请求。失败报告保留，修复后重新完整验证。

`REG-H05` 延后驱动现先核对专用 Hyp 固件基线的 CPSR 与 ready，再读选定核/peer 控制；错误模式或未就绪时不发 Timer、控制或 Probe TCL。独立 C 钩子先用 MRS 核对正常执行 Hyp，在未切模式条件下采样九项 MRC、六项 MRRC，保存全部十五项原始值；动态 TVAL 不作为稳定比较值，禁用语义仍为 UNKNOWN。示例 case 为 CNTP_CVAL、CNTV_CVAL、CNTVOFF、CNTHP_CVAL 指定四个独立固件符号；驱动分别核对该符号、完整期待值和实际六十四位读值，计数器另按记录区间比较。符号仅允许变量或固定数组元素，不调用目标函数。该 Hyp 限定是固件基线流程的前置条件，不冒充应用的通用 Debug state 权限判断。

`deferred_timer_driver_runs_actual_binary_with_independent_fixture_baseline_and_peer` 以实际 EXE/真实 Tcl 软件夹具分别验证旧 MRRC 与专用 Timer 两种双核流程；`require_timer_adapter=true` 要求当前物理 EL2、停止 Hyp 与完整 MIDR/EDSCR/DSPSR/DLR，证据随各次请求写入报告。原来的独立基线核对继续验证四个不对称稳定值、两个计数器、peer/target/配置保持，共六阶段通过。`timer_baseline_driver_rejects_wrong_mode_unready_and_independent_reference_mismatch` 分别验证 SVC、ready=0、独立固件值与期待值不同，在正确 STOP/WIDTH 阶段失败并正常清理；前两类没有任何 TCL 目标访问。测试最初在单核夹具中错误设置多核 control_scope，失败保留为 `artifacts/register-timer-baseline-fixture-before.log`，移除该单核配置后保留原断言通过；最终聚焦日志 `artifacts/register-timer-baseline-focused.log`。驱动默认仍只输出五项 SKIPPED，不连接目标。

固件使用 WSL 的 GNU Arm `arm-linux-gnueabi-gcc` 11.4.0，以 `-mcpu=cortex-r52 -marm -ffreestanding -std=gnu11 -Wall -Wextra -Werror -O0 -g -c` 离线编译。GNU objdump 的实际指令字去掉编译器选择的 Rt/Rt2 后，与独立手写的九项 MRC、六项 MRRC 编码逐项一致；确认 MRS CPSR 先于 Timer 访问，且没有 MCR/MCRR/MSR。对象、反汇编及源 SHA256 记录在 `artifacts/register-timer-firmware-report.json`，反汇编为 `artifacts/register-timer-board-disassembly.log`。该对象未链接或执行；实际固件应使用自己的已审核工具链、启动配置和每核存储，编译不能证明芯片权限或读取值。

参考值全部存储后才执行 DMB 并发布 ready；DMB 不是 cache clean。实际 GDB/CPU/AP 读取参考 RAM 的一致性仍需独立核验，不能把软件夹具的变量值当作缓存可见性证明。单核 case 去掉 peer_core/control_scope；多核 case 为各核提供独立参考存储及符号，不让 peer 覆盖所选核基线。

[十三项环境 case](../tests/cases/register-timer.md) 均 SKIPPED。本批按不执行上板、准备相应用例的任务范围完成 REG-402：六项真正 MRRC 及最低独立协议均有生产 C/候选/运行时证据。REG-401/403 的完整 EL0/EL1 物理权限与采样一致性、实际 GDB 目标描述位宽、REG-208 真实终端、其余系统及 writer、最终安装/Release 仍待完成；当前已完成／未完成 **21/50**。没有把软件模型或 SKIPPED 当作芯片成功。

本批最终完整回归为 **367 单元＋136 集成通过，2 ignored**，F24 **114/114**，耗时 344536 ms，无超时；其余 22 功能套件未选择。报告 [`artifacts/functional-1791205816814-cf34d3cc/report.json`](../artifacts/functional-1791205816814-cf34d3cc/report.json)，完整 Cargo 为同目录 unit.log。仅调整 F24 的来源文件列表和限制说明后，逐项核对当前 manifest 与该完整回归的 114 模式一致，补充证明 `artifacts/register-timer-adapter-f24.json`。严格 Clippy 日志 `artifacts/register-timer-adapter-clippy.log`；其首次发现测试中多余借用，失败保留 `artifacts/register-timer-adapter-clippy-before.log`，删除多余借用后严格检查及两项单元复测通过（`artifacts/register-timer-adapter-unit-final.log`），未改变生产行为。早期新夹具 selector 配置、目录分组断言错误日志保留 `artifacts/register-timer-adapter-worker-before.log` 与 `artifacts/register-timer-adapter-unit-before.log`；最终聚焦日志 `artifacts/register-timer-adapter-focused.log`。
