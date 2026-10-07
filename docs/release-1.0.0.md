# DebugTUI 1.0.0：更新与使用说明

发布日期：2026-10-08。面向 Windows x64，从 `claude/optimizations` 分支发布，标签 `v1.0.0`，正式 Release / Latest。以下更新相对于上一正式版 v0.9.3，包含中间只读寄存器预发布及后续工具、配置和界面改进。

## 更新内容

| 范围 | 1.0.0 的变化 |
| --- | --- |
| 开箱工具 | npm 和完整便携 ZIP 内置 ARM GDB、带 DebugTUI 适配的 OpenOCD、必要脚本、STM32F429 SVD 和许可证；无需将工具复制到固件工程的 `.vscode` |
| 通用配置 | 同一 Project 格式和 `builtin:arm-openocd` 工具 profile 用于 STM32 与多核 R52；Probe、Chip、Debug cores 独立选择 |
| Probe | 支持 `cmsis-dap`、`jlink`、`stlink`；通过 OpenOCD 使用探针。旧 profile 不支持独立 Probe 时，引导选择兼容的内置 profile，取消保留原草稿 |
| 工程发现 | 启动目录优先加载 `debug.toml`，否则查找其他有效工程 TOML；一个直接加载、多个供选择、没有则创建最简通用配置。损坏文件保留并提示 |
| 包内资源选择 | Tools / profile 和 SVD file 的 F2 列表显示实际安装位置，可选择内置或外部文件；保存 `builtin:` 引用，不绑定本机 npm 路径 |
| 芯片覆盖 | 工程指定 Chip config 优先于用户同名芯片文件，再使用内置配置；支持 `extends`。用户定制存放在安装包之外 |
| 多核 | 保留芯片的物理 core ID 与固定端口映射；例如 `[1,3]` 不压缩成 `[0,1]`。寄存器目录、能力、缓存和样本来源按核隔离 |
| 系统寄存器 | 提供 M3/M4/M7/R52/R52+ 目录选择、能力 Probe、字段解码、来源/访问状态、MPU 总览、取消读取及逐核缓存。实际可读范围依赖身份与后端证明，详见下表 |
| 调试视图 | 改善 Watch/Locals 成员展示、宽短/窄高窗口布局、面板缩放、命令搜索与快捷键帮助；保留源码、断点、内存、SVD、构建和下载操作 |
| 交互 | Setup 增加 5 行字段容量，溢出支持右侧滚动条点击/拖动及滚轮；滚动保留选择与草稿。Symbols、Watch expression 与 Console 输入框提供悬停反馈 |
| 性能 | 事件唤醒替代固定轮询、减少无效鼠标重绘、缓冲终端输出、缓存布局/源码索引和搜索排序，SVD 在后台解析并按进程复用 |

## 安装和升级

先正常退出正在运行的 DebugTUI，然后在 PowerShell 执行：

```powershell
npm.cmd install -g --prefer-online "https://github.com/bei-li16/DebugTUI/releases/latest/download/debugtui-cli.tgz"
debugtui --version
# 本次发布输出：debugtui 1.0.0
```

固定安装 1.0.0：

```powershell
npm.cmd install -g "https://github.com/bei-li16/DebugTUI/releases/download/v1.0.0/debugtui-cli.tgz"
```

无需 npm 的安装方式：下载 `debugtui-1.0.0-win-x64.zip`，完整解压后运行 `debugtui.exe`。保持同目录的 `tools` 等资源目录完整。独立 EXE 仅供已有外部工具配置的用户使用，不附带工具。npm 只负责安装，调试运行无需 Node 常驻；固件编译器和探针驱动按工程准备。

Release 提供 `SHA256SUMS.txt`，可使用 `Get-FileHash <下载文件> -Algorithm SHA256` 核对。此分发使用 GitHub Release URL，不依赖 npm Registry 的 `@latest` 标签。

## 首次启动：只需要一份工程配置

在固件工程根目录运行 `debugtui`。若没有可加载的工程 TOML，会自动创建：

```toml
version = 3

[tools]
profile = "builtin:arm-openocd"
probe = "cmsis-dap"

[debug]
chip = ""
cores = []

[program]
elf = ""
source_root = "."

[session]
on_exit = "detach"
log_dir = "debug-logs"
```

Chip、Core 和 ELF 不预选。Build/Download 命令没有工程覆盖；SVD、CPU/寄存器目录、Memory channels 随所选芯片解析，源码重映射默认关闭。工具从安装目录读取；工程文件、源码、日志及自定义资源相对于工程 TOML 解析。只创建配置而不进入界面可执行 `debugtui --init-project`，离线预览可执行 `debugtui --demo`。

### STM32F429

1. 保持 Tools / profile 为 `builtin:arm-openocd`，按实际探针选择 Probe。
2. 选择 Chip `stm32f429`、Debug cores `[0]`，指定当前固件的 ELF 与 Source root。
3. SVD 保持 Automatic，可使用内置 STM32F429 定义；也可在 SVD file 按 F2 显式选择内置或自定义 SVD。
4. Ctrl+S 只保存；点击 Start debugging 或 F5 才启动连接。需要构建/下载时，另配置 Build/Download 命令并使用工作区相应按钮，启动调试不会自动编译或烧录。

