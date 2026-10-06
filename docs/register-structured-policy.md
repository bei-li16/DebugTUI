# 结构化条件、未知归属和 CorePrivate

本批落实只读 Goal 的公共模型。M3/M4/M7 正式系统目录、ID 解码、运行态读取与 R52 当前 Debug EL 的生产后端接入仍按 B/C 项验收，不因 schema 已存在而宣称完成。

## 定义与判定

寄存器可声明有限的字段比较，无表达式或脚本：

```toml
present_if = { reg = "mpu_type", field = "DREGION", op = "ge", value = 1 }
access_rule = { need_halt = true }
```

`op` 支持 `eq/ne/lt/le/gt/ge`。比较字段必须存在、可读、无父链读副作用，与本项 scope 相同，比较值不能超出字段宽度。字段条件与 Alias 一起检查依赖环及深度。字段宽度支持至 64 位，包括既有非连续位段；原始寄存器位宽仍沿用现有模型。

`present_if` 决定实现状态：真为 Yes，假为 No，缺少可靠字段观测为 Unknown。旧 `conditions` 继续有效，别名的全父链条件均参与判定。新的存在性未知时，手动读也不绕过门禁。

访问条件独立于存在性：

```toml
access_rule = { need_enable = { reg = "demcr", field = "TRCENA", op = "eq", value = 1 } }
# 另一个需要当前 Debug 权限的寄存器：
access_rule = { min_el = 2 }
```

`need_enable` 为假时返回 FeatureDisabled / NeedEnable；没有观测时返回 Unknown。不会自动使能、自动补读依赖或因为手动操作忽略条件。用户先手动读无副作用的源寄存器，或运行适配的 capability probe。

`min_el` 接受 0～3，必须由当前事务的后端 Debug-state 证据满足；配置 `cpu.mode`/`cpu.debug_el` 或停止前 CPSR 都不能满足它。本批通用入口尚未接入这样的后端证据，因此声明 min_el 的通用读取在数据请求前返回 Unknown。纯策略函数已有已证明 EL、低 EL 和缺证据的正/负测试；R52 生产成功路径由 C03 继续接通，不将通用拒绝视作 R52 权限已完成。

省略 `need_halt` 保持停止态要求。只有最终 reader 为 MMIO/CorePrivate 才能声明 `need_halt=false`；别名不能降低父项要求。实际通道、后端和请求入口还会检查自身限制。本批公开读取入口仍要求 STOPPED，运行态的 UI/采样接入属于 B11。

## 观测证据与来源

字段只从当前 frame 0、同一会话/停止代次/读取核心、已知且匹配的 owner、PhysicalCore、Valid 且完整宽度的样本中提取。样本须保留匹配的 catalogue reader、实际 Responded 请求、上下文与完成时间；CorePrivate 的实际地址和宽度也必须匹配。配置或有值但缺少上述证据的记录不能冒充观测。

同一来源寄存器以当前上下文中最新的记录为准；新失败/过期不重新使用旧 probe 的字段，迟到的更旧记录不覆盖更新证据。core/cluster/chip 的源字段核对显式 topology；没有对应 owner 时维持 Unknown。未知 scope 不创建 owner、不读、不进入自动队列。

内部字段事实使用保留的 `register.` 命名空间及长度编码，避免寄存器/字段名含点时冲突；`registers.facts` 禁止声明这个命名空间。界面显示可读的 `寄存器.字段` 与比较关系，保留当前实际值、源样本、owner、时间和真实请求。失败旧值的 eligibility 保持原来源，不改成此次失败来源。

## CorePrivate 路由

```toml
reader = { kind = "core_private", address = 0xe000ed00 }
scope = "core"
```

地址是绝对 PPB 地址，允许对齐的 8/16/32 位访问，整个访问须落在 `0xE0000000..0xE0100000`（末端不含）。PPB 是每核独立视图，不能使用全局 `components.ppb` 作为回退。

工程路由示例（target/AP 映射须按实际 OpenOCD 配置复核）：

```toml
[[memory_access]]
id = "m4-ppb"
label = "M4 PPB"
tcl_endpoint = "localhost:6666"
target = "soc.m4"
cores = ["core.2"]
while_running = false

[registers.component_owners.ppb."core:core.2"]
base = 0
channel = "m4-ppb"
little_endian = true

[registers.targets]
"core.2" = "soc.m4"
```

binding 的 base 必须为零；通道必须恰好只属于所读核心，不能使用空 cores（共享）或多核通道；声明寄存器 target 时须与通道 target 一致，同一 endpoint/target 不能另分配给其他核。请求使用具名 `<target> read_memory`，不切全局 targets，不 halt/resume、不写内存。

显式 `channel=""` 可复用本 worker 的 GDB memory。要求实际已知连接与配置 endpoint 一致，且配置没有将该 endpoint 交给另一核心；不透明 GDB 连接变更后拒绝。此路径保持停止态要求，不重写 DCRSR/DCRDR。上述校验证明配置路线及实际请求的隔离，不证明用户配置中的 target/AP 在实板上的映射正确。

## 软件验证与延后环境用例

`src/registers/policy/tests.rs` 验证有限比较、保留命名空间、当前物理证据、失败/迟到、原请求来源、shared owner、EL/enable/halt、非法图和路由。UI 测试验证未知/未启用/权限未证明项目不进入自动队列；既有八类 reader、字段、别名、生命周期测试继续回归。

`tests/register_policy.rs` 运行生产 Coordinator/MI/两个 TCP target：相同地址不同核，容量/使能的正负门禁，Alias 单次父读取，副作用自动零 I/O，unknown owner 和配置 EL 不授予读取；另验证既有 GDB memory 的实际通路及 EXE 配置加载/连接前拒绝。服务器提供独立原始响应和严格请求断言，不模拟 ARM 指令或真实芯片。

延后步骤见 `tests/cases/register-structured-policy.md`，全部 SKIPPED。M 内置目录及 ID 成功探测尚未验收，因此 B10 的完整异构目录/探测/缓存支持不在本批计为完成。

核心参考文档保留绝对路径：

```text
G:\Data\GitFiles\DebugTUI\tui-debugger-spec.md
G:\Data\GitFiles\ARM\File\Arm® Cortex®-R52 Processor Technical Reference Manual.pdf
G:\Data\GitFiles\ARM\File\Armv8-R AArch32.pdf
```

M 核示例地址/字段另对照官方 [MPU_Type](https://arm-software.github.io/CMSIS_6/latest/Core/structMPU__Type.html)、[CoreDebug_Type](https://arm-software.github.io/CMSIS_6/latest/Core/structCoreDebug__Type.html) 结构和核头文件；副作用不能仅由 RO/RW 宏推断。CMSIS 数据转换与完整来源清单在 B01 的目录批次交付，不分发 ADS 配置。
