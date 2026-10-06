# 银行读取的当前 Debug 状态证明

当前补充：[R52 EDSCR 修正](register-r52-debug-state.md) 使用 HDD[15] 并接受 RES1[16]；本文后续构建证据属于旧补丁，新候选仍待构建验证。

2026-10-06。REG-303 仍未完成；总进度为24完成／47待完成。未执行上板，未发布 Release，源码与全局安装仍为0.9.3。

## 自检结论

前序后端使用保存的 DSPSR.M 选择当前银行，再注入 CPU MIDR MRC。DSPSR 描述停止前程序状态，不独立证明已经执行过调试指令后的当前模式；低 EL 的 MIDR 指令可能受 HSTR 限制。不能据此宣称全部 EL1 银行已安全适配。

本轮查阅工作区完整 DDI0487 M.b H2.4.2.2/H2.4.8.2：调试态直接 CPSR/PSTATE 读取是受约束不可预测行为；H2.4.2.2.2 明确允许普通 MRS SPSR。初稿曾采用当前 CPSR 读取，但在源码/模型阶段纠正，未执行目标指令；初稿的模型通过不是架构适用证据。`artifacts/banked-current-*` 保留初稿与纠正过程；最终依据仅为 `banked-proof-*` 和 `banked-current-native-final`。

## 最终行为

专用协议升级为 `debugtui-armv8-banked-2 external-identity current-el dspsr dlr mrs physical-readback no-mode-change stop-on-fault`。在配置的选中 target 事务内检查独立协议；旧 v1、缺失证明、截短 raw、未知或矛盾的身份/EL/方法被拒绝，不回退旧 get_reg、GDB 或模式切换 DPM。

外部 Debug AP 的 MIDR/EDSCR 提供当前身份与 EL，每次 AP 调用核对物理 EDPRSR.HALT。仅接受 Arm/D13/架构F/AArch32、ITE且无故障的状态。逻辑 core 与 target 的映射仍由工程配置给出，不把 MIDR 型号当作唯一核标识。

| 当前 Debug 状态 | 原生行为 |
| --- | --- |
| EL2/Hyp | 三十项允许；SP_hyp 用 MOV32、SPSR_hyp 用普通 MRS32，其余按合法银行规则选择 MOV32或banked MRS32 |
| EL0/User | 七项当前 User 银行使用 MOV32；其他二十三项受限 |
| EL1 | 三项 Hyp 银行受限；其他二十七项权限未知，在 CPU 指令前拒绝 |
| 未适配身份、AArch64、EL3、无 ITE | 不发布数据 |
| 故障、传输/恢复失败、前后身份/执行状态变化 | 停止并隔离 target，结果未知；不继续注入、重试或推测回滚 |

内置目录仍为原二十三项；其余七个 User 名称通过自定义目录验证，不把目录已有通用 R8–R12/SP/LR 改为新路径。完整 EL1/FIQ/IRQ/SVC/ABT/UND/System 当前模式证明仍待解决，因此 REG-303 不勾选。

每次 DSPSR、DLR 与数据传输保存、恢复并物理回读 R0。前后全位复核 DSPSR/DLR，前后核对外部身份与执行状态。不执行 MRS CPSR、CPU MIDR MRC、模式/系统/FPU 控制写入。

成功响应为 `midr RAW32 dscr RAW32 dspsr RAW32 dlr RAW32 value RAW32 method mov32|mrs32|banked_mrs32`。Rust 严格校验名称/字段/32位 raw/实际身份/EL与合法方法，`provenance.access.banked` 记录四项原始证明和传输方法；已有 provenance 提供 owner/context/实际 route/请求起止时间。详情区显示 EDSCR 派生的当前 User/Hyp 模式，DSPSR单列为停止前状态；失败旧值保留旧证明，新失败请求不冒充成功。旧 JSON 缺少新字段仍缺少证据。银行与各核分别采样，无跨项原子快照。

## 软件和环境证据

