# DebugTUI 1.1.1：THA6 内置适配与 Setup 交互改进

发布日期：2026-10-08。Windows x64 正式版本，标签 `v1.1.1`，从 `claude/optimizations` 发布并设为 Latest。本说明相对于上一公开正式版本 **1.0.0**；包含此前仅在本机安装、尚未发布的 1.1.0 改动。

## 更新内容

### THA6 工具与调试配置

- 参考 `mcal-vsconfig` 补齐 THA6104、THA6206、THA6412 芯片配置，共用 `builtin:arm-openocd`。Setup 分别选择 Probe、Chip 和 Debug cores，无需复制 GDB/OpenOCD 到固件工程。
- 新增共用 THA 家族配置和 `tools/openocd/tha6.cfg`；同一文件定义 CPU、CTI、APB/AHB target 与 `dbgreset` / `chipreset`，加载配置本身不执行复位。
- 保留物理核编号和端口：core0 为 3333、core1 为 3334；单选 core1 仍使用 3334，稀疏多核选择不重排。APB_1 使用 AP1，AHB_3 使用 AP3，支持独立内存通道及运行中 Watch 采样。
- THA 芯片复位由 core0 执行一次；不含 core0 时禁用该复位入口。复位过程刷新状态、限时 halt、释放 reset catch，并在成功或失败后恢复原 target；错误保留且允许重试。
- THA6206 两个核实测 MIDR 为 `0x410FD161`，默认寄存器目录修正为 Cortex-R52+。基本 GDB 寄存器保留；尚未验证的 R52+ CP15、banked、VFP、timer、PMU、GIC 注入式读取默认关闭，目录存在不代表后端可读。
- 新增 `profiles/tha6-bundled-project.toml.example`，说明 MCAL 辅助核启动屏障和连接顺序。core1-only 是附加调试，要求 core0 已完成固件初始化。
- 本机 THA6104/6206/6412 SVD 随 npm、便携 ZIP 和 tools ZIP 分发；按要求忽略 Git 中的 `tools/svd/THA6*.svd`。缺少这些可选本地文件不阻止打包，源码测试也不强制要求它们存在。来源、摘要与 THA6412 设备名修正见 `tools/svd/README.md`。

### Setup 界面

- 必填配置项名称显示红色，红色说明移至配置框下方；ELF 标红表示源码调试需要符号，仍允许无符号附加连接。
- 每行说明改为鼠标悬停浮窗，不占用配置框高度，也不改变选中项或草稿。浮窗根据窗口边界自动向左、向上调整，长路径换行；F1 可打开完整帮助并滚动。
- 固定配置框布局，修复在 Memory channels 与 Register catalogue 间切换时框体收缩、无故出现滚动条的问题；小窗口仍可滚动。
- 修复切换配置行后仍残留“Register catalogue selection applied”提示的问题。
- Chip config 自动值显示实际芯片默认文件，例如 `Chip default: tha6206.toml`，跟随 Chip 变化；F2 可选 Automatic 或自定义文件。自定义路径保留，清空后恢复自动解析，不把推导的安装绝对路径写入工程。
- 顶部只保留 **Start debugging、Save config、Workspace、Exit**。在 **Project 行按 Enter 或点击**选择当前目录的工程 TOML，F2 浏览其他目录，F3 保留为兼容快捷键；取消保留当前草稿。
- 移除 F4 示例选择与覆盖草稿行为。通用单核、本机程序和双核示例改为 `profiles/*.toml.example` 静态文件，继续随安装包分发。

### 配置保存与安装验证

- 修复 Windows 两个客户端并发保存偏好时，锁文件清理短暂返回“拒绝访问”而导致保存失败的问题；沿用原有五秒上限重试，保持两端配置合并。
- 发布验证脚本兼容新版 npm 的安装脚本策略，只对当前安装命令允许 `@debugtui/cli` 的初始化钩子，不修改用户全局 npm 设置。

## 安装与升级

先退出正在运行的 DebugTUI，在 PowerShell 执行：

```powershell
npm.cmd install -g --prefer-online "https://github.com/bei-li16/DebugTUI/releases/latest/download/debugtui-cli.tgz"
debugtui --version
# debugtui 1.1.1
```

固定安装本版时，将 URL 中的 `latest/download` 换为 `download/v1.1.1`。无需 npm 时，下载 `debugtui-1.1.1-win-x64.zip`，完整解压后运行 `debugtui.exe`；独立 EXE 不包含工具资源。npm 仅用于安装，调试运行无需 Node 常驻。

用户目录中的芯片/寄存器覆盖和工程 TOML 不随升级替换。旧版外部工具 profile 继续使用；切换内置工具需选择 `builtin:arm-openocd`，核对 Probe、Chip、Debug cores、ELF 和 Source root 后保存。

## THA6206 / MCAL 快速配置

1. 在固件工程根目录复制 `profiles/tha6-bundled-project.toml.example` 为 `debug.toml`，按实际工程核对 ELF、Build 和 Download 路径。
2. Setup 选择实际探针，Chip 设为 `tha6206`，Debug cores 选择 `[0]`、`[1]` 或 `[0,1]`。Chip config 和 SVD 可保留自动值。
3. core0-only 与双核按示例启动策略连接；core1-only 要先由 core0 完成初始化。工作区用 Scope All / Core 指定运行控制范围。
4. Build 仍需 GHS 工具链；MCAL Download 继续调用工程的厂商下载命令。本版未新增 OpenOCD THA Flash 烧录算法，启动调试不会自动编译或烧录。

Bao 等固件复用芯片工具配置，另填自身 ELF、构建、下载和启动策略。MCAL 的辅助核启动动作不能直接当作 Bao 启动策略。

## 验证与支持范围

发布前重新执行 Rust 单元与集成回归、THA 配置/复位离线检查、配置兼容、原生终端以及 npm/ZIP 安装升级验证，具体统计及证据见 [TESTING.md](../TESTING.md) 和附件 `software-validation-1.1.1.json`。

本轮功能开发已在 `THA6XXX_MC_AS440` 的 THA6206/CMSIS-DAP/SWD 上完成 core0、core1、双核调试验证，覆盖断点、单步、运行范围、联停、Watch、AHB 实时采样、内存、重连及复位。最后的 Setup 工程切换验收使用真实工程配置，未改写工程 TOML。发布阶段的软件验证不重复烧录板卡。

THA6104/THA6412、其他探针及 JTAG 只完成离线配置检查，未做相应实板验证；THA6206 扩展系统寄存器仍受上述 R52+ 后端限制。SVD 为参考工程提供的 draft 描述，适用范围与来源见工具文档。

## 发布附件

- `debugtui-cli.tgz` / `debugtui-cli-1.1.1.tgz`：相同内容的固定名称与版本化 npm 安装包。
- `debugtui-1.1.1-win-x64.zip`：完整便携包；`debugtui-windows-x64-1.1.1.exe`：独立程序。
- `debugtui-tools-arm-win-x64.zip`：独立工具资源包。
- `corresponding-source.zip`：随包修改版 OpenOCD 的固定对应源码，与 `tools/bin/openocd/PROVENANCE.json` 声明的 SHA256 一致；本版未重编 OpenOCD。
- `software-validation-1.1.1.json` / `SHA256SUMS.txt`：验证摘要与全部发布附件校验和。

完整配置见 [使用指南](../USER_GUIDE.md) 和 [内置工具说明](../tools/README.md)。
