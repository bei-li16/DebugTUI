# DebugTUI

## 是什么

DebugTUI 是基于 GDB/MI 的终端调试工作台，可在 PowerShell、Windows Terminal 或 VS Code 终端中使用。支持源码断点、单步、Watch、寄存器与 SVD 外设查看、单核/多核调试，以及调用工程的 Build / Download 命令。

本分支版本 **0.10.0-readonly.1**，用于非主分支只读系统寄存器预发布，支持 **Windows x64**。正式版安装地址仍指向最新正式 Release。M3/M4/M7、多核与 R52 当前 Debug EL2 的范围见 [只读指南](docs/registers-readonly-guide.md)；硬件用例尚未执行。调试时无需 VS Code、Python 或 Node 常驻进程；GDB、调试服务和下载工具由工程提供。

## 怎么用

### 1. 安装

安装 [Node.js（含 npm）](https://nodejs.org/en/download)，在 PowerShell 中执行；以后升级也使用同一条命令：

```powershell
npm.cmd install -g --prefer-online "https://github.com/bei-li16/DebugTUI/releases/latest/download/debugtui-cli.tgz"
debugtui --version
```

也可以下载 [Release 中的便携 ZIP](https://github.com/bei-li16/DebugTUI/releases/latest)，解压后运行 `debugtui.exe`。无板卡时可先用 `debugtui --demo` 预览。

### 2. 准备工程

准备与目标匹配的 GDB、调试服务（如 OpenOCD 或 J-Link GDB Server），以及与板上固件对应的带调试信息的 ELF。工具路径、端口和服务参数放在工程配套的 `debug-env.toml`；已有工程直接复用其配置。

首次配置见 [使用指南](USER_GUIDE.md#首次配置与启动)；THA6 MCAL 的工具部署见 [THA6 配置方法](USER_GUIDE.md#tha6-mcal-工程)。

### 3. 开始调试

连接并上电板卡，在工程根目录启动：

```powershell
cd D:\path\to\your-project
debugtui
```

在 Setup 中确认 **Project**、**Tools / profile**、**Program / ELF** 和 **Source root**；使用芯片选择配置时，再选择 **Chip / Debug cores**。点击 **Start debugging**，配置会保存到项目 TOML。

点击源码行号设置断点；**F5** 继续、**F6** 暂停、**F10/F11** 单步；选中变量后用 **Add to Watch** 观察。多核用 **Scope All / Core** 选择控制范围，**F2** 返回配置，**Ctrl+Q** 退出。

需要重新构建或烧录时，先配置 **Build command / Download command**，再使用工作区的 **Build / Download** 按钮。启动调试本身不会自动构建或烧录。

完整配置、源码重映射、实时刷新和自动化接口见 [使用指南](USER_GUIDE.md)。开发与验证见 [架构说明](ARCHITECTURE.md) 和 [测试说明](https://github.com/bei-li16/DebugTUI/blob/main/tests/README.md)。许可证：[Apache-2.0](LICENSE)。
