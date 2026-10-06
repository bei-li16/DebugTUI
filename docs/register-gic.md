# R52 GIC 能力与只读访问自检

当前补充：[R52 EDSCR 修正](register-r52-debug-state.md) 使用 HDD[15] 并接受 RES1[16]；本文后续构建证据属于旧补丁，新候选仍待构建验证。

2026-10-06，源码基线 0.9.3，开发分支 `codex/register-debugging`。本批覆盖 REG-408 的软件要求与 REG-405 的系统寄存器子集。按本任务要求不执行上板；十项环境 case 与可执行驱动已准备，均不能计为物理验证。完整 Debug、GIC Distributor/Redistributor MMIO、低 EL ICV 和 R52+ 实际身份仍未完成。

## 手册依据与接口

依据工作区 `G:/Data/GitFiles/ARM/File/Arm® Cortex®-R52 Processor Technical Reference Manual.pdf`（100026_0104_01_en）：表 10-69（PDF 322–323）给出 ICH 的 AArch32 编码；表 10-71（327）中 ICH_VTR=`0x90180003`，虚拟优先级/抢占位均为五，列表寄存器为四；表 10-80（338）列出 ICC 编码；表 10-94（350）中 ICC_CTLR.PRIbits 为五位，仅 CBPR/EOImode 两个低位可变。§10.3.5（361–362）区分 EL1 的物理/虚拟重定向及 EL2 的物理接口。

工作区 `Arm Generic Interrupt Controller (GIC) Architecture Specification, v3 and v4.pdf` 的 AArch32 ICC_AP0R 说明（PDF 370）给出五/六/七抢占位下的 AP 索引实现条件，未实现项是 UNDEFINED。架构夹具独立检验这些条件；当前 R52 原生适配只接受手册确认的五位容量，拒绝伪造的六/七位响应。ICH_VTR 的虚拟容量不能用来确认物理 ICC 容量，停止前 CPSR.Hyp 也不能证明当前 Debug EL 或读取权限。

用户提供的 `Armv8-R AArch32.pdf` 是架构概览；权限细节另核对工作区完整 DDI0568A.c 与 DDI0487 M.b H2.4.5/H2.4.8。Debug state 系统指令保持当前 EL 和适用陷阱，HDD=0 不能解释成全部 trap 豁免。实现不引用 ADS 资源；环境用例允许同核同停止点的 ADS 原始值作为独立对照。

| 目录/视图 | 数量与来源 | 本批读取边界 |
| --- | --- | --- |
| 物理 ICC AP | ICC_CTLR.PRIbits，本次原生 physical_icc 证明 | R52 AP0R0/AP1R0；索引 1–3 为 No，禁止数据试读 |
| ICH AP / ICV backing | ICH_VTR.PREbits，本次原生 hypervisor_ich 证明 | R52 ICH_AP0R0/AP1R0；ICV AP 显式 alias 同一 backing，不冒充 EL1 ICV 读取 |
| ICH LR/LRC | ICH_VTR.ListRegs+1 | 四项 LR 与四项 LRC 各为独立 MRC32，绝不当作 MRRC64 或原子合并值 |
| ICC IAR0/IAR1 | 架构只读但会 acknowledge | 自动零请求；观测后端手工也拒绝，不确认中断 |
| EOIR0/1、DIR、SGI0R/1R/ASGI1R | 写专用元数据 | 无 reader 请求、无 GIC writer、不 enable/deactivate/send SGI |

`register_gic_metadata.py` 管理四十三项原生编码目录、八项 ICV AP alias 和六项 WO 说明。四十三条路由中二十九条可观测、十二条额外 AP 在 R52 未实现、两条 IAR 有副作用。R52/R52+ 目录同步生成，目录名并不扩展实际后端身份。位域、访问属性、说明与实现条件随目录交付。

## 显式配置与新鲜容量

仅在已审核的专用后端和对应 target/endpoint 上显式配置：

```toml
[registers]
gic_command = "aarch64 gic"
```

默认空值保持原路径；这段是开发功能示例，不代表当前正式 v0.9.3 安装的 OpenOCD 已具备协议。配置必须结合现有工程的每核 targets 和 Tcl 服务。读取按原始 CP15 编码与 32 位宽精确路由，不以客户自定义名称截获其他编码。选择专用通道后，无论协议、身份、权限还是传输失败，都不回退 GDB 或普通 MRC/MRRC。

