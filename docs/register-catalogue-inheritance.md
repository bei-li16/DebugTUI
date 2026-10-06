# 寄存器目录继承与描述来源

本阶段实现只读 Goal 的 A05/A06/A07/A10。三份核心参考保持绝对路径：

```text
G:\Data\GitFiles\DebugTUI\tui-debugger-spec.md
G:\Data\GitFiles\ARM\File\Arm® Cortex®-R52 Processor Technical Reference Manual.pdf
G:\Data\GitFiles\ARM\File\Armv8-R AArch32.pdf
```

## 继承与覆盖

目录可写 `extends = ["armv7m-common"]`，也可指定 `extends = ["../common.toml"]`。名字先在声明文件所在目录查找 `<name>.toml`，不存在时查内置定义；存在但损坏、不可读或为目录时直接报错。显式路径只相对声明文件解析，不回退内置。无声明路径的 `Catalogue::parse` 可继承内置名字，不能猜相对路径。

最多三层（含根目录）、每节点四个不同父目录、整个解析图 16 节点及 16 MiB 输入；单文件仍限制 4 MiB，最终仍限制 4096 寄存器、256 组。规范化文件路径用于循环检查。不同父目录定义同一寄存器时拒绝歧义，不采用加载顺序决定结果。

子层同 ID 必须写 `override = true`，且提供完整寄存器定义，包括 reader、字段、条件、副作用及来源。缺少字段不能从父条目偷偷补齐。没有父条目时写 override 同样报错。同一文件内重复寄存器或分组报错；继承中同 ID 分组只在完全一致时复用。

加载结果是已展开目录。每项的运行态 origin 记录声明文件、根至声明文件的继承链及显式替换的父文件。JSONL 列表的 `definition_origins` 按寄存器 ID 返回这些诊断信息，Status 也可查看。诊断信息不写入持久化目录，不增加原有目录体积；展开的 TOML 可独立重新加载。输入的 `definition_origin` 字段直接拒绝，不能伪造加载凭据。文件不会因查看或解析而改写。

## 版本与元数据

既有 version 1 保持兼容，没有手册来源/复位值/可信度时显示 Unknown。version 2 每个寄存器必须有 source；父条目的来源先在其声明文件解析，再继承到子目录，不能被子文件的 `[meta]` 改写。

```toml
version = 2
cpu = "example"
architecture = "armv8-r-aarch32"
extends = ["r52-source-common"]
[meta]
document = "Arm Cortex-R52 Processor Technical Reference Manual"
number = "100026_0104_01_en"
version = "Issue 01"

[[registers]]
id = "midr_partnum"
name = "MIDR PartNum"
group = "system"
bits = 16
access = "ro"
reader = { kind = "alias", source = "midr", offset = 4 }
source = { section = "4.3.69; PartNum and Architecture slice", page = 170 }
confidence = "high"
```

source 包含 document、version、可选 number/URL、section 和一基 PDF page；锁定版本的源码头文件可用一基 line 替代 page，不编造 PDF 页码。同一文档可通过 `[meta]` 提供 document/version/number/URL；引用另一文档时必须写完整 document/version，不拼接当前文件的文档信息。零页码、缺章节/版本、未知字段和错误类型均拒绝。

`reset` 是可选无符号数，校验不超出寄存器位宽。未知或因实现变化的复位值省略；省略不会成为 0，也不会触发读目标自检。宽度超过 64 位的目录无需给出虚构复位值。

confidence 支持 unknown/low/medium/high/verified。软件校验不会自动升级为 verified；verified 必须另带 `verification = { date, board, firmware, report }`，日期有效且内容非空。Status 显示“声明的硬件验证”，不把输入报告当作本次已执行上板测试。非未知可信度也必须有来源。`fields_missing = true` 明确字段描述不完整，未描述的位不猜测。

Setup/Status 汇总各可信度数量；Status 显示选中条目的复位值、手册来源、声明文件与继承链，寄存器树对 low/medium 加标记。描述来源与当前硬件身份、权限证明及实际采样 provenance 分开。

## 五项代表样例与验证

[公共样例](../profiles/registers/examples/r52-source-common.toml) 与 [增量样例](../profiles/registers/examples/r52-source-sample.toml) 展开为 MIDR、MPUIR、PRBAR0、EDPRSR 和 MIDR 字段 Alias，共五项。它们只用于格式和加载验收，没有批量迁移正式目录或增加探测流程。

- MIDR/MPUIR/PRBAR0 编码和主要字段核对 R52 TRM §4.3.69、§4.3.77、§4.3.85，PDF 169–170、181–182、192–193 页。PRBAR0 reset 按手册 UNKNOWN 省略；region 存在性使用已有 `mpu.el1.regions` 事实。
- EDPRSR 偏移来自 TRM §12.5，PDF 394 页。SR 读副作用另核对 `G:\Data\GitFiles\ARM\File\DDI_0487_M.b_a-profile_architecture_reference_manual.pdf` §H9.2.43，PDF 15261 页。只标注已核对的 SR 字段，fields_missing/medium 保留，其他 A-profile 新字段不套用到 R52。此条目始终手动读取，不能自动轮询。
- Alias 继续使用现有依赖检查及读策略，不能绕过父条目的条件或读副作用；样例的 16 位片段包含 PartNum 和 Architecture，不将全部 16 位谎称为 PartNum。

专项测试覆盖循环、深度/资源边界、缺父目录、明确覆盖、歧义、字段解码、容量三态、读副作用、来源不可伪造、复位值和可信度校验。实际 CLI 脚本 `scripts/test-register-inheritance.cjs --binary <开发 EXE>` 验证不同核心的展开目录、修改公共定义同时作用于两子目录、八类错误连接前拒绝，以及输入文件哈希保持；全程不启动 GDB、不访问目标。

本阶段尚未完成结构化 present_if/access 规则、M 公共系统目录和动态探测、CorePrivate、运行态 MMIO，以及 R52 普通 CP15 权限统一；这些仍按冻结清单追踪。环境用例见 [配置验收](../tests/cases/register-configuration.md)，保持 SKIPPED。
