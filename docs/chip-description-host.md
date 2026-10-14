# 通用调试 Host 与可安装芯片描述

本文描述 1.2.0 提供的调试 Host 与芯片描述适配。DebugTUI 通过 GDB/MI 控制调试会话；芯片配置、系统寄存器目录和外设 SVD 可以独立提供。安装描述文件不需要重新编译 DebugTUI。

## 没有芯片描述时调试

在 Setup 的 **Chip** 选择器按 **G / Generic GDB**，或在 **Tools / profile → F2** 选择 **Generic GDB / existing server**。CPU registers 使用 **GDB target description**，不需要登记芯片或安装系统寄存器 TOML。

```toml
version = 3
[tools]
profile = "builtin:generic-gdb"
[target]
endpoint = "127.0.0.1:3333"
[program]
elf = "Debug/firmware.elf"
```

先用适合实际板卡的配置启动 OpenOCD 或其他 GDB server，再连接上述 endpoint。通用入口不会自动启动 OpenOCD。寄存器列表与数值来自 GDB 的 `-data-list-register-names` / `-data-list-register-values`，通常包含 PC、SP、状态寄存器和通用寄存器；实际名称和可读范围由服务器决定，不假设一定存在名为 xPSR 的寄存器。

安装包内的 GDB 支持 ARM。其他架构需要在工程或自定义工具 profile 中配置匹配的 `[gdb].executable`、参数及服务器。自定义工具 profile 可以声明根字段 `backend = "generic-gdb"`，允许未知芯片标签和显式核心 ID，不查芯片索引、不注入芯片 CPU 默认值。旧 `backend = "generic"` 继续保持原有索引关联规则。

从已有工程切换时，Setup 清空根级及每核 CPU/catalogue 选择，保留 ELF、工程任务、核心 ID、用户显式端口及访问通道。工具和芯片文件继承的 OpenOCD 启动配置不会进入通用 profile。用户显式写在工程里的 service、actions、SVD 等配置仍按原规则生效，连接新硬件前应检查这些工程设置。再次在 Chip 中选择已安装芯片会恢复 ARM/OpenOCD profile 和自动寄存器选择。

## 安装芯片描述

默认用户目录是 `%LOCALAPPDATA%/debugtui`，也可通过 `DEBUGTUI_CONFIG_DIR` 指定。`--init-profiles` 创建下列扩展目录，保留已有文件：

```text
profiles/
  devices.toml              兼容旧版的用户索引，新增文件不要求修改它
  chips/
    stm32f103.toml          芯片元数据、调试后端、访问通道及默认目录引用
  registers/
    stm32f103.toml          系统寄存器定义，可继承共用内核目录
```

Setup 每次打开 Chip 选择器都会发现 `profiles/chips/*.toml` 中的 `[chip]` 元数据。内置 `tools/devices/*.toml` 同样支持发现。芯片 ID 来自文件名；安装后重新打开选择器即可。无需维护第二份索引或重编译 EXE。用户目录中无法解析的文件不加入发现列表，避免阻断其他芯片；工程显式选用该文件或芯片时会报告错误。

下面是新增描述的结构示例，并非经过上板验证的 F103 板级配置：

```toml
[chip]
cores = [0]
backend = "stm32f1"
cpu = "cortex-m3"
register_catalogue = "../registers/stm32f103.toml"

[core_targets."0"]
endpoint = "127.0.0.1:3333"
ready = "Listening on port 3333 for gdb connections"

# 使用 builtin:arm-openocd 时，还需要按实际板卡提供 [service].args，
# 其中引用 ${probe_config}、合适的 OpenOCD target/board cfg 等。
# 按需提供 [memory_access]、[registers.component_owners]、[program].svd。
```

`[chip].cores` 和 `backend` 必填，`cpu` 与 `register_catalogue` 可省略。旧文件的根级 backend 及 extends 继续兼容；如果同时声明根级 backend，必须和元数据一致。新芯片必须在入口文件声明元数据才能自动发现；父文件可以承载共用调试配置与默认寄存器路径。

寄存器文件可以从通用 M3 定义开始，再按芯片资料补齐或覆写实际寄存器、字段、读写规则和支持条件：

```toml
version = 1
cpu = "cortex-m3"
architecture = "armv7-m"
extends = ["cortex-m3"]
description = "基础 M3 定义；芯片差异、板级访问路径和证据仍需补充"
```

只有上述继承不等于完成 F103 专用适配。SCB/NVIC 等使用现有 core-private reader 的资源需要正确的、按核心绑定的 PPB 通道；芯片级 MMIO 资源需要匹配 owner 的基址和通道。文件只能选择软件已经实现的 reader/writer 协议；全新访问协议仍需要开发后端。硬件容量与可选功能仍依赖当前目标证据，不能用配置元数据代替。

## F429 默认配置与优先级

`tools/devices/stm32f429.toml` 默认引用 `builtin:registers/stm32f429.toml`。这个引用从运行中 DebugTUI 安装目录的 `profiles/registers/` 读取，工程使用复制到 `.vscode` 的工具目录时仍能找到安装的寄存器文件。该目录最终解析为 617 个寄存器定义，包含 SCB、NVIC、SysTick、MPU、FPU、调试/跟踪资源和 DBGMCU；定义数量不等于全部寄存器在任意硬件状态下可读。

F429 芯片文件同时提供 `core:core.0` 的 PPB 映射（base 0，地址由 core-private reader 提供）和 `chip:stm32f429` 的 DBGMCU 映射（base `0xE0042000`），都使用 stopped-only `core-tcl` 通道。自定义后端、更名后的通道或用户工程覆盖仍需提供实际可用的对应关系。

- 选择芯片文件的优先级：工程 `tools.chip_profile` → 用户 `profiles/chips/<chip>.toml` → 内置文件。新入口文件中的 `[chip]` 决定其核心与 backend。
- 显式工程或 profile 的 `[registers].cpu/catalogue`，包括空字符串，优先于 `[chip].register_catalogue` 默认值。既有每核覆盖规则不变。CPU registers → **Automatic** 移除工程选择，恢复默认；**GDB target description only** 显式禁用目录。
- `[chip].register_catalogue` 普通路径相对于声明它的芯片文件，继承不会改变路径基准。`builtin:registers/<file>` 相对于应用安装目录的 profiles；不允许通过 builtin 引用越出该目录。
- 可选默认寄存器文件未安装时，使用 GDB 动态寄存器，并在 Register configuration 中注明缺失路径。已存在但损坏的文件、缺失的继承依赖，以及用户显式填写的缺失 catalogue 都会报告错误。
- 未声明 register_catalogue 时仍可用 cpu 名选择通用内核 preset；两者都没有时使用 GDB 动态列表。旧索引 CPU 关联保留兼容。

## 离线验证

运行 `scripts/test-host-descriptions.cjs --binary <EXE>` 验证 F429 自动加载、显式覆盖、未知芯片通用模式、文件安装/移除、缺失默认目录提示，以及通过严格 MI 夹具实际读取 PC/SP/xPSR 的过程。该测试不会连接真实芯片，也不构成 F103 的上板证据。
