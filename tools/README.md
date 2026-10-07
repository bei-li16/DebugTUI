# 内置 ARM 调试工具

本目录随 DebugTUI 1.0.0 npm 包和便携 ZIP 分发，供 STM32F429 与多核 R52 共用。完整上手与升级说明见 [1.0.0 发布说明](../docs/release-1.0.0.md)。

## 目录内容

```text
tools/
  debug-env.toml   公共 GDB/OpenOCD 环境
  debug.toml       新工程的最简配置模板
  bin/            GDB、OpenOCD、配套脚本及许可证
  devices/        芯片描述与 R52 共用设置
  openocd/        STM32F429 板级配置、R52 模板和 probes/ 探针配置
  svd/            STM32F429 外设描述及许可证
```

## 安装与使用

安装 DebugTUI npm 包或解压完整便携 ZIP 后，在固件工程目录运行 `debugtui`。程序优先读取当前目录的 `debug.toml`；没有时查找其他调试工程 TOML，一个则加载，多个则显示列表；均不存在时自动创建最简 `debug.toml`。仅创建配置可运行 `debugtui --init-project`。

```toml
[tools]
profile = "builtin:arm-openocd"
probe = "cmsis-dap"
```

在 Setup 中选择 Probe、Chip、Debug cores 和 ELF。Probe 支持 `cmsis-dap`、`jlink`、`stlink`，空值继承环境默认值。工具从 DebugTUI 安装位置读取，无需复制到工程 `.vscode`，也不需要在工程内记录安装目录。ELF、源码、日志和显式配置文件路径仍相对于工程 TOML。

Tools / profile 和 SVD file 按 **F2** 可选择包内文件或浏览外部文件，列表显示实际安装路径。内置 SVD 保存为 `builtin:svd/STM32F429.svd`；选择 Automatic 则跟随 Chip。旧 profile 下切换 Probe 会打开兼容工具选择列表；选择内置 profile 后应用探针，Esc 保留原配置。切换时只迁移内容完全一致的旧版原始 SVD，自定义文件保留。

内置文件随软件升级更新。自定义芯片 TOML 可在 Setup 的 **Chip config** 中选择，或放到 DebugTUI 用户目录的 `profiles/chips/<chip>.toml`；优先级为工程指定文件、用户文件、内置文件。自定义文件可用 `extends = "builtin:devices/tha6206.toml"` 继承默认配置，再引用同目录的板级 cfg/SVD。R52 连接前须补全真实板级参数。

仓库中的 `install.ps1` 仅供旧版工程复制工具到 `.vscode` 的兼容用法，不随 npm 包分发。新工程直接使用内置工具。自定义配置的完整示例见 [用户指南](../USER_GUIDE.md#内置工具与自定义芯片配置)。
