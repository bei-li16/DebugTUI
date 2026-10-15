# DebugTUI 1.2.2：主分支集成发布

发布日期：2026-10-15。Windows x64 正式版本，标签 `v1.2.2`，从 `main` 发布。

## 交付内容

将 `codex/register-state-fixes` 截至 `e5b2c27` 的 115 个提交快进合入 `main`，保留原提交历史。相对于 1.2.1，本版更新版本与发布文档，将标题宽度判断改为等价表达式以通过严格 Clippy 检查，并重新构建、测试和打包；调试功能与寄存器定义沿用 1.2.1。

- 包含通用 GDB 调试入口、可安装芯片描述及系统寄存器目录继承。
- 包含 STM32F429 的芯片入口、617 项系统寄存器定义及外设 SVD。
- 包含 SCB 搜索、列表滚动、位域可见性及小窗口操作保护修复。
- 包含访问通道选择、读取来源、变量/内存及已适配寄存器的受控写入流程。

本次集成不表示 GitHub issue #1、#2、#3 的全部范围已经验收。寄存器标注 RW 不会自动启用 writer；F429 系统目录仍仅声明 R0–R12、SP、LR、PC 共 16 个 GDB writer。其他类别和硬件范围见 [F429 说明](stm32f429-registers.md) 与 [Host 和芯片描述](chip-description-host.md)。

## 安装与升级

退出正在运行的 DebugTUI，在 PowerShell 执行：

```powershell
npm.cmd install -g --prefer-online --ignore-scripts=false --foreground-scripts --allow-scripts=@debugtui/cli "https://github.com/bei-li16/DebugTUI/releases/latest/download/debugtui-cli.tgz"
debugtui --version
# debugtui 1.2.2
```

固定本版可将 `latest/download` 换成 `download/v1.2.2`。便携用户使用完整 `debugtui-1.2.2-win-x64.zip`。工程配置和包外用户芯片描述在升级时保留。

## 验证范围

本版执行离线 Cargo 全量回归、格式和严格 Clippy 检查、Release 编译，以及最终 EXE 的 Host、芯片配置、终端和分发验证。发布后通过公网附件核对摘要及安装升级流程。实际结果、忽略项和二进制摘要以附件 `validation-1.2.2.json` 为准。

本轮不新增实板认证。1.2.1 的 F429 SCB 31 项、Core 10 项实板读取证据保留在原版本报告中，不作为 1.2.2 新的硬件测试结果，也不外推为全部 617 项可读或可写。

## 发布附件

- `debugtui-cli.tgz` / `debugtui-cli-1.2.2.tgz`：固定名称和版本化 npm 包。
- `debugtui-1.2.2-win-x64.zip`：包含 ARM GDB、OpenOCD、描述文件和许可证的便携包。
- `debugtui-windows-x64-1.2.2.exe`：独立程序，需另备工具与描述文件。
- `corresponding-source.zip`：修改版 OpenOCD 的对应源码，与随包 PROVENANCE 摘要一致。
- `validation-1.2.2.json` / `SHA256SUMS.txt`：验证摘要和附件校验和。

本地未提供三个可选 THA6 SVD，本版不附带。历史版本及其正式附件保留。