最终生产 C 模型：37项 EL0/EL2 成功、41项受限、162项 EL1 Unknown、1110个 I/O故障点；全部三十条银行编码有独立 GNU汇编词证据。模型禁止当前 CPSR和CPU MIDR指令，核对DSPSR/DLR/scratch/身份/执行状态变化，不发布部分结果。报告 `artifacts/banked-current-native-final/report.json`，日志同目录 `banked-transfer-test.log`。

实际 worker/MI/Tcl 回归覆盖三十项名称×八种模式的成功或拒绝结果、保存 User DSPSR而当前 Hyp的反例、Scope All只访问选中核心、线程/frame变化、取消、旧协议/裸值/伪造/截短、安全拒绝后的Core和控制，以及失败后原值/证明。延后驱动用实际EXE验证Hyp、User及EL1，User另用自定义目录读取七项当前银行；精确验证外部身份/EL/方法/来源/上下文/请求时间、独立固件基线、控制/peer/客户文件不变。默认四阶段SKIPPED且不连接目标。聚焦记录 `artifacts/banked-proof-parser.log`、`artifacts/banked-proof-worker-final.log`；最后软件总回归另见下方最终记录。

Windows/Linux新目录构建完成，七个协议/生产事务与真实离线命令检查通过。Windows本机验证DLL依赖和两份STM32离线配置，未init物理adapter。十一项源锁、补丁、候选以及对应源码ZIP逐字节一致性有最终审计；此前候选保存在source.lock的previous_candidates。当前候选未安装为全局OpenOCD。

正常固件钩子扩展到三十个只读参考槽，八种模式用GNU Arm11.4离线编译，禁止模式/控制写入；User源无MRS/MRC。报告 `artifacts/banked-proof-firmware-report.json`。这是正常固件中的采样指令，和调试态禁用CPSR读取的规则分开。[八项新环境case](../tests/cases/register-banked-proof.md)全部SKIPPED；真实RAM/cache可见性、调试指令执行、恢复、EL1完整适配和实际双核仍未验收。

完整 Cargo **385 单元＋160 集成通过，2 ignored，共545通过**；**F24 138/138** 为证据匹配模式数，非用例数。完整运行489102 ms，无超时；其余**22外层功能套件未选择**。报告 `artifacts/functional-1791232474564-90d658f4/report.json`，完整Cargo为同目录 `unit.log`；严格Clippy通过，日志 `artifacts/banked-proof-clippy-final.log`。

首轮完整回归 `artifacts/functional-1791231500428-0254f26a/unit.log` 有385单元通过、2ignored，但分发夹具耗尽G盘，分发/总报告未写完，未计为完整通过。所有失败文件完整归档F盘，原目录保留逐文件哈希一致的原始日志/JSON/TOML；记录 `artifacts/banked-proof-first-full-failure.json` 和 `artifacts/banked-proof-failed-space-archive.json`。分发驱动新增独立 `DEBUGTUI_DISTRIBUTION_ARTIFACT_ROOT`，仅改变测试产物位置；默认仍用工程artifacts，运行逻辑和十四项断言保留。

下一完整回归 `artifacts/functional-1791231868864-b85b2023/report.json` 中分发十四项已通过，但已有MMIO共享证明测试退出时失败：模拟通知文件持续强制peer RUNNING，quit中断后又被改回RUNNING，frame查询无帧。所有共享失效/私有保持/零数据访问断言均已通过；修复仅在这些断言之后清除模拟RUNNING注入，再正常清理，未删除断言或忽略quit错误。聚焦 `artifacts/banked-proof-mmio-cleanup-focused.log` 通过，最终完整回归另列。新User七项实际EXE延后驱动、Hyp与EL1共三种五阶段流程在 `artifacts/banked-proof-extended-user-driver.log` 通过。

最终完整测试全部通过后，紧接的Clippy仍因其他实际EXE夹具占满G盘而报磁盘空间不足，失败日志 `artifacts/banked-proof-clippy-enospc.log` 保留。确认Cargo/Clippy进程全部结束后，将已通过总报告引用的52个完整产物目录归档F盘，逐文件SHA256核对，原目录保留原始顶层日志/JSON/TOML等证据；映射 `artifacts/banked-proof-full-passed-space-archive.json`。归档不改变通过计数。随后严格Clippy重跑通过；没有为节省空间缩减用例或跳过检查。
