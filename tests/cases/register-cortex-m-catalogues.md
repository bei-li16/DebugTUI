# Cortex-M 目录、故障字段与手动读取环境用例

本批不执行上板，以下用例默认 **SKIPPED**。运行前逐项填写实际型号/revision、探针、OpenOCD/GDB 版本与哈希、连接参数、每核 AP/target/端口及 CorePrivate binding，确认独立测试固件和无其他调试客户端。使用隔离工程与日志目录；不改客户配置，不自动写 enable/priority/MPU/cache control。

## M-CAT-H01：M3/M4/M7 的公共与增量定义

前置：选择实际目标对应 CPU preset 和 ELF；启用 GDB 日志。操作：`registers_list` 核对 CPUID、SCB/SysTick/MPU/Debug 及 source；M3 不含 FPU/cache 增量，M4 含 FPU，M7 含 FPU/cache/TCM。核对独立手册或固件报告的 CPUID，确认继承来源和差异。M3 缺 revision 对应布局时 VTOR 仍显示原始值及字段缺口。预期：目录选择不发读写或暂停请求。证据：完整 JSONL、有效配置及 `registers_list` 结果；恢复：关闭隔离会话。状态：SKIPPED。

## M-CAT-H02：故障位与地址

前置：独立测试固件产生已知 MemManage/BusFault/UsageFault/HardFault，停在其处理函数；由固件在 RAM 保存进入处理时的 SCB 状态，不由 DebugTUI 触发危险访问。操作：请求 `registers_read` 的 `scb.cfsr`、`scb.hfsr`、`scb.mmfar`、`scb.bfar`，与 RAM 记录及地址有效位核对，展开位域。预期：字段来自同一原始值；地址有效位为 0 时不声称地址有效；展开字段不增加物理读取。证据：RAM 报告、JSONL、MI/Tcl 请求和界面记录；恢复：结束会话，由测试人员重置独立固件。状态：SKIPPED。

## M-CAT-H03：SysTick / DHCSR 手动单次读

当前运行态入口尚待独立验收，本用例要求目标已由 GDB 暂停。

前置：测试固件已有 SysTick 配置，记录其独立状态；GDB/OpenOCD 正常管理核状态。操作：自动请求 `systick.ctrl`、`dcb.dhcsr` 后核对无内存读取；分别以 `manual=true` 请求一次，核对每项恰好一次、显示原始值及来源；再自动请求确认不重复访问。独立读取 LOAD/VAL/CALIB。预期：不直接轮询 DHCSR、不自动使能/重写 SysTick；COUNTFLAG 是否清除另行记录实际访问属性，不能把软件读清规则外推到 debugger read。证据：操作前后固件记录与完整传输日志；恢复：结束会话，不写回读清位。状态：SKIPPED。

## M-CAT-H04：异构双核及未实现项

前置：M7+M4 等实际异构环境，两个独立物理 route，参照 `register-structured-policy.md` 的同地址隔离用例。操作：各核读取自己的 CPUID/SCB；检查 M7 cache 定义不串入 M4，错误与旧值不跨 owner；仅查看 WO cache maintenance 项。预期：不因地址相同复用私有值，查看 WO 产生零 I/O，未验证的 FPU/容量仍为 Unknown，不当作 0。证据：两个 target 的独立日志和样本 owner/context；恢复：关闭隔离会话。状态：SKIPPED。

建议 JSONL 操作（在工具与正确物理 binding 就绪后执行）：

```jsonl
{"id":1,"method":"registers_list","params":{}}
{"id":2,"method":"connect","params":{}}
{"id":3,"method":"registers_read","params":{"ids":["systick.ctrl","dcb.dhcsr"],"manual":false}}
{"id":4,"method":"registers_read","params":{"ids":["scb.cfsr","scb.hfsr","scb.mmfar","scb.bfar"],"manual":true}}
{"id":5,"method":"registers_read","params":{"ids":["systick.ctrl","dcb.dhcsr"],"manual":true}}
{"id":6,"method":"quit","params":{}}
```

引用保持：`G:\Data\GitFiles\DebugTUI\tui-debugger-spec.md`、`G:\Data\GitFiles\ARM\File\Arm® Cortex®-R52 Processor Technical Reference Manual.pdf`、`G:\Data\GitFiles\ARM\File\Armv8-R AArch32.pdf`。
