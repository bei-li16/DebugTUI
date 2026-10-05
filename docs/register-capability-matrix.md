# 寄存器读取能力矩阵

REG-007 的交付是可导出的能力边界与证据清单。`registers_matrix` 只读取当前 worker 的目录、配置、已缓存 Probe 和样本，在 disconnected、RUNNING、STOPPED、FAULT 均可调用；不连接 GDB、不探测协议、不执行寄存器或内存读取。目录错误按原错误返回。未选择目录时返回 `catalogue=null` 和空矩阵，不从旧 GDB 名称推造位宽。

```json
{"id":1,"method":"registers_matrix","params":{}}
```

返回 schema_version=1、source、context、environment、catalogue、rows、categories、planned_classes、probe/probe_current、effective_facts/fact_source、observations 和 owner_generations。configuration 表示配置声明；配置 endpoint 与已知 connected endpoint 分开保留。矩阵没有硬件支持的自动认证入口，planned_classes.hardware_support 始终为 unverified。

每行包含类别路径、精确 bits、scope/owner、全别名依赖、readable/manual_only、automatic_eligible、实现条件及来源、配置读取计划、状态条件、支持状态和 observation 索引。全目录保留每个依赖项的条件、字段和 access_condition；Probe 和完整观察只保存一次，避免每行重复同一大份原始证据。automatic_eligible 仅表示既有条件引擎是否允许调度，不代表后端已安装、协议通过、访问权限或硬件存在。plan.available 仅表示配置路线具备所需字段；连接、协议、身份、权限仍须实际 reader 核对。

MMIO 计划核对显式 owner binding、核心适用通道、地址溢出、对齐及原生读取位宽；保留全地址字符串、字节序、通道来源及非原子说明。TCL 64 位读取明确是两个 32 位总线字；GDB byte-range 不虚构总线宽度。别名保留自身位宽与物理根计划。显式 mmio_probe 的新鲜证明要求记录于 state_conditions，导出不自动触发 Probe。寄存器 TCL 只使用 registers 的 endpoint/target，不借用其他 target；隔离中的路线标 available=false。

| support | 含义 |
|---|---|
| unobserved | 存在配置计划，当前尚无读取结果；硬件支持未知 |
| observed_value | 当前上下文有完整响应、请求区间、匹配声明和精确位宽的有效样本；只证明这次缓存读取 |
| unproven_value | 有标为有效的旧/兼容值，但缺少完整来源或位宽、上下文、响应证明 |
| conditions_excluded | 当前有效条件不满足；fact_source 区分配置与当前 Probe，不自动声称硬件不存在 |
| not_implemented / reader_unsupported / access_restricted / feature_disabled | 原读取结果的独立原因；不互相推断 |
| unknown_owner / route_unavailable / write_only | 未知归属、缺少配置路线或全父链不可读；未执行试读 |
| error / stale / unknown | 原始错误、上下文/共享代次已失效或尚未确定；保留完整观察及旧值来源 |

observations 保留最新失败的 provenance 和旧值的 last_value_provenance、相应条件依据。observation_context_current 仅表示这次尝试的上下文仍适用，失败即使为 true 也不是有效值。RUNNING、换会话/停止代次/相关帧、共享 owner epoch 变化使 support 失效；旧 Probe 保留作为历史，但 probe_current=false 且不覆盖配置事实。多核协调器在返回前再次核对共享样本、过滤 Probe 并重新解码事实；无法解析报告则返回错误。Scope All 仍仅导出当前核，不进行广播或借用其他核的私有值。

下表覆盖全部计划读取类别。软件依据来自现有实现和其专项报告；**全部实板读取能力仍未验证**。R52+ 目录不能证明 R52+ 专有差异。planned_classes 根据既有组 ID 统计，core 与 banked 的祖先组可能重叠，数字不能相加当独立寄存器总数；自定义组若不使用约定 ID，完整内容仍在 rows/categories 中，其对应计划类别保持未知。