### 多核 R52 / THA6

保持相同工具 profile，选择实际 Probe、Chip（THA6104/6206/6412）和要调试的物理 core ID。例：THA6206 可选择 `[0]`、`[1]` 或 `[0,1]`；THA6412 可选择 `[1,3]`。模板约定端口为 `3333 + core ID`，自定义板级配置须保持对应关系。

**随附 R52 配置是待补全模板。** 在工程内或用户配置目录保存自己的芯片 TOML，通过 Setup 的 Chip config 选择；可用 `extends = "builtin:devices/tha6206.toml"` 继承工具默认，再覆写 service 参数以引用自己的板级 OpenOCD cfg。根据真实 SoC/板卡补齐 JTAG/DAP、AP、CTI、target 名称、复位/下载动作及可选 SVD；不要直接修改 npm 安装目录，也不要把模板地址当作已验证的硬件事实。内置 `r52-template.cfg` 会在未补齐配置时明确报错。

多核运行控制用 Scope All / Core 选择范围；系统寄存器和写入入口仍按当前对象与 owner 执行，不由 Scope All 自动广播。自定义芯片、每核 ELF 和寄存器覆盖示例见 [使用指南](../USER_GUIDE.md#内置工具与自定义芯片配置)。

## 常用操作

| 页面 | 操作 |
| --- | --- |
| Setup | ↑/↓ 选字段，Enter 编辑，F2 浏览，F3 切换工程，F4 示例，Ctrl+S 保存；滚轮或滚动条浏览字段 |
| 工作区 | F2 返回 Setup，F5 继续，F6 暂停，F10/F11 单步，Ctrl+Q 退出 |
| 查找/观察 | Ctrl+K 搜索符号，Files 中 Ctrl+F 过滤文件，Watch expression 输入表达式并 Add |
| 寄存器 | 选择正确核心与目录，查看 Probe/访问状态及样本来源；不支持的对象显示原因，不自动改权限或切换模式 |
| 写入 | 仅后端声明独立 writer 的对象可编辑；先 Preview，再明确 Apply。通用写入入口不表示所有系统寄存器均可写 |

## 旧版本迁移

已有工程会继续加载自己的 TOML，不会用默认模板覆盖 ELF、Watch、断点或构建命令。旧 `.vscode/debug-env-*.toml` 和外部服务配置可继续使用；需要改为内置工具时，在 Tools / profile 按 F2 选择 Bundled ARM / OpenOCD，再设置 Probe 并保存。

只有与随包原始文件内容完全一致的旧 STM32F429 SVD 会在切换时迁移为内置引用；自定义 SVD 保留。SVD 的 Automatic 跟随 Chip，显式文件选择保持固定，Disabled 明确禁用。客户芯片/寄存器覆盖存放在用户目录或工程内，升级不替换它们。

## 支持边界与验证

| 对象 | 本次发布的支持范围 |
| --- | --- |
| M3/M4/M7 | 公共系统寄存器及各核的 FPU/cache/TCM 等目录；实际实现、容量和访问通道仍需核对 |
| 多核 M/R | 每核身份、目录、权限和缓存隔离；软件测试通过不代表所有 SoC 的 AP/target 已适配 |
| R52 | 只读基线为身份/控制/MPU，以及有完整证明的当前 Debug EL2 路径；低 EL/Guest 权限不足时保持 Unknown/Restricted |
| R52+ | 可选目录；不能把 R52 的身份或后端验收直接沿用为 R52+ 支持证明 |
| Banked/VFP/Timer/PMU/GIC/STM、Trace、写入 | 现有可选协议与能力门禁保留；不承诺整个子系统完整可读写，见各专项说明 |

正式版本号不表示开发 TODO 或全部硬件用例已完成。本次执行离线软件、原生终端、本机 GDB、安装/升级与发布附件检查；没有连接物理探针、复位或下载板卡。详细验证见 [TESTING.md](../TESTING.md) 和 Release 附件 `software-validation-1.0.0.json`；待执行的实板用例见 [tests/cases](../tests/cases/)。

M/R 核寄存器基线和限制见 [只读指南](registers-readonly-guide.md)，完整配置及可选后端见 [USER_GUIDE.md](../USER_GUIDE.md)。本版包内提供 Markdown 文档；仓库历史 PDF 手册按其原标注版本保留。

## 附件说明

- `debugtui-cli.tgz`：固定名称的 npm 安装包，供 latest 下载地址使用。
- `debugtui-cli-1.0.0.tgz`：同内容的版本化安装包。
- `debugtui-1.0.0-win-x64.zip`：包含工具和文档的完整便携包。
- `debugtui-windows-x64-1.0.0.exe`：独立 DebugTUI 程序。
- `corresponding-source.zip`：所附 DebugTUI-patched OpenOCD 的固定对应源码、依赖、补丁、构建配方及许可证，与工具 PROVENANCE 的摘要一致。
- `software-validation-1.0.0.json`、`SHA256SUMS.txt`：本次软件验证摘要和所有发布附件的校验和。
