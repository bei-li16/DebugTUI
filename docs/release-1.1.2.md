# DebugTUI 1.1.2：日志目录默认关闭与 Setup 配置

发布日期：2026-10-09。Windows x64 正式版本，标签 `v1.1.2`，从 `claude/optimizations` 发布并设为 Latest。以下为相对于 **1.1.1** 的改动。

## 更新内容

- 在空目录首次启动时，自动创建的工程默认关闭日志文件输出，Setup 的 **Log directory** 显示 `(not set)`。
- 选中 **Log directory** 后，`←/→` 在关闭与 `debug-logs` 之间切换；Enter 输入自定义路径，清空后关闭。Ctrl+S 保存，Start 应用。
- F2 打开文件夹选择器，仅显示文件夹；Enter 进入、Backspace 返回上一级、Space 选择当前文件夹、Esc 取消。编辑日志路径时也可以按 F2。
- 自定义路径支持空格、中文及 Windows 路径分隔符；相对路径以工程 TOML 所在目录为基准。保存或选择路径不会创建日志目录，开始调试连接时才创建。
- 修复清空工程日志路径后重新继承工具 profile 日志设置的问题。关闭时保存 `log_dir = ""`，解析为禁用文件日志，不会误写到工程根目录。
- 已有工程中明确配置的日志目录保留。Console 的日志显示保持可用，关闭文件日志不影响调试命令或 Console 输出。

## 安装与升级

先退出正在运行的 DebugTUI，在 PowerShell 执行：

```powershell
npm.cmd install -g --prefer-online "https://github.com/bei-li16/DebugTUI/releases/latest/download/debugtui-cli.tgz"
debugtui --version
# debugtui 1.1.2
```

固定安装本版时，将 URL 中的 `latest/download` 换为 `download/v1.1.2`。也可下载 `debugtui-1.1.2-win-x64.zip`，完整解压后运行 `debugtui.exe`。旧工程继续使用原日志设置；需要关闭时，在 Setup 的 Log directory 按方向键关闭，或清空路径后保存。

## 验证与支持范围

本版对 Setup、配置解析、日志时间戳/轮换、真实 Windows ConPTY 操作、主机 GDB 日志输出及 npm/ZIP 安装升级进行软件验证。具体结果见 [TESTING.md](../TESTING.md) 和附件 `software-validation-1.1.2.json`。

本次未改变调试后端、板级复位或烧录流程，发布验证不执行板卡连接、复位或下载。芯片适配与支持边界沿用 [1.1.1 发布说明](release-1.1.1.md)。内置 GDB/OpenOCD 二进制保持不变，THA6 SVD 随运行包分发，继续在 Git 中忽略。

## 发布附件

- `debugtui-cli.tgz` / `debugtui-cli-1.1.2.tgz`：固定名称与版本化 npm 安装包。
- `debugtui-1.1.2-win-x64.zip`：完整便携包；`debugtui-windows-x64-1.1.2.exe`：独立程序。
- `debugtui-tools-arm-win-x64.zip`：工具资源包，包含更新后的默认工程模板。
- `corresponding-source.zip`：随包修改版 OpenOCD 的固定对应源码，与 PROVENANCE 声明的 SHA256 一致。
- `software-validation-1.1.2.json` / `SHA256SUMS.txt`：软件验证摘要与附件校验和。

完整配置见 [使用指南](../USER_GUIDE.md)。
