# R52 PMU 读取与证据

当前补充：[R52 EDSCR 修正](register-r52-debug-state.md) 使用 HDD[15] 并接受 RES1[16]；本文后续构建证据属于旧补丁，新候选仍待构建验证。

本批落实 REG-404 的软件读取范围。源码仍为 0.9.3，未修改安装版本，未执行上板测试。目录、读取通道、实现数量和访问权限分别记录；目录中存在名称不证明目标可读。

依据为本地 Cortex-R52 TRM `100026_0104_01_en` §4.2.18、§13.1–13.4、p.642，以及 Armv8-R AArch32 Supplement `DDI0568A.c` Table E1-1、HDCR/HSTR 说明。Debug 中系统指令保留当前 EL 的权限和陷阱，依据 `DDI0487 M.b` H2.4.5/H2.4.8。新版架构手册仅用于未改变的指令语义，不导入 R52 未实现的 PMUv3 扩展。

## 数据与路由

R52 有四个 32 位事件计数器、一个 64 位周期计数器。PMCR.N 不包括周期计数器；PMCR.E=0 不意味着未实现。PMCR.LC 选择周期溢出检测使用 bit 31 或 bit 63，周期存储仍为 64 位。本批 PMCCNTR 使用一次 `MRRC p15,0,Rt,Rt2,c9`，不拼接 32 位 MRC 或外部 MMIO 高低字。

二十二项 32 位目录覆盖 PMCR、PMCNTENSET/CLR、PMOVSR/PMOVSSET、PMSELR、PMCEID0/1、PMXEVTYPER/PMXEVCNTR、PMUSERENR、PMINTENSET/CLR、PMCCFILTR、四项 PMEVCNTRn 和四项 PMEVTYPERn。事件项目直接使用 c14/c8 与 c14/c12 的 Op2=0–3 编码，保持 PMSELR；没有 PMSWINC reader 或 PMU writer。位域、注释、实现条件由 `register_pmu_metadata.py` 维护，R52/R52+ 目录同步生成；D16/R52+ 实际身份仍未适配，不能用目录名宣称支持。

`registers.pmu_command="aarch64 pmu"` 是独立显式选择；默认空值不启用专用协议。选择后逐项校验原始 reader 编码与位宽，不按名称截获客户的其他编码，不在协议/权限/传输错误后回退到普通 MRC、MRRC 或 GDB。GDB 的停止前 CPSR、配置声明和旧 PMCR 样本不能替代当前权限或计数器数量证据。

PMSELR=0–3 时，PMXEVTYPER/PMXEVCNTR 使用当前索引；SEL=31 时 PMXEVTYPER 是 PMCCFILTR，PMXEVCNTR 在 R52 上 UNDEFINED，在数据指令之前拒绝。SEL=4–30 的选中事件也先拒绝。直接索引读取不改变选择器，读操作不会开始、停止、复位或清空计数，不更改过滤器、溢出标志和中断。

## 当前 Debug 状态与数量

原生后端从明确配置的当前 target 的外部 Debug AP 读取 EDSCR、MIDR，并在每次外部访问前检查 EDPRSR.HALT。只适配 Arm/D13/AArch32：当前 Debug EL2、ITE 已完成、无错误、HDD 不阻止 EL2。先读取 DSPSR、DLR、ID_DFR0、PMCR、HDCR、PMSELR；要求 PerfMon=3，实际 PMCR 的 IMP/IDCODE/N/RES0 符合 `0x41132000`，确认四项物理事件容量后才发送数据读指令。

EL0/EL1 的 PMCR.N 可受 HDCR.HPMN 限制，PMU 访问还受 TPM、TPMCR、HSTR.T9 和用户使能控制。低 EL 无法读取 Hyp 控制来证明权限，本批返回 Unknown，保持 Halt 与模式，不注入 PMU 数据指令；HDD=0 或停止前 Hyp 不作为陷阱豁免。当前 EL2 且 HDD=1 的已知禁止状态为 AccessRestricted。低 EL 完整支持和通用系统权限补齐仍属于后续任务。

数据读取后重新验证完整六项证明字、MIDR、当前 EDSCR 状态；每次使用的 R0/R1 均恢复并物理回读。传输、执行、控制、身份、PC/状态或 scratch 回读不确定时停止，不回滚/重试，将 target 标为 UNKNOWN 并隔离服务。收到自相矛盾或截短的成功回应也不发布值。

新 `Access.pmu` 保存上述八项证明字和 MRC32/MRRC64 方法。样本绑定物理 core、frame 0、停止代次、服务 endpoint/target 和本次主机时间区间；跨核与跨寄存器不是原子快照。旧 JSON 缺少 PMU 字段仍兼容，失败后的旧值保留原始证明，不能冒充新采样。

