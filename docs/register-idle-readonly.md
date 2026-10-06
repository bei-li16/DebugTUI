# 只读寄存器的按需读取与空闲资源验收

本说明对应冻结 Goal 的 D04。当前实现复用既有 UI 事件循环、可见行调度、worker 和后端事务；没有增加后台目录扫描、目标轮询或新的定时器。完成状态只维护在 [唯一账本](registers-readonly-goal.md)。

核心参考文档保留绝对路径：

```text
G:\Data\GitFiles\DebugTUI\tui-debugger-spec.md
G:\Data\GitFiles\ARM\File\Arm® Cortex®-R52 Processor Technical Reference Manual.pdf
G:\Data\GitFiles\ARM\File\Armv8-R AArch32.pdf
```

## 空闲检查的修复

`sync_register_absence` 原先在每次 UI 循环中遍历整个目录，并为每项解析 owner。这会反复复制每核路由配置，即使所有组都折叠且没有样本。现在先从已有的未实现样本找出候选 ID，仅为这些候选核对当前目录、实际 owner 和采样上下文。没有候选时不遍历目录或解析每项的路由。

候选不是可用性证明：来自其他核、其他 owner、失效上下文或已从目录移除的样本不能隐藏当前定义。同 ID 的 peer 未实现样本也不能覆盖当前核的有效值。既有生命周期失效规则保持生效，运行态不沿用停止态的缺失判定。

## 真实终端测量

2026-10-06，在 Windows PTY 中运行实际 `debugtui.exe` 的交互模式，寄存器面板可见，动画关闭、Watch 为空、没有启用 Live refresh。GDB 使用 `tests/mock-gdb.cjs` 的本地 MI 管道夹具；没有服务、探针、Tcl/AP 连接或上板执行。每个区间约 15 秒，每秒采样一次，统计 TUI 自身的 CPU、Working Set、Private Bytes 和 MI 命令数。

CPU 是一个逻辑处理器的占用百分比；内存不包含 Node、终端宿主或 OpenOCD。结果是这些短区间的实际观测，不代表长期内存检测或各类终端的性能保证。配置与缓存状态不同的行分别列出，不用它们计算通用加速比例。

| 二进制 | 场景 | CPU % | Working Set MiB | Private MiB | 区间新增 MI |
| --- | --- | --- | --- | --- | --- |
| 修复前 | 无系统目录，旧 GDB 列表可见 | 0.72 | 15.26 | 2.29 | 0 |
| 修复前 | M7，404 项，所有组折叠 | 4.21 | 19.96 | 5.11 | 0 |
| 修复前 | M7，Core 展开且可见值已缓存 | 1.33 | 21.26 | 5.07 | 0 |
| 修复后 | 无系统目录，旧 GDB 列表可见 | 1.03 | 15.56 | 2.22 | 0 |
| 修复后 | M7，404 项，所有组折叠 | 0.92 | 19.93 | 4.75 | 0 |
| 修复后 | M7，Core 展开且可见值已缓存 | 1.54 | 21.25 | 5.09 | 0 |
| 修复后 | R52，1,999 项，所有组折叠 | 0.61 | 43.96 | 27.06 | 0 |

全部区间的 Private Bytes 和 Working Set 增量为零或负值。R52 目录的常驻内存与 M7 不同，不能据此把未访问的定义计为已成功读取。首次连接的 MI 初始化不属于空闲区间；用 Tab 经过 Asm/Files 时，夹具会一次性拒绝不支持的命令，该错误在开始计数前已结束，没有持续重试。

M7 展开 Core 后，仅可见的 R0～R4 产生五次 raw register 读取，随后不重复读；总 MI 数包含每次读取前后的实际线程/帧检查。折叠时不读取 GDB 名称或值。缓存场景的搜索、Status 打开关闭和 R0 格式菜单应用，MI 数保持 31 → 31。元数据、说明和格式操作不会触发额外目标访问；展开使尚未缓存的可见项进入既有按需队列，不能据此宣称首次展开也零读。

本轮修复后 EXE SHA256：

```text
f88783d8f58fb4ab3048c8cb3bd569db196ae91ff836a5e23cb69e9a2822c83e
```

全部七份 JSON、每秒样本、MI 原文、配置和 UI 操作记录保存在：

```text
C:\Users\18283\.codex\build-cache\DebugTUI-registers-readonly\evidence\register-idle-20261006
```

修复前报告记录各自二进制摘要；修复后的四份报告还记录配置摘要。所有报告均为软件场景，不作为 ARM 指令执行或硬件能力证据。

## 复测方法

使用本地 Node MI 夹具配置启动交互 TUI，关闭动画、Watch 和 Live refresh，设置独立 `DEBUGTUI_CONFIG_DIR`。无目录基线、M7 和 R52 使用相同 GDB 名称和值、连接模式、窗口和读取配置；目录场景使用已保存的 `open=[]` 偏好。不要使用 `--headless`、`--demo` 或真实探针。

进入 System Regs，等待连接和界面请求结束，再在另一个 PowerShell 中测量已经运行的进程：

```powershell
.\scripts\measure-register-idle.ps1 `
  -ProcessId <交互DebugTUI进程ID> `
  -Transcript '<本地夹具的MI原文路径>' `
  -Configuration '<实际debug.toml路径>' `
  -Output '<报告JSON路径>' `
  -Scenario 'cortex-m7-visible-collapsed' `
  -Seconds 15
```

测量脚本只观察已运行的 TUI、配置和原文，不启动调试器或发送输入。进程退出、配置改变、原文截断/修改或出现任何新增 MI 命令都会失败；报告保存每秒资源样本和二进制/配置摘要。该脚本及本次报告用于上述离线软件夹具，不能用于宣称真实目标未被其他客户端访问。

再次展开 Core、等待首次读取结束，测量缓存场景；在已缓存的值上验证搜索、Status 和格式操作。最后 Ctrl+Q 正常退出，确认夹具子进程结束。本轮全部测量进程和 Node 夹具均已退出。

## 软件与只读边界

本轮 `ui::registers` 56 项、`register_access` 15 项、`register_lifecycle` 4 项、`register_shared` 6 项，共 81 项通过；严格 Clippy、fmt 和 PowerShell 语法检查通过。新增回归检查 peer 未实现、当前有效/缺失样本、目录外 ID 及运行状态切换。既有 UI 测试继续覆盖可见行/单次调度、副作用门禁、纯展示操作、缓存和 owner 失效。

本轮未修改 worker 访问策略、M/R 目录、OpenOCD 事务、补丁或候选工具。D01～D03、B/C 组仍适用的证据沿用。所有测量原文无目标数据写入、模块使能、CP15 指令或 monitor 配置命令；必要 scratch/selector 的保存恢复和故障隔离沿用已验证且未改动的生产事务证据。

[M 多核](../tests/cases/register-cortex-m-multicore.md)、[运行态](../tests/cases/register-running.md)、[M MPU](../tests/cases/register-cortex-m-mpu.md)、[M7 cache](../tests/cases/register-cortex-m7-cache.md) 和 [R52 selector](../tests/cases/register-r52-selector-read.md) 延后 case 保持 SKIPPED。没有执行上板或升级硬件 verified。本轮所选软件测试不能代替 E03 的最终完整回归；E01～E05 尚待最终用例/文档、安装升级和非主分支 Release 交付。
