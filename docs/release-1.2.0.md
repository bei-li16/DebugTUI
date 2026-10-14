# DebugTUI 1.2.0：通用调试 Host 与可安装芯片描述

发布日期：2026-10-14。Windows x64 正式版本，标签 `v1.2.0`，从 `codex/register-state-fixes` 发布为 Latest。以下为相对于 1.1.5 的更新。

## 更新内容

- 新增 **Generic GDB** 调试入口：没有专用芯片描述时，连接已有 GDB Server，使用服务器提供的基础寄存器。Setup 的 Chip 选择器按 **G**，或选择 `builtin:generic-gdb`。默认 endpoint 为 `127.0.0.1:3333`；此 profile 不启动 OpenOCD。随包 GDB 支持 ARM，其他架构需配置对应 GDB。
- 自动发现随包 `tools/devices/*.toml` 和用户 `profiles/chips/*.toml` 中的 `[chip]` 元数据。使用现有访问协议的新芯片可通过添加芯片和寄存器描述扩展，无须手工维护 `devices.toml` 或重新编译 Host。用户覆盖及旧索引兼容保留。
- STM32F429 在没有显式寄存器覆盖时自动关联随包专用目录，并配置 PPB/DBGMCU 的归属和访问通道。目录展开为 **617 项定义，含 31 项 `scb.*`**；沿用 1.1.5 已审计的寄存器权限、读写规则、位域与证据说明，本版没有新增 writer。
- 明确配置优先级：显式 CPU/catalogue（含空字符串）优先于芯片默认值；可选默认目录未安装时回退 GDB 并给出诊断，已安装文件损坏、继承缺失或显式路径缺失时报告错误。
- 支持在通用模式与已安装芯片之间切换，保留工程 ELF、任务、核心和显式连接设置；旧式 `.vscode` 工具安装、npm 与完整 ZIP 同步交付新的入口。

## 安装与升级

退出正在运行的 DebugTUI，在 PowerShell 执行：

```powershell
npm.cmd install -g --prefer-online --ignore-scripts=false --foreground-scripts --allow-scripts=@debugtui/cli "https://github.com/bei-li16/DebugTUI/releases/latest/download/debugtui-cli.tgz"
debugtui --version
# debugtui 1.2.0
```

固定本版可将 `latest/download` 换成 `download/v1.2.0`。便携用户完整解压 `debugtui-1.2.0-win-x64.zip` 后运行 EXE。工程和用户扩展目录不随升级覆盖。默认扩展位置为 `%LOCALAPPDATA%/debugtui/profiles/chips/` 和 `profiles/registers/`，也可由 `DEBUGTUI_CONFIG_DIR` 指定。

使用方法见 [Host 与芯片描述](chip-description-host.md) 和 [F429 系统寄存器](stm32f429-registers.md)。已有工程若显式指定通用 `cortex-m4` 或自定义目录，会继续采用原选择；在 Setup 将 CPU registers 改为 **Automatic** 才恢复芯片默认目录。

## 验证与范围

升版前同一功能实现已通过离线 Cargo 849 项（0 失败，3 个原有忽略项）及严格 Clippy。正式 release EXE 重新执行 Host 专项、工具安装、内置工具和 14 项生产分发测试，分发升级使用经公开摘要核验的真实 1.1.5 包；发布后验证公网 Latest 下载、安装升级、重复安装及卸载。具体结果与二进制摘要见附件 `validation-1.2.0.json`。

本轮没有新增实板验收。既有 FreeRTOS 工程离线验证能自动加载 F429 的 617 项定义，工程配置和 ELF 保持原样。F103 仅验证无描述时的基础调试与添加文件后的加载机制，没有交付实板认证的 F103 描述包；全新访问协议仍需开发相应后端。寄存器可访问性、可选功能与实际容量继续依赖运行时证据。

完整包保留现有 ARM GDB/OpenOCD、芯片和探针配置、STM32F429 SVD 及许可证。本地未提供三个可选 THA6 SVD，本版不附带；相关芯片配置仍保留。修改版 OpenOCD 对应源码与来源证明继续随 Release 提供。

## 发布附件

- `debugtui-cli.tgz` / `debugtui-cli-1.2.0.tgz`：固定下载名称和版本化 npm 包。
- `debugtui-1.2.0-win-x64.zip`：完整便携包。
- `debugtui-windows-x64-1.2.0.exe`：独立程序，工具与文件资源请使用完整包。
- `corresponding-source.zip`：随包修改版 OpenOCD 对应源码，摘要与 PROVENANCE 一致。
- `validation-1.2.0.json` / `SHA256SUMS.txt`：验证摘要与附件校验和。
