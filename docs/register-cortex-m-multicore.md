# M7＋M4 的 CorePrivate 隔离

同一 PPB 地址由每核自己的绑定访问。`cores.registers` 选择该核 CPU/目录和 `ppb` owner；`component_owners.ppb["core:<name>"]` 指定 channel，该 channel 明确声明 cores、target 和 Tcl endpoint。不同核可以使用同一 OpenOCD 服务的不同 target，也可以使用不同服务；同 endpoint/target 不可同时冒充两个私有核。GDB memory 使用各核独立且已确认的 worker endpoint。

每核加载公共目录加型号增量。M7 的 cache 定义和已观测容量不会因为相同地址、Scope All、同一服务或切核而成为 M4 的能力。Scope All 是运行控制范围，系统寄存器、Probe、MPU/cache 数据请求仍只属于选中核。

普通样本、MPU/cache 视图和能力证明绑定 core owner、会话、停止代次、目录和实际访问路线。只切核可查询该核仍适用的缓存，查询不产生 PPB I/O。帧、运行或身份变化会使相关旧值 stale，旧值保留原时间/上下文/来源；不会继承另一核的有效值或容量。重连按既有多核连接流程更换各 worker 的会话，旧 context 在数据请求前拒绝。

MPU/cache 访问使用既有受保护事务。MPU 仅临时选择 RNR 并恢复/回读，cache 仅临时选择 CSSELR 并恢复/回读；不写 CTRL、cache 配置或维护寄存器。已确认恢复的失败撤销本核 Probe，必须重新证明本核后再读 indexed 值。取消等待受保护事务完成恢复后丢弃新结果，原缓存保持原归属；独立服务的 peer 不借用失败核的错误或证明。恢复不确定的共享服务隔离规则继续生效，不能把实际共享后端的故障当作独立服务处理。

软件验收直接使用内置 M7/M4 目录、Coordinator、独立 MI worker、真实 TCP 和生产 Tcl 控制流程。共同地址 `0xe000ed00` 返回两种独立 CPUID；MPU 容量分别 16/8，RNR 初值分别 3/5，区域数据和 target 各自不同。另验收 M7 cache、M4 拒绝、foreign context 零 I/O、单核错误/取消、帧/运行/证明边界/重连和新身份撤销。UI 来回切换目录后拒绝另一核同地址的迟到样本。此处的目标模型提供字节和故障，不模拟实际 ARM 执行，也不证明实板 AP 映射。

延后 case 和默认零连接驱动见 [M7/M4 环境用例](../tests/cases/register-cortex-m-multicore.md)。AP 是 case 中的声明；驱动通过实际成功样本核对 endpoint/target，并要求独立型号/容量/region 基线。软件输出不证明实际 AP 物理归属，不升级 hardware verified。

核心参考文档（绝对路径保留）：

```text
G:\Data\GitFiles\DebugTUI\tui-debugger-spec.md
G:\Data\GitFiles\ARM\File\Arm® Cortex®-R52 Processor Technical Reference Manual.pdf
G:\Data\GitFiles\ARM\File\Armv8-R AArch32.pdf
```
