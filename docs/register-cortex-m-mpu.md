# Cortex-M MPU bank 的只读采样

对应冻结清单 B05。M3/M4/M7 沿用内置 CMSIS 定义和已有 CorePrivate 路由。TYPE 的合法观测决定 0/8 个 region，M7 另支持实际观测到的 16 个；未知、配置声明或错误响应不能授权 selector 操作。硬件支持仍未验证。

`:mpu` / `:mpu m` 打开 M 总览，打开、滚动、字段解释和 bank 按钮不读取目标。物理 frame 0 暂停后先 Probe，再按 Read。总览展示每个 region 的 RBAR/RASR 原始值及目录字段，包括 ENABLE、SIZE、SRD、AP、XN、TEX/S/C/B；不把配置解释当作给定地址的有效权限证明。R52 的 EL1/EL2 总览保持既有直接读取路径。

Headless 使用 `registers_mpu {context, bank:"m", read:true}`，返回 `view`；省略 read 时只返回缓存。Snapshot 的 `register_mpu` 保留此核 bank 的完整结果；每个 index 下复用 Sample、Reader、owner、Context、请求时间及 Provenance。indexed 样本不覆盖普通 `mpu.rbar`/`mpu.rasr` 的当前 RNR 样本。

## 访问和恢复

整批持有现有服务 lease。前后检查实际 GDB 线程、物理 frame、停止上下文；一个固定的 Tcl 事务通过明确 `<target>` 调用完成：

1. 检查 curstate，重新读 CPUID/TYPE，与本核已观测型号及容量相符。
2. 读取 CTRL，保存并严格验证原 RNR。无 MPU 时仅核对 CPUID/TYPE，不读取 CTRL/RNR/RBAR/RASR，返回空 region；没有制造控制或 selector 的零值样本。
3. 对每个有效 index，仅写 RNR，再回读确认，读取 RBAR/RASR。
4. 无论选择/读取是否失败，都恢复原 RNR 并回读验证；再次核对 curstate、CPUID/TYPE。
5. 完整响应与最终上下文均通过后才发布整个 bank。

只临时写 RNR。不写 MPU_CTRL、RBAR、RASR、enable 位或 CPU 状态，不使用全局 targets 切换。每次选择后的同步内存回读确认该访问的完成；不额外注入 CPU 指令。这里证明的是暂停状态的顺序配置采样，没有宣称硬件原子快照。

region bank 事务必须配置独占该核的 Tcl/AP 内存通道。GDB CorePrivate memory 仍支持普通当前 RNR 寄存器读取；缺少经过验证的 selector 事务路线时，bank 动作在目标 I/O 前拒绝，不拼接独立 GDB 写/读来冒充原子事务。通道须以真实板级配置独立确认 AP/target 映射，软件的配置一致性校验不等于硬件 AP 身份证明。

示例（名称、端口和 AP/target 映射必须替换为实际配置）：

```toml
[[memory_access]]
id = "m_ppb"
label = "M core private PPB"
tcl_endpoint = "127.0.0.1:6666"
target = "chip.cpu0"
cores = ["default"]
while_running = false

[registers.component_owners.ppb."core:default"]
base = 0
channel = "m_ppb"
little_endian = true
```

多核在每个核的 `cores.registers` 中选 CPU/目录/binding，分别绑定自己的通道和 target；同一 Tcl endpoint 可服务两个不同 target，但不可把另一个核分配给相同 endpoint/target。Scope All 不广播 MPU bank 动作。

已知恢复后的普通读错不发布部分结果，保留上次完整 bank 的值、采样时间和来源并标 stale，要求重新 Probe。取消在事务结束后处理。恢复失败、响应缺失/损坏或传输不确定使通道/服务 FAULT；不自动重试或换路线。运行、换帧、重连等现有上下文失效边界使旧 bank 过期。

## 软件与延后环境验证

模型测试检查容量和完整响应解析；UI 检查显式请求、窄屏、字段解释和 stale。`tests/selector_access/m_profile_mpu_cases.rs` 的 worker/Coordinator/实际 EXE 使用真实 Tcl 8.6 控制流，独立检查值、实际写地址、RNR 恢复、多个失败阶段、取消、断开/坏响应、上下文改变、异常定义零 I/O，以及 M4/M7 非连续 core0/core2 的不同 target/cache。

硬件 case 见 `tests/cases/register-cortex-m-mpu.md`。可执行驱动为 `scripts/test-m-profile-mpu-hardware.cjs`；默认全部 SKIPPED、零目标 I/O。软件模型仅证明生产主机状态机、协议和路由，不证明 AP 集成、硅片权限或板级行为。定义来源延用固定 CMSIS 源文件/摘要及其目录 provenance；[Arm CMSIS MPU 文档](https://arm-software.github.io/CMSIS_6/latest/Core/group__mpu__functions.html) 说明 RNR、RBAR/RASR 和 MPU 控制字段。

核心参考文档：

```text
G:\Data\GitFiles\DebugTUI\tui-debugger-spec.md
G:\Data\GitFiles\ARM\File\Arm® Cortex®-R52 Processor Technical Reference Manual.pdf
G:\Data\GitFiles\ARM\File\Armv8-R AArch32.pdf
```
