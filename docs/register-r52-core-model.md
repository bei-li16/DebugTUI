# R52 身份、控制与 MPU 描述

本批完善已有 R52 定义，未增加寄存器类别或 writer。目录继续由离线生成器产生；`scripts/register_r52_core_metadata.py` 保存经过手册复核的编码、主要字段、正常执行的访问条件、结构化来源与可信度。R52+ 入口继承这些 R52 参考定义，但明确保留身份和差异未验证提示。

| 寄存器 | TRM 节与实际 PDF 页 | 本批内容 |
| --- | --- | --- |
| MIDR | 4.3.69，169–170 | Implementer/Variant/Architecture/PartNum/Revision，保留 revision 矛盾 |
| MPIDR | 4.3.78，182–183 | Aff0/1/2、M/U/MT，不从配置序号推导 affinity |
| SCTLR / HSCTLR | 4.3.92，200–203 / 4.3.53，144–147 | M/BR、cache、执行与异常配置；SCTLR.FI 为 HSCTLR.FI 的 RO 副本 |
| CPACR | 4.3.1，79–81 | cp10/cp11、ASEDIS/TRCDIS；cp11 不同于 cp10 时保留 UNKNOWN 解释 |
| HCR | 4.3.39，120–125 | 区分 TRVM 读陷阱与 TVM 写陷阱、各 ID trap、VM 与其他控制 |
| MPUIR / HMPUIR | 4.3.77，181–182 / 4.3.47，135–136 | 各自容量，EL1 16/20/24，EL2 0/16/20/24 |
| PRSELR / HPRSELR | 4.3.87，195–196 / 4.3.50，139–140 | 16 区使用 [3:0]，20/24 区使用 [4:0]；零容量不得写 selector |
| MAIR0/1 / HMAIR0/1 | 4.3.70，170–172 / 4.3.45，131–133 | Attr0–7 的各个 byte、Device/Normal/保留编码说明 |
| HPRENR | 4.3.46，133–135 | 有效 region 的 enable bitmap，容量以外位 RAZ |
| PRBARn/PRLARn | 4.3.85/4.3.86，192–194 | n=0–23 的已有 direct 编码、BASE/LIMIT/AP/SH/XN/EN/AttrIndx |
| HPRBARn/HPRLARn | 4.3.48/4.3.49，136–139 | 各自 direct 编码及 EL2/EL0–EL1 权限含义 |

`source.page` 使用一基实际 PDF 页。MIDR 的表 4-1 复位值为 `0x411FD134`，但第 170 页 Revision 行写 `0x5 r1p4`；MPIDR 的 affinity 位域与其文字的 least/most-significant 描述不一致；PRSELR 的容量标题写 EL2，而本节名称/编码指向 EL1。这三处在目录描述中标为 `Source conflict`，confidence 为 medium，不自行裁决。位置与编码仍保留明确来源；不固定 MIDR revision，也不从这些文字推导拓扑或错误容量。REGION 显示最多五位，说明 16 区时 bit 4 是 RES0，合法选择始终小于该 bank 的实际容量。

只有整寄存器复位值有可靠依据时填写：CPACR=0、HCR=2、HPRENR=0。可配置的 MPU 数量、SCTLR/HSCTLR 的 EE/TE、MPIDR、MAIR 及 region 地址不填猜测的 reset。PRLAR/HPRLAR 的 EN 在复位时为 0，但其他位 UNKNOWN，因此不把整个寄存器复位值填成 0。

现有 MPU 模型继续处理各 bank 的独立容量、EL2 的零容量、64 字节地址边界与八个 MAIR attribute；不改容量规则或重新实现解码。direct region 读取不写 PRSELR/HPRSELR。仅 HCR/SCTLR 的描述或 RW 属性不提供 debugger 权限或 writer。

目录写回时省略默认空的字段枚举列表，仍兼容输入中的显式 `enums=[]`，非空枚举完整保留。这样新增手册元数据后的 R52/R52+ 目录仍能在原有 4 MiB 限制内序列化并重新加载，用户覆盖与 Setup 详情沿用同一生产加载器；不提高输入资源上限。软件测试同时检查实际内置目录的完整写回和再次解析。

这里完成 C01/C02 的描述、编码与容量模型验收。normal-execution 的 EL/trap 说明不能充当注入许可，停止前 CPSR/DSPSR 不能证明当前 Debug EL。后续已接通普通 CP15、MPU 和 selector 的统一当前证明，并通过恢复、拒绝及完整后端候选的软件验收；当前协议与边界见 [受保护只读访问](register-r52-core-read.md)，实时 C03～C06 状态和证据只维护于 [唯一账本](registers-readonly-goal.md)。这些软件结果不代表实板通过。

软件模型测试检查内置实际 TOML 的来源、位宽、reader、字段、复位边界、冲突及每个 direct region 的独立 bank 条件；既有 MPU/MAIR 解码测试保持。它们不模拟 ARM 指令执行，也不证明实际 AP 归属。六项 [延后环境 case](../tests/cases/register-r52-core-model.md) 均 SKIPPED，不升级 hardware verified。

核心参考文档（绝对路径保留）：

```text
G:\Data\GitFiles\DebugTUI\tui-debugger-spec.md
G:\Data\GitFiles\ARM\File\Arm® Cortex®-R52 Processor Technical Reference Manual.pdf
G:\Data\GitFiles\ARM\File\Armv8-R AArch32.pdf
```

指定 TRM 为 `100026_0104_01_en`/718 页；指定架构 PDF 为 `DEN0130_0100_en`/26 页的介绍资料，不代替完整指令和 External Debug 权限规则。