| 类别 | 计划读通道 / 位宽 | 状态与归属条件 | 已有软件范围 / 剩余缺口及证据 |
|---|---|---|---|
| 通用 Core/CPSR | 命名 GDB / 32 | 选中核、线程和相关栈帧；STOPPED | [框架](register-framework-audit.md)、[读取来源](register-read-provenance.md)；真实目标描述与工具身份 REG-001/002/301 待补 |
| 模式银行 | 独立 banked v2 / 32 | 外部当前 Debug EL、物理 frame 0、core owner | [银行证明](register-banked-proof.md)；当前 EL2 与 User 子集有软件证据，完整 EL1 REG-303 待补 |
| VFP 控制、S/D/Q | 独立 VFP v2 / 32、64、128 | 当前 Debug Hyp、容量/使能/陷阱、新鲜物理 pair、core owner | [VFP 证明](register-vfp-proof.md)、[存储视图](register-storage-views.md)；合法 EL1/Guest/User REG-304 待补 |
| CP15 ID/控制/异常/虚拟化 | 配置 MRC / 32；指定 MRRC 或命名 GDB / 64 | 物理暂停上下文、明确编码；具体访问条件仍须 reader 检查 | [来源](register-read-provenance.md)；通用安全权限与完整类别 REG-302/308 待补 |
| EL1/EL2 MPU/MAIR | CP15 与选择器事务 / 32 | 明确区域数量、当前权限、选择器保存恢复、core owner | [MPU 实现](../src/registers/mpu.rs)；完整 EL1/Guest 与实际映射 REG-306/307 待补 |
| Generic Timer | 独立 Timer v1 / 32、64 | 当前 Debug EL、物理/虚拟视图区分；单项 MRRC，跨项非同时 | [Timer](register-timer.md)；低 EL 权限 REG-401 待补 |
| PMU | 独立 PMU v1 / 32、64 | 当前 Debug EL2、数量、直接只读计数视图、core owner | [PMU](register-pmu.md)；不由配置或失败推断不存在；其他权限边界仍未知 |
| GIC 物理 ICC | 独立 GIC v1 / 32 | 当前 Debug EL2、物理 priority/AP 容量、core owner | [GIC](register-gic.md)；完整权限/接口 REG-405 待补 |
| GIC 虚拟 ICH/ICV | 独立 GIC v1 / 32 | VM 控制与虚拟影子视图区分；不宣称读取正在运行的 Guest | [GIC](register-gic.md)；完整虚拟上下文 REG-405 待补 |
| GICD/GICR MMIO | 指定 GDB byte-range 或 TCL memory / 32、64 | 显式组件基址/owner、核心通道、可选新鲜身份/容量证明；64 位非原子 | [MMIO](register-mmio.md)、[证明](register-mmio-probe.md)；完整模块、板级 AP 映射与 BUS-006 待补 |
| Debug | CP15 或显式组件 MMIO / 32、64 | 核私有外部 Debug 身份/容量、物理地址与配置 owner 分开 | [MMIO](register-mmio.md)、[证明](register-mmio-probe.md)；完整 Debug/低 EL REG-405 待补 |
| STM | 目录、板级存在性、基址/AP 路线及读取策略未适配 / 未确定 | 不从缺少目录推断硬件 No，不盲扫地址 | planned_classes 明确保留零目录、unverified；REG-406 待补；Trace 采集/解码不在本项范围 |

软件验证：七项单元覆盖两目录/全条目、类别/位宽、协议路线优先级、条件 No/Unknown、完整/缺失/错误 receipt、共享 epoch、旧 Probe、MMIO 地址/位宽/字节序和别名父链。实际 CLI 六项离线用例逐字节核对客户配置；worker 集成验证 Scope All 当前 core1、peer 失效、原生 VFP 完整缓存及故障隔离中的零新增 I/O。延后驱动通过实际 EXE/MI/TCP/Tcl 软件夹具的四阶段，并验证独立值不符时失败。

环境用例与命令见 [tests/cases/register-matrix.md](../tests/cases/register-matrix.md)，默认四阶段 skipped；未上板。完整回归证据与本批提交见 [进度](registers-development-status.md)。本项验收不等于其余前置身份、类别 reader、安装或 Release 已完成。
