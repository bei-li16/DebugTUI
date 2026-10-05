# S/D/Q 存储视图与浮点显示自检

2026-10-06。对应 REG-305 的软件验收；实板未执行。后续[VFP当前Debug状态自检](register-vfp-proof.md)已升级v2的当前Hyp证明；REG-304的完整EL1/Guest/User合法读取仍待完成。

## 范围与依据

本地 Cortex-R52 TRM 100026_0104_01_en 的第48页说明 S/D/Q 是同一存储的不同视图，第560–568页列出单精度 D16 与可选双精度/Advanced SIMD D32 及 MVFR0/MVFR1 字段。给定的《Armv8-R AArch32.pdf》是26页概述；另外核对同目录完整 DDI0568A.c，第20/26/54页约束单精度且无 SIMD 时只能 D0–D15，并允许 VMOV 两个整数寄存器与一个 D 寄存器间传输。官方参考：[Cortex-R52 TRM](https://documentation-service.arm.com/static/5f905fedf86e16515cdc25e2)。

| 观察到的配置 | S | D | Q |
| --- | --- | --- | --- |
| 合法 D16，MVFR0/MVFR1 一致 | S0–S31 | D0–D15 | 明确未实现 |
| 合法 D32/NEON，MVFR0/MVFR1 一致 | S0–S31 | D0–D31 | Q0–Q15 |
| 缺失、矛盾或未适配的能力 | 保持 Unknown | 保持 Unknown | 保持 Unknown |

S(2n) 和 S(2n+1) 分别来自 Dn 的低/高32位，n<16；Qn 来自 D(2n) 的低64位和 D(2n+1) 的高64位。内置目录使用 Single、Double、Quad 分组与数量/扩展条件；GDB 名称数量、CPACR 和 FPEXC.EN 不用于推断实现容量。普通位格式可以解释64位数据为 IEEE binary64，D16 的存储宽度不意味着支持双精度运算。

## 本轮变更

成功 VFP pair 响应现在把首个 D 编号、完整128位 raw、同次 MVFR0/MVFR1/FPEXC 保存到 `provenance.access.vfp_pair`。数据只在一次请求内共享；同一物理 pair 的 S/D/Q 保留原始 route/context/请求区间，没有额外读取。`PairEvidence` 校验精确位宽、完整 raw、D16/D32 一致性、EN 和边界，并可校验各标准视图属于该 pair。控制寄存器没有伪造数据 pair，旧 JSON 缺字段仍缺证据。

详情显示 D 对、容量、原始位和能力字段，说明 lane 0 是最低有效位。格式只作用于已经取得的 raw，显示切换不发出目标读写；整数、binary32/binary64 及8/16/32/64位向量分量沿用同一格式器。quiet/signaling NaN 保留符号和完整载荷，正负无穷、正负零和 subnormal 有独立验证。字段仅提供整数格式。

失败请求不制造新成功证明；Snapshot 的最近有效值保留此前 pair、原始能力、owner/context 与时间。不同 pair 和不同核心分别采样，无跨项原子快照。存储元数据本身不授予执行权限；本批552项证据使用当时v1，后续v2将外部MIDR/EDSCR和DSPSR/DLR/HCPTR另存为当前Hyp访问证明，停止前DSPSR不再充当当前EL依据。完整低EL后端适配继续列在REG-304，最新回归见[VFP证明](register-vfp-proof.md)。

## 软件证据

四项新单元覆盖全部标准视图、D16/D32边界、伪造/截短/矛盾证据、特殊值向量和详情。三个新集成覆盖全部D16项目、失败旧值/原来源，以及独立64个逻辑字的80个S/D/Q视图；最后一项使用真实 worker/MI/TCP/Tcl、双核 Scope All，只有选中核数据被读取，peer保持不变。既有86项D32全视图、Unknown/关闭/受限/协议/上下文/取消/恢复故障回归保留。

实际 EXE 的延后驱动验证D32、D16、EN=0和受限四种五阶段软件场景。驱动逐项比较独立正常固件RAM参考，并核对同次 pair raw、MVFR/FPEXC、owner/context/route及请求时间。聚焦日志 `artifacts/vfp-views-unit-focused.log`、`vfp-views-ui-focused.log`、`vfp-views-worker-focused.log`、`vfp-views-patterns-focused.log`。

`tests/fixtures/register-storage-patterns.json` 手工给定特殊原始字与显示期望，包含NaN、无穷、负零、subnormal及不同高低字。配套 `.S` 仅供专用正常汇编启动代码初始化测试 FP 数据，故意替换D8–D15，不能当作普通C/AAPCS函数使用；不会写模式或FP控制。D16/D32×大小端四种离线对象已编译，全部初始化编号及256字节数据节与独立字表一致；对象未执行。报告 `artifacts/storage-view-firmware-report.json`。

[八项环境用例](../tests/cases/register-storage-views.md)全部SKIPPED，默认既有驱动四阶段SKIPPED。实板读取、缓存可见性、寄存器恢复和真实终端视觉结果不由软件模型推断。

完整 Cargo **389 单元＋163 集成通过，2 ignored，共552通过**；**F24 145/145**为证据匹配模式数，非用例数。完整运行496515 ms，无超时；其余**22外层功能套件未选择**。原始完整报告与53份子报告位于F盘，完整总报告/日志的逐字节镜像为 `artifacts/functional-1791235241817-19eb948a/report.json` 和同目录 `unit.log`；镜像核对见 `artifacts/storage-views-report-mirror.json`。严格Clippy通过，日志 `artifacts/storage-views-clippy.log`。REG-305软件范围与REG-006源码/构建自检已验收，完整TODO为**26完成／45待完成**，目标active；未执行上板、安装或发布Release。

测试产物可用 `DEBUGTUI_TEST_ARTIFACT_ROOT` 指定其他磁盘；默认位置保持工程artifacts。功能入口同时发现工程目录、该可选目录与独立分发目录中的本轮报告。只是测试输出位置变化，全部用例和断言保留。
