# DebugTUI 1.2.1：寄存器搜索、SCB 滚动与终端布局修复

发布日期：2026-10-15。Windows x64 正式版本，标签 `v1.2.1`，从 `codex/register-state-fixes` 发布为 Latest。以下为相对于 1.2.0 的修复。

## 修复内容

- 寄存器搜索输入期间暂停自动读取，确认或退出搜索后再读取最终列表，避免快速清空并输入 `scb` 时触发中间列表的额外读取、影响界面响应。
- 寄存器列表按实际数据区域计算滚动高度，STM32F429 的 SCB 尾部 `CPACR` 可完整显示；位域选中项在窗口缩放后保持可见，包括 `CP10`、`CP11`。
- 修复外设工具栏换行后遮挡 `Edit value` 的问题；窗口大小变化时保留已可见的选中项及手工滚动位置。
- 列表和设置菜单提供整行悬停反馈，键盘选中状态保持优先。窄窗口保留完整关键快捷键、Generic/Cancel 入口和核心运行状态。
- 终端小于 45×12 时阻止隐藏控件响应鼠标和确认操作，保留退出和取消；写入预览、错误详情支持滚动，长文本与中文输入按终端显示宽度裁剪。

继续包含 1.2.0 的通用 GDB 调试入口、芯片描述自动发现和 F429 系统寄存器适配。F429 随包目录展开为 **617 项定义，其中 SCB 31 项**；本次主要修复 Host 界面和交互，沿用已有寄存器权限、读写规则和访问后端。

## 安装与升级

退出正在运行的 DebugTUI，在 PowerShell 执行：

```powershell
npm.cmd install -g --prefer-online --ignore-scripts=false --foreground-scripts --allow-scripts=@debugtui/cli "https://github.com/bei-li16/DebugTUI/releases/latest/download/debugtui-cli.tgz"
debugtui --version
# debugtui 1.2.1
```

固定本版可将 `latest/download` 换成 `download/v1.2.1`。便携用户解压完整的 `debugtui-1.2.1-win-x64.zip`。工程配置与用户扩展目录保存在包外，升级保留原文件。配置方法见 [Host 与芯片描述](chip-description-host.md) 和 [F429 系统寄存器](stm32f429-registers.md)。

## 验证与范围

修复阶段已验证多种终端尺寸、面板、弹窗和运行状态，保留布局回归与真实 ConPTY 终端证据。F429 使用 FreeRTOS 工程进行 SCB 31 项读取、OpenOCD 独立比对、CPACR/位域显示和窗口缩放验证；这不代表全部 617 项寄存器都已实板验收。R52 等型号仅包含软件布局和目录回归，本次没有新增实板认证。

发布验收包括离线 Cargo 回归、release EXE 终端测试、真实 1.2.0 包升级的 14 项生产分发测试，以及公网 Latest 安装、重复安装和全局安装后的实板复验。各阶段结果、二进制摘要与限制以附件 `validation-1.2.1.json` 为准。

完整包保留现有 ARM GDB/OpenOCD、芯片和探针配置、STM32F429 SVD 及许可证。本地未提供三个可选 THA6 SVD，本版不附带。修改版 OpenOCD 对应源码和来源证明继续随 Release 提供。

## 发布附件

- `debugtui-cli.tgz` / `debugtui-cli-1.2.1.tgz`：固定名称和版本化 npm 包。
- `debugtui-1.2.1-win-x64.zip`：完整便携包。
- `debugtui-windows-x64-1.2.1.exe`：独立程序；工具和描述文件请使用完整包。
- `corresponding-source.zip`：随包修改版 OpenOCD 的对应源码，摘要与 PROVENANCE 一致。
- `validation-1.2.1.json` / `SHA256SUMS.txt`：验证摘要与附件校验和。
