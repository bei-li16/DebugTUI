# Cortex-M 系统寄存器目录与生成

内置 `armv7m-common`，由 `cortex-m3`、`cortex-m4`、`cortex-m7` 继承；M4 增量包含 FPU 配置/身份，M7 增量包含 FPU 与 cache/TCM 配置。型号差异使用完整 `override=true` 定义，加载结果保留父文件及覆盖来源。Setup 可以直接选择三个型号。芯片/AP/target 绑定仍来自实际连接配置，目录不证明实板已适配。

地址、访问元数据及字段来自 [固定 CMSIS-Core 输入](../third_party/cmsis-core/README.md)，6.1.0 的 commit 为 `b0bbb0423b278ca632cfe1474eb227961d835fd2`。四个输入文件均校验 SHA256，原版权和 Apache-2.0 声明完整保留。`source` 记录精确头文件、提交和一基行号，CMSIS 未提供的 reset 不填写；未经上板的定义没有 `verified` 标志。

在仓库根目录执行：

```powershell
python -X utf8 scripts/generate-register-catalogues.py --check
python -X utf8 scripts/test-cmsis-registers.py
```

去掉 `--check` 才更新受版本管理的产物。流程完全离线，固定选择本版需要的结构体成员，解析成员偏移和数字位掩码；不执行预处理器、表达式脚本或头文件代码，不根据厂商芯片名猜测数量。未知表达式、冲突宏和字段重叠都阻止生成。

SCB 包含 CPUID、ICSR、VTOR、AIRCR、SCR、CCR、SHCSR、CFSR/HFSR/DFSR、MMFAR/BFAR/AFSR。CFSR 使用独立故障位，不把聚合 mask 与位段叠加。DHCSR 使用读状态视图，排除与状态重叠的写 DBGKEY；AIRCR 使用读键 VECTKEYSTAT，并排除写键和废弃拼写别名。MPU RASR 使用属性字段，排除 aggregate ATTRS。

M3 的 VTOR 宏有 r2p1 前后的条件分支，缺少观测 revision 时保留原始值，并以 `fields_missing=true` 说明缺口；M4/M7 有各自明确的位域覆盖。不由“最后一个宏”默默选定 M3 的布局。

NVIC 最多展示头文件定义的 8 个 bank 与 240 个 priority byte。实际可读 bank/byte 由当前物理 ICTR.INTLINESNUM 门控，这个字段仅代表中断数量上限；实际中断列表、优先级位数仍需要 SVD 或显式配置。本批不将这些定义数量记作板级数量或动态探测验收。MPU TYPE 提供 DREGION 条件；RBAR/RASR 展示当前 selector 的值，不隐式选择新 region。完整 region 事务单独验收。

后续 ID 接入现已完成，见 [M 身份、容量与 NVIC 来源](register-cortex-m-probe.md)：有效条件还要求当前物理 CPUID 与选定适配型号匹配，并校验容量编码。SVD/显式优先级来源、实际 IRQ 声明及 Unknown/冲突在 Probe/Status 中可见。这里的目录数量仍不是实板成功读取数量。

所有新 PPB 定义使用 CorePrivate，读取必须满足 [私有核路线约束](register-structured-policy.md)。没有新 writer。现有通用 GDB 定义与 writer 保留；M4/M7 D/S 视图仍复用 GDB/同源 Alias，新增 MVFR0 条件，仅真实物理字段观测可以满足该条件。旧 GDB 定义未补造手册来源，confidence 仍为 Unknown。

FP_CTRL 的地址与非连续 NUM_CODE 使用 [DDI 0403E.e §C1.11.3、物理 PDF 页 756–757](https://documentation-service.arm.com/static/606dc36485368c4c2b1bf62f)；REV、NUM_LIT 与 NUM_CODE 分开，不截断高 3 位。DWT CTRL 要求已有 DEMCR.TRCENA 观测为 1，不自动使能。

DHCSR 与 SysTick CTRL 均采用手动单次读取策略。DHCSR 读清状态依据同手册 §C1.6.1、物理页 701。SysTick §B3.3.2、物理页 621 区分软件读清 COUNTFLAG 与架构调试器读保留；当前通道尚没有可验证的 debug-access 属性，因此依本 Goal 保持保守的 manual-only 策略，不能把所有调试器读都宣称有读清副作用。LOAD/VAL/CALIB 可独立读取，读策略不授予写能力。

M7 cache maintenance 项仅展示 WO 定义，不进入读取或执行队列。CCSIDR 是当前 CSSELR 所选项的值；完整 selector 事务、动态身份/容量、安全运行态读取、真实旧包升级及硬件适配仍需后续验收，不能因目录可加载而全部勾选。

本版核心参考文档（保留绝对路径）：

```text
G:\Data\GitFiles\DebugTUI\tui-debugger-spec.md
G:\Data\GitFiles\ARM\File\Arm® Cortex®-R52 Processor Technical Reference Manual.pdf
G:\Data\GitFiles\ARM\File\Armv8-R AArch32.pdf
```