独立握手为 `debugtui-armv8-gic-1 external-identity current-el fresh-capacity physical-icc hyp-ich no-ack no-enable stop-on-fault`。每项请求从当前物理 target 的外部 AP 读取 MIDR/EDSCR，确认 Arm/D13/AArch32、当前 Debug EL2、指令完成、无故障且 HDD 不禁止；低 EL 保留 Unknown，在任何 GIC opcode 前拒绝；已知 EL2 HDD 禁止为 AccessRestricted。

十项前后证明是 DSPSR、DLR、ID_PFR1、ICC_HSRE、ICC_SRE、ICC_CTLR、ICH_VTR、HCR、ICH_HCR、HSTR。逐步确认使能/实现再访问后续控制；R52 的 ID_PFR1.GIC 在 [31:28]，HSRE=`0xf`、SRE=`0x7`、CTL 除低两位为 `0x400`、VTR=`0x90180003`。在数据指令之前判定 AP 未实现或 IAR 有副作用。只读单个 MRC32，保存/恢复/物理回读 R0，复核控制、状态、身份与 PC；控制/传输/scratch 不确定时停止、标 UNKNOWN 并隔离服务，不猜测回滚或重试。

Probe 优先专用 ICC_CTLR 与 ICH_VTR。原始 `icc.ctlr_pribits`、`ich.vtr_pribits/prebits/listregs` 可来自 GDB，但只保留原始字段。真正 `icc.physical.*`、`icv.virtual.*`、`ich.list_registers` 要求成功原生来源、PhysicalCore、同 owner/context、Tcl GIC 路由、响应阶段、完整请求区间、MIDR 与原始值匹配，以及正确接口的当前 EL2 证据。声明值和观测值分别保存；不写回客户配置。

实现条件独立使用 `icc.physical.prebits` 与 `icv.virtual.prebits`：索引 0 至少五位、1 至少六位、2/3 至少七位。Unknown 不自动试读，No 即便手工也不发数据访问。使用独立架构条件夹具验证 5/6/7 边界；原生 R52 只证实五位与四列表。

## 多核、来源与保留依据

Scope All 只访问选中物理 core，绝不广播。服务事务保存/恢复原 target，响应与 frame 0、实际线程、session/停止代次和 owner 绑定；切核、切帧、继续/再停止或重连使旧 Probe 无效。取消在途访问时完成已发送事务的恢复，再丢弃结果；无法恢复则隔离服务。

`Access.gic` 保存两种物理接口、全部十二项原始证明和 MRC32 方法，与实际 endpoint/target/命令/主机请求区间一起发布。失败旧值保留其原始来源和时间；不把主机响应时刻当硬件时间戳，跨条目和跨核不能视为同时采样。alias 记录派生链，ICV AP 不伪装成独立 ICC 读取。

自检补齐了条件判定的证据保留：`ProbeBasis.observations` 保存成功样本的完整 provenance，已过滤的 No 条目即使没有数据读仍可追查容量读取的 Debug 状态、接口、控制、路由和时间。旧值沿用原判定依据；失败样本的保留 raw/native proof 不作为当前成功依据。旧 JSON 缺少新增字段仍兼容，并保持 Unknown，绝不补造物理证明。

## 后端构建与验证

后端基于 OpenOCD `d3ebb8d2b9adbfd9a13072e8e446f424b5ff3c0e`、Jim Tcl `d5243a25c488dfe751ef218828f13516e04ea2ba`。当前 source.lock 记录十一项源文件和七个独立协议，补丁 SHA256 为 `11774d282896cf012eb5f41850683253b75b7e2d35505a6f03bc63544d2df465`。

