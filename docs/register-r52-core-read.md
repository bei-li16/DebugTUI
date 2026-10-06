# R52 常用寄存器的受保护只读访问

本批接通普通 32 位 CP15 读取的 host 与生产后端事务，是 C03 的子批次。支持已复核的 15 项身份/控制/MPU 标量及 96 项 EL1/EL2 MPU 直接索引定义，不增加目录类别。MPU 总览现已复用该事务并核对整批证据，见下节；selector 的统一权限改造、完整 OpenOCD 候选构建与命令入口验收仍待完成，不能仅凭本批模型和 worker 通过勾选 C03。

三个核心参考保持绝对路径：

```text
G:\Data\GitFiles\DebugTUI\tui-debugger-spec.md
G:\Data\GitFiles\ARM\File\Arm® Cortex®-R52 Processor Technical Reference Manual.pdf
G:\Data\GitFiles\ARM\File\Armv8-R AArch32.pdf
```

当前 Debug 行为另参考 `G:\Data\GitFiles\ARM\File\ARM Architecture Reference Manual Supplement - ARMv8, for the ARMv8-R AArch32 architecture profile.pdf`，编号 `ARM DDI 0568A.c`，F1.3.4/G2.1.8。指定的架构介绍不能替代 External Debug 详细规则。

## 配置与读取范围

选择本批协议的每核有效配置示例：

```toml
[registers]
cpu = "cortex-r52"
cp15_command = "aarch64 r52_read"
selector_command = ""
isb_command = ""
tcl_endpoint = "localhost:6666"
[registers.targets]
"core0" = "soc.r52.0"
"core1" = "soc.r52.1"
```

每核覆盖继续使用已有 `cores.registers`。当前配置验证只允许旧 selector 与旧 MRC 家族匹配，因此这个普通读取子批次清空 selector/ISB；不要混用旧 selector 编码参数。已有 Banked/VFP/Timer/PMU/GIC 的显式专用配置继续独立生效。

后端命令为 `aarch64 r52_read NAME`，只接受 MIDR、MPIDR、SCTLR、HSCTLR、CPACR、HCR、MPUIR、HMPUIR、PRSELR、HPRSELR、HPRENR、MAIR0/1、HMAIR0/1 的小写名称，以及 `prbar0..23`、`prlar0..23`、`hprbar0..23`、`hprlar0..23`。实际 index 必须小于同次读取的 bank 容量。编码由既有 reader 转为受限名称；未知编码、错误宽度或非 core 归属不发送数据请求。

`aarch64 debugtui_r52_protocol` 必须返回 `debugtui-r52-core-1 external-identity current-el2 dspsr dlr fresh-capacity scratch-readback stop-on-fault`。旧后端、协议不符和缺失/矛盾的结果为 ReaderUnsupported，不回退普通 MRC 或 GDB。旧配置行为保留；本批未将客户配置或已安装工具改为新协议。

## 权限与事务

本批成功范围是实际 Arm D13 R52、AArch32、当前 Debug EL2、HDD[15]=0、指令完成且无故障。低 EL 在 CPU 指令前返回 Unknown/Restricted，未适配身份返回 Unsupported；不切换 CPU 模式、改控制/权限或尝试低 EL trap。当前 EL2 的合法性只适用于已复核的受限指令范围，不授权任意 CP15 编码。

后端通过外部 Debug AP MIDR/EDSCR 核对身份与状态；DPM 的实际外部访问还检查 EDPRSR.HALT。先验证当前状态，再使用既有物理保存/恢复/回读 R0 的 helper 读取 DSPSR/DLR 和数据。DSPSR 保留停止前程序的全部状态，User 值与当前 Debug EL2 可以同时成立；它不授予访问权限。

MPU 依赖项在同一事务读取正确 bank 的 MPUIR/HMPUIR，核对 EL1 的 16/20/24 或 EL2 的 0/16/20/24，并在读取后重新确认。零 EL2、超出当前容量的 region 为 NotImplemented；非法容量为 Unsupported。容量拒绝不读取 region，也复核已执行前置指令的恢复边界。直接索引读取不写 selector。

数据前后验证外部状态/身份、完整 DSPSR/DLR 和相应容量；R0 恢复逐次物理回读。故障、状态变化或恢复不确定立即停用目标，不发布值、不盲目重试或推测回滚。host 复用现有服务锁、实际线程和 frame 0 检查、取消边界与 target 恢复。Scope All 仅读取所选核心，证明绑定当前请求与上下文，不跨核/停止点复用。

