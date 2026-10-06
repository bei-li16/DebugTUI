# 系统寄存器的运行态读取

B11 复用既有 AP/MMIO 内存读取通道和前后上下文检查，接通 `registers_read`、Registers 自动按需队列和手动 Read；没有新增轮询器、GDB 后端或运行控制路径。软件验证覆盖 M3/M4/M7 与自定义普通 MMIO；实际上板仍未执行。

## 何时可读

同时满足寄存器及全部 Alias 父项允许运行态、存在性/权限/使能条件已满足、owner 确定、实际显式内存通道对当前核可用且 `while_running=true` 才尝试读取。元数据和通道声明各自都不能独立授权另一项；响应失败不更换路线。

CorePrivate 使用已有核独占路由校验。GDB memory fallback、GDB regfile、CP15/MRRC/Banked/VFP/backend 等执行路线在运行态返回 NeedHalt，零数据读取。要求 stopped identity/control proof 的 MMIO/STM 仍 NeedHalt。M7 CCSIDR 的受保护 CSSELR 事务，以及 MPU/cache bank 和 Probe 入口保持停止要求。

示例（按实际工程修改 endpoint、target 与核名，先确认该 AP 在运行态允许访问）：

```toml
[registers]
cpu = "cortex-m4"

[registers.targets]
default = "cpu0"

[registers.component_owners.ppb."core:default"]
base = 0
channel = "ppb0"
little_endian = true

[[memory_access]]
id = "ppb0"
label = "Core PPB"
tcl_endpoint = "localhost:6666"
target = "cpu0"
cores = ["default"]
while_running = true
```

每核拥有自己的 channel/target binding；同 PPB 地址在别核是另一个对象。`while_running` 是显式通道能力声明，不是实板验证标记；真实工具/AP/目标拒绝时显示失败，保留原来源，不自动 halt 或回退。

DHCSR、SysTick CTRL 继续只允许明确手动单次读，自动队列和 Probe 不读取它们。WO 维护定义不能读取或执行。未实现、未知存在性或使能不足分别处理，不自动使能 DWT/FPU/cache。

运行态 raw 不升级停止态 Probe、模块容量或 selector 的证明；仍依赖这些证明的可选项会显示 Unknown/受限，需要暂停后重新 Probe。本批接通已符合全部条件的实际路由，不沿用暂停前的旧证明授权新读。

## 样本和按需刷新

- 运行态成功值标记 `view=running_memory`，仅在当前运行态及 owner/session/generation 有效。完成的实际 `tcl_memory` 路线和响应区间与样本 context 一致，缺失/不完整来源不能作为当前运行值。
- 停止态旧值进入运行态时 stale；运行态旧值暂停后 stale。保留旧 raw 的原始 provenance；失败响应表示本次失败，状态快照与 UI 另保留 last-known raw 和来源，不能误读为本次成功。
- AP 采样前后消费已经到达的 MI 通知，不发送运行态 frame/thread 查询。停止/再运行、线程选择、关闭连接等事件改变读取边界，迟到响应不通过；同批后续读停止，取消不发布新值。
- Registers 仅读取当前可见安全项；进入运行态清除上一状态的尝试记录，允许第一次新读。同一状态/上下文已有尝试不反复自动提交；手动 Read 可再次取样。纯打开说明、字段、搜索和格式不引入轮询。
- 显示的是请求期间取得的值，不能代表一个同步多寄存器硬件快照。64 位普通 MMIO 是两个 32 位传输值，记录 `atomic=false`；Alias 共用其父值及同一次请求来源。
- 异构多核 B10 和生命周期 D03 已分别完成软件复验，证据见 [唯一账本](registers-readonly-goal.md)；对应硬件 case 仍为 SKIPPED，B11 的证据不代替实板验证。

## 软件与延后环境验证

软件用例执行生产 worker、MI、TCP 与实际 Tcl，包括 M3/M4/M7 原始值、GDB/停止/错核路线拒绝、64 位/Alias、手动副作用与 WO、错误和旧值、取消、采样期间停止/再运行/线程选择/断线。UI 用实际共享调度入口验证安全可见项、每上下文一次、状态切换及窄/宽来源展示；实际 EXE 驱动核对独立值，故意错误基线必须失败。

[硬件 case](../tests/cases/register-running.md) 与 `scripts/test-register-running-hardware.cjs` 准备 H01～H04 的可执行检查；默认模式不连接，四项 SKIPPED。H05～H07 提供手动/故障注入步骤。所有本 Goal 软件报告 `board_tests_executed=false`，不修改硬件 verified。

核心参考绝对路径：

```text
G:\Data\GitFiles\DebugTUI\tui-debugger-spec.md
G:\Data\GitFiles\ARM\File\Arm® Cortex®-R52 Processor Technical Reference Manual.pdf
G:\Data\GitFiles\ARM\File\Armv8-R AArch32.pdf
```
