# 多核、多个 CTI 与多个 AP

这是板级配置示例，不能直接用于 STM32F429。TUI 不推断 AP 编号、CTI 基地址或触发通道，必须按芯片手册和探针服务器配置替换。当前每核使用同一 ELF/GDB/SVD，异构符号文件尚不支持。

## OpenOCD 层

先使用板卡脚本建立 DAP、每核 CPU target 和 GDB 端口。之后可配置独立总线 target，例如以下变量全部由板卡脚本定义：

```tcl
# BOARD_DAP / BOARD_BUS_AP: this board's actual DAP and MEM-AP.
target create soc.ahb mem_ap -dap $BOARD_DAP -ap-num $BOARD_BUS_AP -endian little
soc.ahb configure -gdb-port disabled

# CTI APs/base addresses/channels come from the SoC integration manual.
cti create cti0 -dap $BOARD_DAP -ap-num $BOARD_CTI0_AP -baseaddr $BOARD_CTI0_BASE
cti create cti1 -dap $BOARD_DAP -ap-num $BOARD_CTI1_AP -baseaddr $BOARD_CTI1_BASE
# Configure CTI input/output event mappings here for the actual SoC.
# Some target drivers also require an explicit -cti association.
```

`mem_ap` 不依赖核心暂停。CPU target 的内存操作是否使用借核访问、是否要求暂停，由该 target 驱动和板级配置决定；TUI 不把 CPU target 的名字等同于某个 AP。ADIv5 中 `-ap-num` 是 AP 索引，ADIv6 的含义不同，按实际 OpenOCD 文档配置。

## tools/debug-env.toml

下面片段与项目现有 `[gdb] / [target] / [service]` 配置合并。确保 TCL 端口仅在本机监听。

```toml
[[memory_access]]
id = "core0-memory"
label = "Core 0 memory"
tcl_endpoint = "127.0.0.1:6666"
target = "soc.core0"
while_running = false
cores = ["core0"]

[[memory_access]]
id = "core1-memory"
label = "Core 1 memory"
tcl_endpoint = "127.0.0.1:6666"
target = "soc.core1"
while_running = false
cores = ["core1"]

[[memory_access]]
id = "ahb"
label = "Shared AHB memory"
tcl_endpoint = "127.0.0.1:6666"
target = "soc.ahb"
while_running = true

[sync]
method = "cti"
tcl_endpoint = "127.0.0.1:6666"
# Assumes the CTIs and event/channel mappings above already exist.
# Enable/ungate only the channels your board requires.
open = ["cti0 enable on", "cti1 enable on"]
```

多个通道可以使用不同 `tcl_endpoint`，对应不同 DAP/服务器。核心限制是当前观察项归属的调试核心；共享通道省略 `cores`。所有通道以空闲时默认关闭轮询开始，用户在右键菜单按项启用。工程中的 `[[memory_access]]` 会整体覆盖环境列表。

项目的 `debug.toml` 选择该环境并配置核心：

```toml
[tools]
profile = "tools/debug-env.toml"

[[cores]]
name = "core0"
endpoint = "127.0.0.1:3333"
startup_order = 0

[[cores]]
name = "core1"
endpoint = "127.0.0.1:3334"
startup_order = 1
```

连接期间先启动共享服务，再按顺序执行全部 `sync.open`，成功后才连接各核。配置错误会明确失败并清理自有服务；切换当前核心只改变调试 UI 和命令路由，不发送 CTI 触发，不改变其他核的运行状态。实际同步暂停/运行取决于芯片的 CTI 硬件连接和配置，不等同于软件按顺序发送 GDB 命令。

官方语义参考：[OpenOCD target configuration](https://openocd.org/doc/html/CPU-Configuration.html)、[CTI commands](https://openocd.org/doc/html/Architecture-and-Core-Commands.html#ARM-Cross_002dTrigger-Interface)。本地没有多核板卡，多 CTI/AP 的路由和错误处理测试使用模拟 TCL 服务；单核 F429 AP0 AHB 已实板验证。
