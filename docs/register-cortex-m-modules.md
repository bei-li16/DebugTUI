# Cortex-M Debug/DWT/FPB 与 FPU 模块验收

本批对应冻结 Goal 的 B06/B07。适用 Cortex-M3/M4/M7 的 Debug/DWT/FPB，以及 M4/M7 已有的 FPU 配置和 GDB 寄存器堆。复用公共目录、ID Probe、CorePrivate、统一条件和 GDB Reader；未增加指令注入器或 writer，没有执行上板。

## 定义、实现与使能分别判断

- DHCSR 保留原始状态和字段，只允许明确的手动单次读取。Probe、普通自动刷新、状态查看和帮助不读取它；核状态仍由 GDB/OpenOCD 管理。手动读取会影响粘滞位，不能将它当成普通轮询源。
- DEMCR.TRCENA 是 DWT 读取条件。新鲜同核观测为 0 时显示 NeedEnable；未观测或新观测失败时显示 Unknown。没有自动置位。使能尚未证明时，DWT_CTRL 的零或旧 NUMCOMP 不证明模块不存在；证明使能后观测的零容量是已知零。后续使能位关闭或读失败，撤销此前容量。
- FP_CTRL 的 NUM_CODE 使用两个位段 `[7:4]`、`[14:12]`，NUM_LIT 使用 `[11:8]`，REV 使用 `[31:28]`。适配 REV 0/1，保留零容量；未知 revision 或读取失败保留诊断但不授予容量事实。不启用 FPB，不修改比较器，也不扩大本版到完整断点/Trace 功能。
- 可选 FPU 必须由匹配 CPUID 和有效 MVFR0 得到正向证明；配置声明、零 MVFR0、异常编码、读取错误和旧样本都不能代替该证明。FPU 配置和 D/S/FPSCR 入口在证明缺失时发送零数据读取，仍保留原始 ID 及限制原因。

DHCSR 与 FP_CTRL 的规则沿用 [M 目录来源](register-cortex-m-catalogues.md)，有效性规则沿用 [M 身份与容量](register-cortex-m-probe.md)。事实始终绑定实际 owner、当前物理上下文与已完成的 GDB/Tcl 请求。

## CPACR 与 GDB 浮点读取

固定 CMSIS 头文件提供 CPACR 地址而没有 CP10/CP11 宏；生成器现补入两个 M 核手册的位域来源：M4 的 `DUI 0553A` §4.6.1/Table 4-50，物理 PDF 第 264 页；M7 的 `DUI 0646B` §4.7.1/Table 4-59，物理第 287 页。CP10 为 `[21:20]`，CP11 为 `[23:22]`，保留禁止、仅特权、保留/不可预测及完全访问四种编码，不合并不同值，也不推断当前软件模式。目录来源指向 [M4 官方指南](https://documentation-service.arm.com/static/5f2ac4ab60a93e65927bbdbf) 和 [M7 官方指南](https://documentation-service.arm.com/static/61efd6602dd99944d051417b)。不填未经本目标证明的复位值。

CPACR 描述处理器执行 FP 指令的权限。DebugTUI 的 M 浮点读取继续经 GDB regfile，名称、停止上下文、真实响应和 FPU 实现证明各自检查；不会因为 CPACR 显示禁止就自动使能，也不会把 CPACR 的值当成外部调试授权。失败保留后端错误，不能显示零值或旧值为有效。该区分依据 [OpenOCD armv7m.c](https://openocd.org/doc-release/doxygen/armv7m_8c_source.html) 的 D0–D15/FPSCR 目标描述及 [cortex_m.c](https://openocd.org/doc-release/doxygen/cortex__m_8c_source.html) 的外部寄存器传输路径；软件夹具不证明某块板卡在所有 CPACR 状态下必然可读。

FPCCR/FPCAR/FPDSCR 和 MVFR 的现有 CMSIS 字段继续复用。D0–D15 为 64 位 GDB 原始存储，S0–S31 从同一个父 D 样本派生；同批 D0/S0/S1 只读一次 D0，保留完整 NaN payload、负零、高低半部和同一采样来源。M4 的 D 名称不意味着处理器具有双精度运算能力，不创建 NEON/Q 视图。若后端只提供 S 名称而缺少 D 父项，本版明确 ReaderUnsupported，不用额外表达式求值或注入绕过。

CPACR 字段 Status 在 35×12/80×24 显示权限枚举、说明和准确来源页码；查看、滚动及枚举展开不发 I/O。ADS 分组、浮点格式和变化高亮的整体复验仍由 D01 跟踪。

## 软件验证与环境用例

`tests/m_capability_access/m_modules_cases.rs` 经实际 Coordinator/worker/MI 验证三个 M 核模型的手动 DHCSR、DWT 使能/容量撤销，FPB 分离计数/零/未知 revision/失败，以及 M4/M7 FPU 配置与 D/S/FPSCR、错误编码、缺名、失败旧来源。独立供给原始字节和 GDB 位模式，不在夹具中实现被测门禁。模型单元测试补已使能 DWT 零容量和 CPACR 保留编码；实际 UI 单元测试补窄宽字段详情。

延后用例见 [M 模块环境案例](../tests/cases/register-cortex-m-modules.md)，可用驱动 `scripts/test-m-profile-modules-hardware.cjs` 与独立期望值模板 `tests/fixtures/m-profile-modules-board.example.json`。默认五项 SKIPPED，零目标 I/O。显式 `--run --software-fixture` 的集成测试运行实际 EXE、五阶段及关闭会话；故意错误的独立 D0 期望值必须失败。它验证驱动，不计实板通过。软件例子的 FPB 容量与位模式刻意覆盖算法边界，不能照抄为真实 M4 的硬件配置。

本批不证明 B08 cache/TCM、B10 完整异构生命周期、B11 运行态入口或 C03 R52 当前 Debug 权限；仍以实时清单验收。

核心参考文档保留绝对路径：

```text
G:\Data\GitFiles\DebugTUI\tui-debugger-spec.md
G:\Data\GitFiles\ARM\File\Arm® Cortex®-R52 Processor Technical Reference Manual.pdf
G:\Data\GitFiles\ARM\File\Armv8-R AArch32.pdf
```
