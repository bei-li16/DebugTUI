# DebugTUI

## 是什么

DebugTUI 是基于 GDB/MI 的终端调试工作台，可在 PowerShell、Windows Terminal 或 VS Code 终端中使用。支持源码断点、单步、Watch、寄存器与 SVD 外设查看、单核/多核调试，以及调用工程的 Build / Download 命令。

当前版本 **1.1.2**，支持 **Windows x64**。npm/完整 ZIP 内置 ARM GDB、OpenOCD、STM32F429 与 THA6104/6206/6412 芯片配置及 SVD；调试时无需 Node、Python 或 VS Code 常驻。工程保存一份 `debug.toml`，通过 Probe、Chip 和 Debug cores 选择目标。更新与升级方法见 [1.1.2 发布说明](docs/release-1.1.2.md)，THA 使用方法见 [内置工具说明](tools/README.md)，M/R 核的系统寄存器范围见 [只读指南](docs/registers-readonly-guide.md)。

## 怎么用

### 1. 安装

安装 [Node.js（含 npm）](https://nodejs.org/en/download)，在 PowerShell 中执行；以后升级也使用同一条命令：

```powershell
npm.cmd install -g --prefer-online "https://github.com/bei-li16/DebugTUI/releases/latest/download/debugtui-cli.tgz"
debugtui --version
```

也可以下载 [Release 中的便携 ZIP](https://github.com/bei-li16/DebugTUI/releases/latest)，解压后运行 `debugtui.exe`。无板卡时可先用 `debugtui --demo` 预览。

### 2. 准备工程

准备与板上固件匹配、带调试信息的 ELF。内置 ARM/OpenOCD 配置可直接选择 CMSIS-DAP、J-Link 或 ST-Link 探针；固件编译器、探针驱动及自定义板级配置按工程准备。已有外部 GDB/调试服务配置可继续通过 Tools / profile 使用。

首次配置见 [使用指南](USER_GUIDE.md#首次配置与启动)；THA6 MCAL 的工具部署见 [THA6 配置方法](USER_GUIDE.md#tha6-mcal-工程)。

希望 STM32 与多核 R52 共用 Project 格式和 Tools / profile 时，使用 [tools 通用配置说明](tools/README.md)：由 Probe、Chip 和 Debug cores 选择探针和板级设置，工程仅保存 debug.toml。已包含 THA6104/6206/6412 板级配置与 SVD；其他 R52 板卡使用自定义模板。

### 3. 开始调试

连接并上电板卡，在工程根目录启动：

```powershell
cd D:\path\to\your-project
debugtui
```

没有工程 TOML 时自动生成最简 `debug.toml`：Tools / profile 为 `builtin:arm-openocd`、Probe 为 `cmsis-dap`，Chip/Core/ELF 留空。选择 **Chip / Debug cores**、**Program / ELF** 和 **Source root**；STM32F429 选择 `stm32f429` 与 `[0]`。点击 **Start debugging** 前校验并保存配置，Ctrl+S 可只保存。

**Project 行 Enter** 选择当前目录的调试配置，F2 浏览其他文件，F3 也可打开配置列表。顶部保留 Start、Save config、Workspace、Exit；配置行说明通过鼠标悬停或 F1 查看。Tools / profile 和 SVD file 按 **F2** 选择内置资源或外部文件；配置项溢出时可用滚轮或右侧滚动条浏览。

点击源码行号设置断点；**F5** 继续、**F6** 暂停、**F10/F11** 单步；选中变量后用 **Add to Watch** 观察。多核用 **Scope All / Core** 选择控制范围，**F2** 返回配置，**Ctrl+Q** 退出。

需要重新构建或烧录时，先配置 **Build command / Download command**，再使用工作区的 **Build / Download** 按钮。启动调试本身不会自动构建或烧录。

完整配置、源码重映射、实时刷新和自动化接口见 [使用指南](USER_GUIDE.md)。开发与验证见 [架构说明](ARCHITECTURE.md) 和 [测试说明](https://github.com/bei-li16/DebugTUI/blob/main/tests/README.md)。许可证：[Apache-2.0](LICENSE)。
