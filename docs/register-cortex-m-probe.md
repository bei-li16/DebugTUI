# Cortex-M 身份、容量与 NVIC 来源

本版使用现有 `registers_probe` / TUI 的 Probe capabilities，增加 M3/M4/M7 探测，复用当前 worker 的 CorePrivate 路线、MI 内存读取或显式 Tcl target 通道。探测仍要求当前物理核、frame 0 和 STOPPED；安全运行态入口由 B11 单独验收。没有新增 GDB 调试器或 writer。

先读取 CPUID，确认 implementer、architecture、PARTNO 与所选适配核一致。未知、未适配或型号冲突时保留 CPUID 原值和原因，不读取后续可选 ID。不能用所选目录或配置事实代替实际身份。每项有效依据必须是同一 owner/context 的完整 32 位 CorePrivate 样本，具有已响应请求、精确地址、访问区间和已知路线。

通过身份核对后读取 ICTR、MPU_TYPE、DEMCR、FP_CTRL；仅在观察到 DEMCR.TRCENA=1 时读取 DWT_CTRL，M4/M7 再读取目录已有的 MVFR 项。探测不读取 DHCSR、SysTick CTRL、维护命令，也不写 enable、IPR 或 MPU/cache 配置。探测期间的依据只保存在本地事务中，通过最终 thread/frame/context 检查后才发布。运行通知、取消或上下文改变丢弃结果，停止后续 I/O。

## 有效能力与原始值分别保留

- ICTR.INTLINESNUM 的合法实现编码为 0～7，bank 数为字段值加 1，中断线数上限为 bank 数乘 32。最后一组上限为 256，但这些型号最多 240 个外部 IRQ。这个上限不能当作实际中断列表或所有位均已实现的证据。[M4 TRM DDI 0439B §6.3.2，物理 PDF 页 65](https://documentation-service.arm.com/static/5f19da2a20b7cf4bc524d99a)
- MPU 容量来自 MPU_TYPE.DREGION。校验 unified MPU、IREGION 和保留位，以及适配的实现编码；M3/M4 为 0 或 8，M7 为 0、8 或 16。有效零容量表示没有 MPU；非法编码、读失败或身份未知保持 Unknown，不依据厂商芯片名填写容量。[M3 TRM](https://documentation-service.arm.com/static/5e8e107f88295d1e18d34714)、[M4 TRM §5.1](https://documentation-service.arm.com/static/5f19da2a20b7cf4bc524d99a)、[M7 配置选项](https://developer.arm.com/-/media/Arm%20Developer%20Community/PDF/Processor%20Datasheets/Arm-Cortex-M7-Processor-Datasheet.pdf?hash=5C37DFAA43B532B8BD2DB277685F76007FCFEAE5&la=en&revision=35233191-7cdb-429e-9170-e07ad9d87c7a)
- DWT 未使能显示 NeedEnable，容量 Unknown，不将被门控的零值解码成“不存在”。FPB 容量保留完整的非连续 NUM_CODE 位，未知 revision 不授权实例。FPU 的零值/失败保持 Unknown；本批仅接受已核实的正向 MVFR0 编码，GDB 名称和访问能力仍独立检查。
- 原始 ID 样本、已解码能力和配置声明分别展示。非法原始编码不会进入有效 `present_if` 字段事实；配置不能伪造这些观测。最新同上下文失败或身份不再成立会撤销对应有效字段/容量，较早 probe 不复活旧依据。

## NVIC 优先级及中断名称

优先级位数依次使用 SVD `/device/cpu/nvicPrioBits`、显式 `registers.facts.nvic.priority_bits`。适配范围为 3～8；非法显式配置在连接前拒绝，缺少可用依据显示 Unknown。SVD 值和配置值同时保留，冲突时报告双方并使用有效 SVD 值；SVD CPU 与所选核冲突时，其优先级和 IRQ 声明不用于该核，仍可使用有效的显式配置。[CMSIS-SVD CPU 定义](https://arm-software.github.io/CMSIS_5/SVD/html/elem_cpu.html)、[M3 TRM](https://documentation-service.arm.com/static/5e8e107f88295d1e18d34714)、[M4 实现选项](https://developer.arm.com/-/media/Arm%20Developer%20Community/PDF/Processor%20Datasheets/Arm%20Cortex-M4%20Processor%20Datasheet.pdf?hash=854DC03108CBAE964E37A15A715418F1AB04E293&la=en&revision=904a9fc1-9c66-4816-80bf-ff8c76420e5a)

单核显式配置示例（不表示硬件探测）：

```toml
[registers.facts]
"nvic.priority_bits" = 4
```

多核可在相应 `cores.registers.facts` 中声明，遵循既有每核 map 覆盖规则。SVD 路径沿用当前工程 `program.svd`，不新增配置层；异构核不能共用一份声明错误型号的 SVD，缺少适合该核的 SVD 时显式配置可提供优先级位数，实际 IRQ 列表仍为 Unknown。

`probe.nvic` 单独保存优先级来源、SVD CPU、原配置/SVD 值和 interrupt 列表，声明不冒充物理 ID 观测。IRQ 保留 peripheral/name/value/description，包括合法的共享编号与外设别名；列表条目数不等于独立 IRQ 个数。超出 ICTR 上限或 240 个 IRQ 架构边界的 SVD 条目报告冲突，不据此增加实例。

在 NVIC 寄存器的 Status 中查看优先级值与来源、配置冲突及 SVD 列表来源；IPR 项还显示匹配 IRQ 编号的外设/名称/说明。过期上下文显示 Unknown，不使用另一核证据。查看 Status、展开和格式操作不增加 I/O。JSON `registers_list` / `registers_matrix` 保留本核 probe 与有效事实，Log 输出原始样本和来源。

软件验证见 `tests/m_capability_access.rs`、`src/registers/m_profile/tests.rs` 及 Status 测试：三种型号的生产 MI 路径、实际 Tcl CorePrivate 内存通道、M4/M7 两个独立 worker、Scope All 仅探测选中核、合法/非法/缺失 ID、容量门禁、SVD/配置来源、运行期间丢弃及零写入。夹具提供原始字节，不模拟 ARM 实板。延后硬件用例见 [环境 case](../tests/cases/register-cortex-m-probe.md)，默认 SKIPPED。

本批不等于完整 B05 region 事务、B06/B07/B08 全部模块验收、B10 全生命周期或 B11 运行态入口完成；后续沿用这些 ID 完成对应验收。

核心参考文档（保留绝对路径）：

```text
G:\Data\GitFiles\DebugTUI\tui-debugger-spec.md
G:\Data\GitFiles\ARM\File\Arm® Cortex®-R52 Processor Technical Reference Manual.pdf
G:\Data\GitFiles\ARM\File\Armv8-R AArch32.pdf
```
