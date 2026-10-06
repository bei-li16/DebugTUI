# Cortex-M ID、动态容量和 NVIC 来源环境 case

状态：以下全部 **SKIPPED**，本版未执行上板。准备 case 不授予参数文件 `verified=true`。缺少环境时不连接、不产生目标 I/O。

执行前填写目标型号/revision、探针、OpenOCD/GDB 版本和哈希、测试 ELF、SVD 路径和版本、每核 AP/target/GDB/Tcl 端口、CorePrivate binding、测试固件状态、输出目录和恢复步骤。使用隔离工程；确认目标上电、授权状态及无其他调试客户端。由现有 GDB 控制 halt/run，不通过 Tcl 抢发运行控制或切全局 target。

## M-ID-H01：M3/M4/M7 的身份与实例（SKIPPED）

每种型号各执行一次。停在物理 frame 0，选择相应 preset，先记录独立 Arm 调试器/已知固件和手册对应的 CPUID、ICTR、MPU_TYPE。TUI 执行 Probe capabilities，保存 Log、`registers_list` 和 `registers_matrix` 报告。核对 CPUID 模型/revision、owner、实际 target/endpoint、精确地址与当前上下文；逐项核对 bank 上限、MPU 容量和原始编码。启用一个有效 NVIC bank 项并读一次，随后请求超出观测容量的已定义 bank，期望零数据 I/O并显示未实现。不得通过写入来制造不同容量。

预期：容量来自本核 ID。未知/非法响应不显示为 0 个实例；适配且合法的零 MPU 容量显示未实现。ICTR 上限不能冒充实际 IRQ 数。恢复：退出新增 Watch，保留目标控制寄存器原值，按测试固件正常断开连接。

## M-ID-H02：身份冲突及未使能 DWT（SKIPPED）

先以真实核对应的目录探测，记录 DEMCR。若 TRCENA 原本为 0，直接验证 DWT_CTRL 不发送读取，显示 NeedEnable/容量 Unknown；本 case 不自动清除或置位 TRCENA。原本为 1 时验证正常 ID 读取并记录 NUMCOMP。另在隔离工程显式选择不同的 M 型号目录，执行探测。

预期：不匹配型号只读取 CPUID，后续可选 ID 零 I/O；配置容量不能消除冲突。探测不读 DHCSR/SysTick CTRL、不发 enable/write_memory/IPR/cache 维护命令。恢复正确 preset；核对 DEMCR 与前值一致。

## M-ID-H03：优先级和真实中断来源（SKIPPED）

分别准备：有效且 CPU 匹配的 SVD；仅显式 priority_bits；二者冲突；二者均缺失；错误 CPU 的 SVD。SVD 的 IRQ 编号/名称以芯片手册与固件向量表独立核对。每种配置重连到当前停止上下文后探测，在一个 IPR 项的 Status 中查看来源、IRQ 名称和冲突，再导出 JSON。

预期：有效 SVD 优先，其次显式配置；缺失为 Unknown。两份声明都保留，错误 CPU 的 SVD 不授予本核 priority/IRQ 信息。超出观测/架构范围的 SVD IRQ 有明确诊断，不扩大实例；没有 IPR 写探测。查看/滚动 Status 不增加 I/O。恢复原隔离配置。

## M-ID-H04：异构双核、Scope All 与过期响应（SKIPPED）

使用 M7＋M4，配置独立 AP/target/端口和各核 binding，独立记录两个 CPUID 与容量。Scope All 下选择 M4 探测，检查只有 M4 路线产生 ID I/O；切到 M7 后检查未探测信息为 Unknown，再探测并核对其自己的返回值/来源。切回 M4 确认没有复用 M7 的容量、错误或优先级声明。在另一次探测过程中由 GDB 恢复运行，检查剩余 ID 不继续读取、局部证据不发布。

预期：同 PPB 地址在两个核上指向不同实例；Scope All 不广播探测；运行或上下文变化丢弃当前 probe，旧证据不授予新上下文访问。记录工具请求次数。恢复：由 GDB 按独立测试流程暂停各核、清除本次 Watch 并断开，不修改 selector/enable/权限。

核心参考路径：

```text
G:\Data\GitFiles\DebugTUI\tui-debugger-spec.md
G:\Data\GitFiles\ARM\File\Arm® Cortex®-R52 Processor Technical Reference Manual.pdf
G:\Data\GitFiles\ARM\File\Armv8-R AArch32.pdf
```
