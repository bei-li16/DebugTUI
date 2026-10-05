# R52 Timer 目录与读取路径自检

本批补齐内置 R52/R52+ Timer 的十五项描述、字段和实现条件，并以独立数据验证全部 MRC/MRRC 路径。属于 REG-401/402/403 的阶段性软件证据，三项完整任务仍未勾选；实际权限、后端指令执行及采样一致性的其余验收继续保留。

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

上述 EL 表是正常执行权限，不能直接作为 Debug state 的读取门禁。完整 R52 supplement `DDI0568A.c` F1.3.3–4（F1-189）说明 EDSCR.HDD 取决于 Hyp invasive debug 授权，HDD=1 时低 EL 的 EL2 trap 会变为 UNDEFINED；基础 `DDI0406C.b` C5.3.3/C5.5 另区分 Debug state 的 CP14/15 权限及异常行为。停止前 CPSR、目录 EL 标签和正常运行 trap 不能独自证明调试指令访问。当前目录前缀已明确正常执行规则与后端 Debug state 证据的区别；完整运行时权限适配仍需实际执行等级、EDSCR.HDD/授权及相应控制的生产后端证据，不能用配置或旧 Probe 猜测为允许，也不能一律按普通 EL0 权限拒绝合法的 Debug state 操作。

TVAL 是三十二位有符号差值 `(CompareValue - counter)[31:0]`，可选择有符号格式查看，不能当成六十四位计数器。ENABLE=0 时 TVAL 读值和 ISTATUS 都是架构 UNKNOWN；目录保留原始位并说明限制，不为 ISTATUS 设置可能误导的条件枚举。Valid 仅说明完整原始值读取成功，不证明禁用 Timer 的这些位有有效含义。IMASK 独立于 ISTATUS。CVAL/计数/offset 的字段保留全部六十四位；不同条目分别采样，不能把它们视为同一时刻的原子快照，也不能从先后读取的 CNTPCT/CNTVCT 推导精确 CNTVOFF。

## 软件证据与自检

| 验证 | 实际检查 |
| --- | --- |
| `timer_catalogues_match_r52_trm_widths_encodings_fields_and_independent_access` | 两份内置目录全部十五项独立编码、位宽、字段、只读覆盖及 min/max=1；零、缺失、越界实现状态；无 Timer writer |
| `timer_fields_keep_signed_tval_full_high_words_and_disabled_status_semantics` | 三份 TVAL 的 0/最大正数/最小负数/-1，三个 CTL 的全部位组合、禁用说明和 ISTATUS 只读；三个 CVAL 的高位完整 |
| `timer_unknown_presence_never_dispatches_automatic_mrc_or_mrrc_reads` | 实际 worker 在 Unknown 条件下全部十五项 Not read，未访问目标、不创建原始值或请求来源 |
| `all_timer_routes_preserve_unknown_presence_exact_encodings_errors_and_selected_core` | 实际协调器/worker、MI 进程和 Tcl/TCP 夹具；Scope All 选 core1，两种声明起点，十五个独立不对称值；每项恰好一个正确编码请求、完整高位及 owner/target，恢复外部 target；一个错误不阻止后续计数器读取；Status 保留旧值及原条件依据；新 stop 的 Unknown/No 不发命令，旧 context 被拒绝 |

旧目录在 Unknown 下仍尝试读取的失败复现保存在 `artifacts/register-timer-before.log`。新集成测试开发中曾错误预期 `error` 而非 `unavailable`、在本次响应而非 Status 中寻找保留值、单步后未等待停止；失败日志分别保留为 `register-timer-worker-before.log`、`register-timer-worker-response-before.log`、`register-timer-worker-stop-before.log`。修正测试以符合生产协议，保留完整断言，未修改运行时代码迎合测试。最终聚焦结果见 `artifacts/register-timer-model.log`、`artifacts/register-timer-worker.log`，完整回归和严格 Clippy 结果见 [开发进度](registers-development-status.md)。

首次完整回归 `artifacts/functional-1791199829590-2342083e/report.json` 发现旧按需读取测试只声明 GDB 中的 CNTPCT 名称和字节，却未声明 Timer 已实现。新增实现条件因此正确阻止自动读取。该已知可读软件夹具现在显式声明 `timer.present=1`，保留原来的请求数量、完整六十四位和缓存失效断言；独立 Unknown 测试继续核对零请求。失败报告保留，修复后重新完整验证。

`REG-H05` 延后驱动现先核对专用 Hyp 固件基线的 CPSR 与 ready，再读选定核/peer 控制；错误模式或未就绪时不发 Timer、控制或 Probe TCL。独立 C 钩子先用 MRS 核对正常执行 Hyp，在未切模式条件下采样九项 MRC、六项 MRRC，保存全部十五项原始值；动态 TVAL 不作为稳定比较值，禁用语义仍为 UNKNOWN。示例 case 为 CNTP_CVAL、CNTV_CVAL、CNTVOFF、CNTHP_CVAL 指定四个独立固件符号；驱动分别核对该符号、完整期待值和实际六十四位读值，计数器另按记录区间比较。符号仅允许变量或固定数组元素，不调用目标函数。该 Hyp 限定是固件基线流程的前置条件，不冒充应用的通用 Debug state 权限判断。

`deferred_timer_driver_runs_actual_binary_with_independent_fixture_baseline_and_peer` 以实际 EXE/真实 Tcl 软件夹具验证四个不对称稳定值、两个计数器、peer/target/配置保持，共六阶段通过。`timer_baseline_driver_rejects_wrong_mode_unready_and_independent_reference_mismatch` 分别验证 SVC、ready=0、独立固件值与期待值不同，在正确 STOP/WIDTH 阶段失败并正常清理；前两类没有任何 TCL 目标访问。测试最初在单核夹具中错误设置多核 control_scope，失败保留为 `artifacts/register-timer-baseline-fixture-before.log`，移除该单核配置后保留原断言通过；最终聚焦日志 `artifacts/register-timer-baseline-focused.log`。驱动默认仍只输出五项 SKIPPED，不连接目标。

固件使用 WSL 的 GNU Arm `arm-linux-gnueabi-gcc` 11.4.0，以 `-mcpu=cortex-r52 -marm -ffreestanding -std=gnu11 -Wall -Wextra -Werror -O0 -g -c` 离线编译。GNU objdump 的实际指令字去掉编译器选择的 Rt/Rt2 后，与独立手写的九项 MRC、六项 MRRC 编码逐项一致；确认 MRS CPSR 先于 Timer 访问，且没有 MCR/MCRR/MSR。对象、反汇编及源 SHA256 记录在 `artifacts/register-timer-firmware-report.json`，反汇编为 `artifacts/register-timer-board-disassembly.log`。该对象未链接或执行；实际固件应使用自己的已审核工具链、启动配置和每核存储，编译不能证明芯片权限或读取值。

参考值全部存储后才执行 DMB 并发布 ready；DMB 不是 cache clean。实际 GDB/CPU/AP 读取参考 RAM 的一致性仍需独立核验，不能把软件夹具的变量值当作缓存可见性证明。单核 case 去掉 peer_core/control_scope；多核 case 为各核提供独立参考存储及符号，不让 peer 覆盖所选核基线。

[十二项环境 case](../tests/cases/register-timer.md) 均 SKIPPED。REG-401/402/403 完整 Debug state/EL1/Guest/User 权限与一致性、实际 GDB 目标描述位宽/最低后端约束、REG-208 真实终端、其余系统及 writer、最终安装/Release 均继续待完成，已完成／未完成仍为 **20/51**。
