# R52 常用寄存器的受保护只读访问

普通 32 位 CP15、MPU 总览和有界 MPU selector 已接通当前 Debug EL2 的 host 与生产事务，并通过 Windows/Linux 完整 OpenOCD 候选构建和实际命令入口检查。支持已复核的 15 项身份/控制/MPU 标量及 96 项 EL1/EL2 MPU 直接索引定义，不增加目录类别。软件验收见唯一账本 C03～C06；尚未执行目标 ARM 指令或实板验证，最终发布另行验收。

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
selector_command = "aarch64 r52_select"
isb_command = ""
tcl_endpoint = "localhost:6666"
[registers.targets]
"core0" = "soc.r52.0"
"core1" = "soc.r52.1"
```

每核覆盖继续使用已有 `cores.registers`。需要 MPU selector 时显式配置上述匹配命令；仅普通 CP15/MPU 总览可以清空 selector。新事务自带真正 ISB，`isb_command` 保持为空，不混用旧 MRC/MCR。已有 Banked/VFP/Timer/PMU/GIC 的显式专用配置继续独立生效。

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

## MPU selector

`aarch64 r52_select el1|el2 INDEX EXPECTED_COUNT` 只接受 16/20/24 容量内的 MPU index；PMU selector 不在新协议范围。`aarch64 debugtui_r52_selector_protocol` 返回 `debugtui-r52-selector-1 external-identity current-el2 dspsr dlr fresh-capacity selector-readback isb scratch-readback stop-on-fault`。

`registers_select` 保留既有 API 和两项样本。新路径不从保存 CPSR 构造旧 Plan；读前检查实际生效目录的身份、容量、selector 和 BAR/LAR 定义、owner、读属性、副作用及客户访问条件。服务锁覆盖当前物理核和线程/frame 0；Scope All 仍只读选中核。

后端在同一事务中核对外部 MIDR/EDSCR、DSPSR/DLR 和正确 bank 的容量原值，实际 count 必须等于请求的 Probe count。原 selector 必须小于 count，RES0/非法值不用于恢复。选择前执行真正 ISB，临时写 selector 后执行 ISB 并回读；读完 BAR/LAR 恢复原 selector，执行 ISB 并回读，再核对完整容量、DSPSR/DLR 和外部身份/状态。R0 逐操作保存恢复及物理回读。没有选择变化时不写 selector。

ISB 使用现有 DPM AArch32 EDITR 路径接受的 T32 编码，最终由 `T32_FMTITR` 排列指令；不把 A32 `0xf57ff06f` 直接交给该格式化器，也不依赖 SCTLR.CP15BEN 或修改控制位。

响应固定为 `midr RAW32 dscr RAW32 dspsr RAW32 dlr RAW32 bank el1|el2 capacity RAW32 original RAW32 restored RAW32 selected RAW32 base RAW32 limit RAW32`。host 严格核对字段顺序、32 位宽、身份、当前 EL2、bank/count/index 和原 selector/恢复值；成功样本的 `access.r52_core` 保留此次事务证明。协议不符不执行 selector；容量改变撤销 Probe；注入故障或恢复证据矛盾立即 FAULT/隔离，无后续注入和自动重试。取消发生在原子请求提交之后时完成当前事务再丢弃结果；帧/核改变也不能发布新有效值，旧值保留原来源。

延后 [native selector 硬件 case](../tests/cases/register-r52-selector-read.md) 使用既有驱动和 `tests/fixtures/r52-native-selector-board.example.json`，核对独立 BAR/LAR、容量、当前 Debug 证据、原 selector、控制值及 peer。默认四项 SKIPPED、零目标 I/O；software_example 只验证流程，不是实板基线。

## 验证边界

`tools/openocd-adapter/tests/r52-transfer.c` 编译实际生产事务头文件，物理 I/O 使用独立模型；手写指令字来自 TRM，覆盖 111 项、4360 个逐操作故障点、768 个独立 bank 容量组合，以及恢复/身份/DSPSR/DLR/EDSCR 变化。低 EL/HDD 拒绝不执行 CPU 指令，容量拒绝无 region 数据读取，无模式/selector/使能/控制 MCR。

Rust 测试覆盖受限编码与协议解析、权限预检查、实际 MI/TCP/Tcl worker、多核隔离、能力 Probe、保存 User 状态、取消/帧变化、旧值和故障隔离。延后驱动默认四项 SKIPPED；实际 EXE 与软件夹具验证驱动流程及错误独立基线拒绝，不能当作 ARM 指令执行或实板证据。

`r52-selector-transfer.c` 编译同一生产头文件，使用独立指令字和物理 I/O 模型，覆盖两 bank 的所有 16/20/24 容量索引、原 selector 相同/不同的成功与逐操作失败路径、非法原值、容量变化、scratch/selector 回读、状态漂移及立即中止。它与完整后端命令入口、目标 ARM 指令和上板验证分别记账。

2026-10-06 的 Windows/Linux 候选已由固定源码和本批补丁完整构建；实际程序验收九项协议、命令帮助、参数/索引拒绝和未 examine target 的精确错误码。Windows 另通过本机 DLL 闭包、对应源码包及离线配置检查，两平台均包含 J-Link、CMSIS-DAP、ST-Link、FTDI 驱动。检查关闭服务端口，未连接物理探针。`source.lock.json` 仅据此更新软件 build verified 和当前候选摘要；`board_support_verified=false` 保持不变，已安装 xPack 工具未被替换。

当前 Windows EXE SHA256 为 `0820f197803c55ecf756d7b7ef33f6c82ef97821b2764561ad71d32455e779a0`，Linux ELF 为 `f20a92efb849f49ca93c188a32aedb074bba29cb6c695282f56e0e641b4c1a0d`。最终日志、适用源码、候选包及摘要见 [唯一验收账本](registers-readonly-goal.md) 迭代 16。[硬件 case](../tests/cases/register-r52-core-read.md) 尚未上板；候选构建和软件入口不代替最终 DebugTUI 升版、安装、完整回归及非主分支 Release。
