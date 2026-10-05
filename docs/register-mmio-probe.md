# R52 新鲜 MMIO 组件身份与容量

显式 registers.mmio_probe=true 在 registers_probe 中启用三组件的只读物理证明，并要求后续 MMIO 数据读取具有同一当前停止上下文的有效证明。默认 false 保持旧配置；component_owners 必须明确，缺失映射零 I/O，不从逻辑核心标签、MPIDR 或默认基址推导地址。

先读取 Debug 外部 MIDR，拒绝非 Arm/D13/AArch32 或与当前 CPU MIDR 不符的值，再逐项核对 CoreSight CIDR、EDDEVAFF 与 EDDFR。GICD/GICR 各从 IIDR 和 classF CIDR 开始，身份不符即停止该组件的后续地址。IIDR revision/variant 必须匹配外部 MIDR；GICR TYPER Aff0/ProcessorNumber 必须匹配实际 Debug Aff0。GICD_TYPER 固定字段/ITLinesNumber 和 EDDFR 精确 R52 字段才授予容量，未知布局/读错误保持 Unknown。

在原15项CPU Probe之外，成功路径追加42个MMIO请求：Debug 九项前读、GICD 六项前读/后读、GICR 六项前读/后读、Debug 九项后读。原服务发生不确定故障时不追加MMIO请求。保存两个独立样本及实际 route、aperture、owner、STOPPED frame0 context、读宽/字序和主机时间区间；每组完全先读后复读，GIC 组位于 Debug 复核包围区间。64 位 TYPER 是两字非原子，前后相同不证明期间未变化或多个条目同时采样。这42个MMIO请求不执行unlock、控制写、IAR acknowledge、DTR/PC/sticky 状态读取或指令注入；既有CPU专用后端的事务规则不变。

依据 R52 TRM 100026_0104_01_en 表10-7、10-35/36、12-5/40/41：GICD 固定字段0x02480000，ITLinesNumber=1..30；IIDR 基值0x0100043b加 MIDR variant/revision；Debug CIDR 为0d/90/05/b1，GIC为0d/f0/05/b1；EDDFR D28高字=0，D2C低字除保留UNKNOWN低四位外为0x10707100，得到8断点/8观察点/2上下文比较器。不使用有矛盾的 EDPFR 两字推断。

事实 source=mmio:component；已声明值保存在配置及条件 configured_value，当前有效观察覆盖同名配置事实。失败不会推断 physical No，也不删除配置；启用严格校验后，没有有效物理证明的数据请求以 ReaderUnsupported 拒绝，即使配置 present=1。容量已证明不足则 HardwareNotImplemented，自动和手工都不发送。板级 base/owner 关联和供电认证权限仍需独立配置，不把读到相同产品 ID 当全局唯一核心身份。

共享 cluster 的 Probe 在 coordinator 响应上绑定 owner epoch，在请求期间或发布后任意相关 peer 生命周期变化时使样本及事实失效，并以 FIFO 通知 worker。私有 core 证明仍只适用于本核当前 context。Probe 样本使用 mmio_probe.ID 和 mmio_probe.ID.after，普通数据读取不会覆盖它们；条件详情保存共享 owner/epoch 的完整观察。

验证记录及新六项延后环境 case见 [register-mmio-probe.md](../tests/cases/register-mmio-probe.md)。失败日志保留。REG-405/BUS-006 的完整 Debug/低 EL/其他范围仍待完成，TODO 保持24完成/47未完成；未上板、未升版安装或发布最终 Release。

## 本轮最终验证

完整 Cargo **383 单元＋158 集成通过，2 ignored，共541通过**；**F24 134/134**为证据模式数，非用例数。550889 ms，无超时；其余**22外层功能套件未选择**。报告 artifacts/functional-1791228389704-41ff400d/report.json，Cargo为同目录 unit.log；严格 Clippy通过，日志 artifacts/register-mmio-probe-clippy.log。最终驱动新增输入完整性校验后，缺失容量期望在目标I/O前拒绝，实际EXE正向五阶段再次通过，见 artifacts/register-mmio-probe-driver-guards.json。

新四项单元和五项worker/EXE/AP集成通过；默认驱动五阶段SKIPPED、六项环境case均延后。固件离线21地址/22次32位读取、一次MIDR MRC与前置CPSR检查通过，对象未执行，见 artifacts/register-mmio-probe-firmware-report.json。首次脚本仅匹配rN寄存器而拒绝编译器合法ip选择，修正离线正则后通过；未改变固件指令或判定上板成功。既有后端十一项源锁、补丁、Windows/Linux候选与对应源码包一致，日志 artifacts/register-mmio-probe-inherited-package-audit.log。G:空间归档只迁移已结束的14/0/0分发夹具到F:，原报告/日志恢复并校验散列，映射 artifacts/register-mmio-probe-space-archive.json 及 register-mmio-probe-final-space-archive.json。
