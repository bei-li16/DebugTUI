# STM 只读配置与状态

REG-406 接入 Arm STM v1.1 的身份/功能寄存器以及 STM-500 配置、状态。STM 是芯片集成的 CoreSight 组件；R52 MIDR、CPU 数量和选中的目录都不能证明板上存在 STM。地址及芯片归属必须来自板级资料，当前没有自动扫描或 ROM table 发现。

实现依据为 [Arm STM v1.1 Architecture Specification，IHI0054B](https://documentation-service.arm.com/static/5f8ffb70f86e16515cdbfee8) 和 [STM-500 Technical Reference Manual，DDI0528B](https://documentation-service.arm.com/static/5f106ed80daa596235e81fcc)。使用其控制空间、身份和功能定义；芯片的 AP 编号、实例位置、供电及认证需要另行确认。用户提供的 R52 手册用于 CPU 功能，不能代替芯片 STM 集成信息。

两个 AArch32 内置目录各新增 46 项，均为 32 位、`scope="chip"`，不提供 writer。目录区分 STM 核心、可选硬件事件控制和可选 DMA 控制；`WO` 条目用于解释不可观测的命令，读取前拒绝。不访问 stimulus 空间，不开启 Trace，不解锁，不修改选择器，也不采集或解码 Trace 数据。

项目中明确配置如下，地址只是示例，应替换后再使用：

```toml
[registers]
cpu = "cortex-r52"

[registers.topology]
chip = "board"

[registers.component_owners.stm."chip:board"]
base = 0x51000000
channel = "stm_ap"
little_endian = true

[[memory_access]]
id = "stm_ap"
target = "soc.stm_ap"
tcl_endpoint = "localhost:6666"
```

`stm` 映射必须是明确的 4 KiB 对齐控制区，不能填写 stimulus 基址。空 `channel` 使用选中核心的 GDB memory；非空名称选择已经配置的内存通道。示例 AP target 必须来自实际工具环境。缺少芯片 owner 时，不使用无归属 `components.stm` 回退。

只有板级资料确认可选接口时，才另外添加 `component_owners.stm_hwe."chip:board"` 或 `stm_dma`。它们指向与 `stm` 相同的控制基址、channel 和字节序；不配置时不探测其地址。可选类未知/不适配只让该接口保持 Unknown，不推断硬件不存在。

在停止的物理 frame 0 上手工执行 Probe caps（RPC `registers_probe`）。核心身份/功能使用前后各十五个独立 32 位请求，验证 CIDR、DEVARCH `0x47710a63`、DEVTYPE `0x63`、Arm PIDR、控制区尺寸及端口容量。可选硬件事件控制有前后二项，DMA 有前后一项，处于核心身份请求之间。值、地址、实际 endpoint/target/channel、字节序、上下文、owner、共享代次和请求次序都必须匹配。

读取 STM 数据总是要求当前组件证明，包括手工读取和自定义目录；`registers.mmio_probe=false` 也不能绕过。核心配置布局只适配 PIDR part `0x963` 的 STM-500，其他识别出的 Arm STM v1.1 part 可显示共同的身份/功能区。可选字段使用真实功能值过滤；未定义或保留的 SPTER/TRIGCTL 编码保持 Unknown，不能当作 No 或 Yes。没有 CPU 身份时仍可独立取得 STM 证明。

硬件事件 `NUMHE<=32` 时不读 HEBSR；未实现 trigger/mux、端口 enable/trigger/override、同步与时间戳频率寄存器也不会被自定义目录强行试读。端口和硬件事件 mask 显示当前 bank 原始值，不表示所有端口/事件的完整快照。不同寄存器、不同核心的采样没有同时性保证。

切核后的 worker 使用自己的 GDB/AP 路线，不因 Scope All 广播读取。其他核心运行或发生共享生命周期变化时，芯片 Probe 失效；协调器在响应和矩阵导出时再次过滤共享证据。能力矩阵包含这 46 项，但 `hardware_support` 始终为 `unverified`，配置和主机夹具不能证明实板权限。

软件验证覆盖独立偏移/值、身份与容量错误、保留编码、混合 owner/代次/请求、GDB 与 AP 大端路线、四 worker、WO 无访问以及 peer 在采样中或之后运行。新增延后驱动由实际 EXE/MI 软件夹具执行，并拒绝未就绪、身份错误和独立基线不符。八项[环境 case](../tests/cases/register-stm.md)保持 SKIPPED。

首轮单元夹具误用了不存在的 `Phase::Submitted`，编译失败记录保留在 `artifacts/register-stm-unit-first.log`；改用真实 `Started` 验证未完成请求拒绝，未移除原断言。

首轮完整回归发现既有矩阵共享测试的自动停止竞争：mock在continue后10 ms停机，独立stack刷新越过了日志检查点。仅此夹具保持对端RUNNING，并新增明确RUNNING断言；原整份MI日志不变断言保留。专项复核通过；失败完整报告 `C:/Users/18283/.codex/artifacts/DebugTUI-stm-20261006-01a0e2e6/functional-1791244302757-a6d1ea43` 和首轮控制台保留，后续重跑完整回归。

完整Cargo **406单元＋175集成通过，2 ignored，共581通过**；F24 **156/156**为证据匹配模式数，非用例数。525324 ms无超时；其他**22外层功能套件未选择**。完整报告/Markdown/unit.log逐字节镜像 `artifacts/functional-1791244770256-6c55a69d/`，61份原始子报告保留于总报告引用位置；Node大产物留在C盘，核对 `artifacts/register-stm-report-mirror.json`。严格Clippy通过，日志 `artifacts/register-stm-clippy.log`；目录再生成及后端11项源锁静态核对通过，见 `artifacts/register-stm-static-audit.json`。

最终整体目标仍 active；实板 owner/AP 集成与其他 TODO、升版和 Release 尚未完成。

文档链接自检：本轮新增/变更的相对链接均有效。`TESTING.md`历史记录已有49个归档产物链接在原本地位置不可见，本轮保留历史文本并登记 `artifacts/register-stm-static-audit.json`；未宣称这些旧证据已复原，完整历史报告交付仍随REG-506跟踪。