host 的 min_el 预检查仅在全部读取依赖都是受限 core reader 时，把 EL 核实交给该事务中的后端；不把固定 EL2 写成观测事实。owner、NeedHalt、NeedEnable、实现条件、Alias 与副作用门禁继续生效。能力 Probe 的 MIDR、MPUIR、HMPUIR、CPACR 在新配置下也走该生产路径，不使用可见的 GDB 名称替代它，不用保存 CPSR 判断 HMPUIR 权限。

响应固定为 `midr RAW32 dscr RAW32 dspsr RAW32 dlr RAW32 bank none|el1|el2 capacity RAW32 value RAW32`。所有原始值精确 32 位；外部 MIDR 与 CPU MIDR 数据必须一致。采样来源保留实际协议、target、endpoint、请求区间及 `access.r52_core`；失败旧值保留原来源。界面可查看当前 Debug 状态和停止程序状态，证据不授予后续请求权限。

## MPU 总览

在上述新配置下，`registers_mpu` 的 EL1/EL2 总览复用现有整批服务锁和每次 `r52_read`，直接索引读取容量内的 BAR/LAR，不写 selector。保存的 CPSR 可以是 User，权限由每次后端的当前 Debug EL2 证明独立判断。CPSR 的 GDB 来源继续保留；其完整程序状态和实际线程/帧在发布前复核，不能冒充后端权限。

读前逐项核对有效目录的 reader、core 归属、宽度和副作用，并保留用户声明的更严格访问/实现条件。实际 MIDR 与本次 Probe 一致，新鲜 count 与 Probe 一致后才读取控制/MAIR/region；不因目录选了 R52 或旧 Probe 有容量就直接读取 index。每项必须有本次 owner/context 的响应证明，且外部 MIDR、DSPSR、DLR、EDSCR 的架构状态与整批首项一致，bank 容量的完整原始值与当次 count 一致。允许 EDSCR 的 DTR 传输标志改变，它们不表示权限变化。

发布前重新物理读取 MIDR 和相应 MPUIR/HMPUIR，复核状态与完整容量；这些校验值不替换原始样本的请求来源。任一权限拒绝、未知、证明/容量变化、上下文改变、取消或后端故障都停止整批，不发布已读部分。旧样本按既有失效规则保留原值、时间和证明；恢复不确定继续 FAULT/服务隔离。整批证据一致性仍是顺序采样约束，不宣称硬件原子快照或地址的有效访问权限。

零 EL2 bank 只读 MIDR/HMPUIR 及它们的最终校验，不读取 region、MAIR 或 MPU 控制。仅查看已缓存总览不产生 GDB/Tcl I/O。Scope All 下只有选中物理核心执行总览；切核需要对应 context 和 Probe，不能借用另一核容量或样本。旧 MRC 配置仍维持原有兼容行为，本目标的当前 Debug 权限验收以新协议为准。

延后 [MPU 硬件 case](../tests/cases/register-r52-mpu-read.md) 复用 `scripts/test-mpu-regions-hardware.cjs`，增加可选 `current_debug` 独立期望，校验实际 EXE 返回的 route、当前状态和全部 region/MAIR。新模板为 `tests/fixtures/r52-native-mpu-board.example.json`；software_example 不能用于实板验收。

## 验证边界

`tools/openocd-adapter/tests/r52-transfer.c` 编译实际生产事务头文件，物理 I/O 使用独立模型；手写指令字来自 TRM，覆盖 111 项、4360 个逐操作故障点、768 个独立 bank 容量组合，以及恢复/身份/DSPSR/DLR/EDSCR 变化。低 EL/HDD 拒绝不执行 CPU 指令，容量拒绝无 region 数据读取，无模式/selector/使能/控制 MCR。

Rust 测试覆盖受限编码与协议解析、权限预检查、实际 MI/TCP/Tcl worker、多核隔离、能力 Probe、保存 User 状态、取消/帧变化、旧值和故障隔离。延后驱动默认四项 SKIPPED；实际 EXE 与软件夹具验证驱动流程及错误独立基线拒绝，不能当作 ARM 指令执行或实板证据。

最终日志、适用源码及摘要见 [唯一验收账本](registers-readonly-goal.md) 本批记录。[硬件 case](../tests/cases/register-r52-core-read.md) 尚未上板；`source.lock.json` 的 Windows/Linux 新候选仍未构建，verified=false、候选摘要为空。稳定后端的完整候选及最终非主分支 Release 仍是目标结束条件。