- Linux 候选 SHA256：`f66512dfa54242498ca9bd737559aa9b8d0875b289016140dfba8ffb6f38f822`。新目录构建及真实离线命令/生产事务报告在 `/home/bei-li16/.cache/debugtui-openocd-linux-gic-20261006/tests/report.json`，镜像日志 `artifacts/openocd-gic-linux-build.log`。
- Windows x64 候选 SHA256：`5d65e19fe1c9463fc84e6aed45879ec26eaaee55f1ce6c63748ce12dddf5ca40`。交叉构建 `artifacts/openocd-gic-windows-build.log`，本机验证 `artifacts/openocd-gic-windows-native.log` 与 `.dev/openocd-windows-gic/windows-tests/report.json`；DLL 依赖、F429/CMSIS 配置与 GPL 对应源码检查通过。九项包拒绝测试见 `artifacts/openocd-gic-package-corrected-test.log`。
- 同一生产 GIC C 事务以四十三条独立手写编码/十条独立证明编码验证二十九项成功、3,190 个 I/O 故障点、239 项安全拒绝、406 项证明/scratch 变化。确认无 ack、控制写、模式切换和故障后续访问，见上述两平台事务报告与 `artifacts/openocd-gic-production-model.log`。
- 前序 PMU 自检发现原生安全错误误带 Timer 前缀，本批改为 PMU 自有 typed-tag，并以三个精确错误断言验证；两个当前候选均含修正、已重建，旧 PMU 候选与历史报告保留。前序 Timer/银行/VFP/写入事务随七项独立协议全部回归。

当前候选仅用于开发验证，尚未替换全局 xPack/用户安装，也未建立实板等价证明。源码 ZIP 包含当前十一项源文件与 recipe 补丁/源锁/README/生产测试；构建并不证明目标硬件访问成功。

软件专项包含 GIC 解析/路由/条件单元、七项真实 EXE/worker/MI/Tcl 集成及十三项能力回归。它们验证双核 Scope All、停止前 USR 与当前 EL2 的区别、二十九项原生读取/ICV alias、No 零访问及完整依据、独立物理/虚拟容量、权限/协议/截短/伪造拒绝、取消/上下文变化和故障隔离。延后驱动另验证独立固件参考、未就绪及参考值不匹配，失败正常清理且工程不变。完整回归与最终计数见 [开发进度](registers-development-status.md)。

## 延后环境与固件

[十项环境 case](../tests/cases/register-gic.md) 全部 SKIPPED。`node scripts/test-register-gic-hardware.cjs` 默认五阶段 SKIPPED、无连接；只有显式 `--run --binary EXE --project TOML --core CORE --case JSON` 执行专用停止夹具。默认报告 `artifacts/register-gic-hardware-1791218725523-5f1d83e3/report.json` 明确 `board_tests_executed=false`。

`tests/fixtures/register-gic-board.c` 在正常 Hyp、身份/容量/使能已符合条件时只读捕获全部二十九项值，每核独立 snapshot slot；ready 门禁不通过不继续。不会切换模式、使能 SRE、确认中断或配置控制。GNU Arm 11.4 离线编译与独立三十七条 MRC 反汇编检查通过，确认 MRS 守卫先于 GIC 访问、无 IAR/MCR/MCRR/MSR/MRRC，证据为 `artifacts/register-gic-firmware-report.json` 与 `artifacts/register-gic-board-disassembly.log`；对象未执行。

JSON 示例是软件数据，物理使用前必须以各实际 core 的独立固件/已审核调试工具证据替换 reference/expected/MIDR，并去除 software_example。HPPIR/AP/LR 会受外部中断与 Guest 调度影响；先由实际环境建立静态基线，不能让驱动禁用中断或清 pending 来制造一致性。可选 peer_ready/peer_core 支持逐核基线与不变检查；本批驱动软件正向流程仅验证单核，双核 worker 隔离另有独立测试。完整双核环境、MMIO、低 EL ICV、Debug 和最终工具整合继续待验收。

## 最终软件结果

完整 `node scripts/test-functional.cjs --only unit` 为 **375 单元＋150 集成通过，2 ignored**，共 **525 通过**，**F24 129/129**（匹配模式数），334793 ms 无超时；其余 **22 套件未选择**。报告 [`artifacts/functional-1791220138851-f975cb60/report.json`](../artifacts/functional-1791220138851-f975cb60/report.json)，Cargo 为同目录 unit.log；最终严格 Clippy 日志 `artifacts/register-gic-clippy-final.log`。

源码锁/候选/对应源码 ZIP 一致性自检：`artifacts/register-gic-final-package-audit.json`。仅 REG-408 软件范围勾选，已完成／未完成 24/47；REG-405 与最终交付仍未完成。