自检纠正了旧 Probe 仅凭停止前 `CPSR.M=Hyp` 确认物理数量的问题。`pmu.counters` 现在要求 PMCR 样本的 native 来源、PhysicalCore 视图、响应阶段、上下文、实际 MIDR、完整匹配值和当前 EL2 证明。`pmu.pmcr_n` 可保留未经物理容量证明的原始字段；`pmu.guest_partition` 是 HDCR.HPMN 原始值，不授予 Guest 访问。已选择专用协议时，Probe 不使用同名 GDB PMCR 代替原生读取。

## 后端与软件验证

补丁基于 OpenOCD `d3ebb8d2b9adbfd9a13072e8e446f424b5ff3c0e`、Jim Tcl `d5243a25c488dfe751ef218828f13516e04ea2ba`，PMU 原批次维护十项源文件散列及独立 `debugtui-armv8-pmu-1` 协议。以下为保留的 PMU 批次证据；当前包含 GIC 的十一项源锁、候选散列和原生错误标签修正见 [GIC 自检](register-gic.md)。PMU 批次补丁 SHA256：`803aad6b643b4df96ff6e2afe8c3e495c6497992205c08cba3e9cff2b944e24d`。

- Linux 候选 SHA256：`acf62e95c0ed092394eb530a7fb73f4408e313f8f08c6b7fa9bc5b585eeadc9f`，构建与生产 C/真实离线命令报告在 `/home/bei-li16/.cache/debugtui-openocd-linux-pmu-final-20261005/tests/report.json`，日志镜像为 `artifacts/openocd-pmu-linux-final-build.log`。
- Windows x64 候选 SHA256：`bfe3d7aa99e10a978f0d34b3aa9b479a61bfef0ba68dc56ae41c9836d8321bc1`，本机验证报告 `.dev/openocd-windows-pmu-final/windows-tests/report.json`，日志 `artifacts/openocd-pmu-windows-native-corrected.log`。依赖 DLL、GPL 对应源码与本机 dummy/profile 检查通过；未等同于已安装 xPack 或实板验证。
- 实际生产 PMU C 事务验证 23 项独立手写编码、1,614 个 I/O 失败点、190 项安全拒绝、208 项证明/scratch 变化、六个完整 64 位移动计数样本及 LC/E 组合。全部路径没有 PMU 控制或选择器写指令，失败不发布结果；scalar 仅在读取成功后复制，失败路径不使用未初始化值。
- 实际 DebugTUI worker、MI、TCP 和真实 Tcl 模型验证专用协议、当前 EL2 与停止前 USR 的区别、所有位宽、Scope All 的选定核、旧上下文、取消、变更、错误分类/隔离、旧值来源保留。延后驱动另验证独立固件值、不就绪、参考值不匹配和已开启计数拒绝；完整回归为 371 单元＋143 集成通过、2 ignored，F24 127/127；最终报告见 [开发进度](registers-development-status.md)。

## 环境用例与固件

`tests/cases/register-pmu.md` 十项环境用例均 SKIPPED。`scripts/test-register-pmu-hardware.cjs` 默认生成五项 SKIPPED 且不连接目标；只有显式 `--run --project FILE --core NAME --case FILE` 执行专用停止夹具。JSON 中的示例值必须先换成独立实测值；`software_example=true` 禁止作为物理验收使用。

`tests/fixtures/register-pmu-board.c` 只读固件 hook 在正常 Hyp 执行，先确认模式、身份、容量、已有 PMCR.E=0、HDCR.HPME=0、有效选中事件和非零周期高字，再采集全部原始值。它不会关闭或配置计数器；启动前置条件不成立时 ready=0。每个物理核必须有独立 snapshot 槽和相应符号，不能共享一个数组元素。

GNU Arm `arm-linux-gnueabi-gcc` 离线编译成功；反汇编独立核对 22 项标量编码与一条 MRRC，并确认 MRS CPSR 先于 PMU 访问、无 MCR/MCRR/MSR。证明在 `artifacts/register-pmu-firmware-report.json` 与 `artifacts/register-pmu-board-disassembly.log`。对象未链接、未执行。实际目标需自己的已审核启动、权限和工具链，编译不证明硬件可读。

外部 PMU 是独立 CoreSight 组件，需要实际 PMU base/channel/authentication/lock 配置。TRM 指出外部 PMCR 与系统 PMCR 不同；不能将 Debug base 加 `0xE04` 当成 PMU 系统读值。本批不猜测 MMIO 映射，也不宣称开始性能测量、Trace 导出或 ADS 资源复用。

最终候选另通过九项包装拒绝检查，日志 `artifacts/openocd-pmu-package-corrected-test.log`。源码、候选、对应源码 ZIP、当前补丁/测试/README 的一致性记录 `artifacts/register-pmu-final-package-audit.json`；两平台均在错误路径修正后重新构建，旧候选记录保留。
