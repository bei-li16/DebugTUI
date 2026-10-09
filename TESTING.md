# DebugTUI 验证记录

## 1.1.2 正式发布验证（2026-10-09）

Cargo、lockfile 和 npm 统一为 **1.1.2**。本版新增默认关闭文件日志、Setup 日志开关/路径编辑/F2 文件夹选择，并修复空路径重新继承 profile 日志目录的问题。更新与操作见 [发布说明](docs/release-1.1.2.md)。

- 受影响 Rust 模块 **86/86**：Setup **52**、配置 **32**、日志 **2**，分别记录于 `artifacts/release-1.1.2-launch.log`、`release-1.1.2-config.log`、`release-1.1.2-logging.log`。本次按变更范围回归，未重复统计 1.1.1 的全量集成结果。
- 严格 Clippy `--all-targets -- -D warnings`、格式检查和 Release 构建通过：`artifacts/release-1.1.2-clippy.log`、`release-1.1.2-build.log`。
- 最终 Release EXE 的 Windows ConPTY 日志配置流程 **5/5**，覆盖空目录首次启动、开关、编辑/清空/取消、F2 选文件夹、保存及重启；CLI **12/12**。日志：`artifacts/release-1.1.2-terminal.log`、`release-1.1.2-cli.log`。
- 主机真实 GCC/GDB 执行 **14** 条调试命令，验证三次会话日志轮换、逐行时间戳、旧日志不覆盖。另验证未设置、显式空路径、空路径覆盖 profile 三种关闭状态，连接与重连均不创建日志文件，Console 继续输出：`artifacts/release-1.1.2-gdb-logs.log`。
- 完整便携包的内置工具与配置 **15/15**，含真实 GDB/OpenOCD 离线资源加载、自动建工程、探针选择及 THA 物理核/端口：`artifacts/release-1.1.2-bundled-tools.log`。首轮误传不带工具目录的 Cargo EXE，被脚本拒绝；完整包复验通过，首轮记录保留为 `release-1.1.2-bundled-tools-initial.log`。
- 从公开 **v1.1.1** 包升级的生产 npm/EXE/ZIP 验证 **14/14**，覆盖初始化钩子、CMD/PowerShell 入口、重复安装、卸载/重装、客户目录保留、错误覆盖与失败钩子：`artifacts/release-1.1.2-distribution.log`。旧包 SHA256 为 `8994065c69f9e9de6c42f71e12aaad73950baafb2819e659a7f37f4dd0872536`，与 GitHub Release 附件摘要一致。

最终 EXE SHA256 为 `c4fa18e589237ab07e2a4b39a02151ddd5baee271e0888d5eadaae837530d160`。npm/ZIP 中的程序、工具与模板由生产打包脚本逐项核对；THA6 SVD 随本地实际资源打包，保持 Git 忽略。随包 OpenOCD 及固定对应源码不变。机器可读摘要随 Release 附件 `software-validation-1.1.2.json` 交付。

本次不执行板卡连接、复位或烧录，THA 实板与扩展寄存器支持边界沿用 1.1.1 记录。

## 1.1.1 正式发布验证（2026-10-08）

Cargo、lockfile 和 npm 统一为 **1.1.1**。相对于公开正式版 1.0.0，本次包含此前本机 1.1.0 的 THA 工具和 Setup 改动，以及发布回归中发现的 Windows 并发保存修复。更新、升级和支持边界见 [发布说明](docs/release-1.1.1.md)。

- Rust 单元测试 **539 通过、2 ignored**：`artifacts/release-1.1.1-cargo-verified.log`。两项跳过分别为 Windows 剪贴板写入和需连接 THA6206 的 Source-to-Watch 实板测试。最终并发保存修复后，配置模块 **32/32** 复验通过，双客户端连续 **10 轮** 同时保存保持两核偏好：`artifacts/release-1.1.1-config-final.log`、`artifacts/release-1.1.1-concurrent-save.log`。
- 严格 Clippy `--all-targets -- -D warnings`、格式检查和 Release 构建通过；最终构建用时 4 分 24 秒，日志 `artifacts/release-1.1.1-clippy-final.log`、`artifacts/release-1.1.1-build-final.log`。
- 真实 Windows ConPTY **8/8**、CLI **12/12**、便携包工具/配置 **15/15**、旧工程安装器兼容 **19/19**、THA 配置/复位离线检查 **30/30**。日志分别为 `artifacts/release-1.1.1-terminal.log`、`release-1.1.1-cli.log`、`release-1.1.1-bundled-tools.log`、`release-1.1.1-chip-profiles.log`、`release-1.1.1-tha-tools.log`；最终程序的复验记录随发布附件列出。
- 用完整便携包加载 `THA6XXX_MC_AS440` 的 6 份实际工程配置，均保持 DISCONNECTED 且没有 GDB/MI 命令；记录 `artifacts/release-1.1.1-real-project.json`。工程 TOML 和包外用户 `profiles/devices.toml` 的前后 SHA256 相同，未改写实际工程。
- 本机三个 THA SVD 已真实解析并随 npm/ZIP 分发，Git 中保持忽略。测试不强制要求克隆仓库中存在这些本地文件。固定 OpenOCD 的 EXE 摘要与 PROVENANCE 一致；对应源码从 v1.0.0 公网附件取得，SHA256 为 `67a14bcc54fbd89337073246bd3cf0e1ffcd5ddcd378bb84c424925676e614e8`。

最终安装包以 Release EXE SHA256 `f6fb9cfafc75d53ad22b98a69b89a94b70d62670482f93de4146298bbe191e31` 验证。`test-register-distribution.cjs` 使用从 v1.0.0 Release 下载且摘要匹配的真实旧包作为基线，**14/14** 通过；覆盖生产 npm/ZIP 资源、EXE/ZIP 替换、实际 postinstall、CMD/PowerShell 入口、重复安装、卸载、客户配置保留和失败钩子：`artifacts/register-distribution-1791467607868-5b56804f/report.json`。原生终端、CLI、内置工具与双客户端并发保存也针对最终 EXE 复验通过。THA SVD 出现在 npm 清单和完整 ZIP 中，未被 Git 暂存。

Rust 集成回归的 **17 个测试目标、260 个用例全部通过**，与上述单元测试合计 **799 个不同 Rust 测试通过、2 ignored**。分批证据为 `artifacts/release-1.1.1-integration-final.log`、`release-1.1.1-integration-remaining.log`；`register_distribution` 的 Rust 包装器只调用同一 Node 驱动，本轮直接以最终 Release EXE 和真实 v1.0.0 历史包执行该驱动的 14 项生产验证，单独统计，未将它重复计入 Rust 用例。完整机器可读摘要随 Release 附件 `software-validation-1.1.1.json` 交付。

首轮失败日志保留：F1 从特定行帮助改为通用浮窗后，一项旧测试没有关闭上一行帮助，已更新导航预期；Clippy 检出的等价比较写法已修正。模拟 TCL 服务五秒接入预算曾在高并发下超时，串行复验通过；双核 connect 含两名 worker 及同步，原十秒聚合等待不足，测试预算单独改为三十秒，未修改生产通信超时。双客户端保存曾遇到 Windows 锁文件清理的短暂 ACCESS_DENIED，现纳入原五秒上限内重试，并增加连续并发保存验证。失败原记录没有覆写为通过。

功能开发阶段已在 THA6206/CMSIS-DAP/SWD 与 MCAL 工程完成 core0、core1、双核和芯片复位检查；发布阶段只执行软件/配置/安装验证，没有再次复位或烧录板卡。THA6104/6412、JTAG、其他探针及 R52+ 扩展寄存器不作为已通过实板的功能宣称。

## 1.0.0 正式发布软件验证（2026-10-08）

Cargo、lockfile 和 npm 版本统一为 **1.0.0**，从 `claude/optimizations` 发布。更新细节、首次启动默认配置、STM32/R52 操作与迁移说明见 [发布说明](docs/release-1.0.0.md) 和 [使用指南](USER_GUIDE.md)。历史 PDF/TeX、个人审计提示词与本地参考资料不进入运行包，包内文档使用 `docs/*.md`。

- Release 编译通过（2 分 30 秒），沿用既有 target，两个编译任务、关闭增量缓存。日志：`artifacts/release-1.0.0-build.log`。
- 完整功能运行记录：`artifacts/functional-1791389900591-e3057e31/report.json`。24 个套件中 22 个首次通过，包含 CLI、终端、本机 GDB、变量/位域写入、断点、内存、SVD、日志及多核。首次失败记录保留，下面两项复验补足验收，未将失败原报告改写为通过。
- Cargo 首次统计 790 通过、1 失败、2 ignored。失败发生在位域 Node MI 夹具的 1 秒启动期限，尚未处理首条 MI 初始化命令；降低并发后仍有相同失败。该测试用于验证位域策略而非启动耗时，将此夹具预算调整为 5 秒后，整个 `write_access` **42/42** 通过（默认并发），对应 `artifacts/release-1.0.0-write-access-final.log`。合计 **791 个不同 Rust 测试通过、2 ignored**；doc tests 通过（0 个示例），没有修改生产通信超时。
- `test-devices.ps1` 更新 Project→Tools/profile→Probe→Chip 的导航路径，并要求真正的 Saved 路径消息；芯片选择、core1/双核保存重载、自定义芯片及用户目录保留复验通过：`artifacts/devices-tui-5bb9dfe7e4dd4709abcec896dec2ac6f/report.json`。
- Rust 格式及严格全 target Clippy 通过：`artifacts/release-1.0.0-clippy.log`。生产 npm/ZIP 与真实 postinstall 的隔离分发检查 **14/14** 通过：`artifacts/register-distribution-1791390075996-d4cf107d/report.json`；此处升级基线为明确标注的离线 fixture。公网验收脚本改为在独立用户目录执行真实安装钩子，发布后使用实际 v0.9.3 完成升级/重装/卸载检查。
- 已下载 v0.9.3 历史包并与其 Release 的 SHA256SUMS 核对；所附 OpenOCD、DLL 与对应源码包均符合 PROVENANCE，记录在 `artifacts/release-1.0.0-inputs/verified.json`。Release 提供 `software-validation-1.0.0.json` 与全附件 SHA256。

本次没有连接物理探针、复位、烧录或执行实板测试；已有硬件 case 继续保留其待执行状态。

## Setup 滚动与输入框悬停（2026-10-07，0.10.0-readonly.6）

Setup 最大高度从 30 行调整为 35 行，字段区域增加 5 行容量；溢出时显示可点击、拖动的右侧滚动条，字段区支持滚轮。滚动不会选择字段或提交编辑草稿，键盘操作将当前字段滚回视野。Symbols、Watch expression 及共用的搜索输入控件增加悬停底色与边框反馈，不改变编辑焦点，不发出调试请求。

- 单元测试 **530 通过、2 ignored**，严格 Clippy 通过。日志：`artifacts/setup-scroll-hover-unit-final.log`、`artifacts/setup-scroll-hover-clippy.log`。覆盖五行扩容、全部字段显示、小窗口滚轮/点击/拖动/释放、草稿保留、弹窗隔离、键盘回显，以及三种窗口尺寸的悬停/离开/焦点保持。初轮悬停测试补正了紧凑布局需选择 Watch 面板和已有焦点装饰的预期。
- 实际 Ratatui 单元格已导出并检查：[Symbols 悬停](artifacts/setup-scroll-hover-preview/input-hover-symbols.png)、[Watch 悬停](artifacts/setup-scroll-hover-preview/input-hover-watch.png)。这两张图来自测试渲染，原始单元格 JSON 同目录保留。
- 首次生成的配置仍直接使用 `tools/debug.toml`：内置 ARM/OpenOCD、CMSIS-DAP、空 Chip/Core/ELF、源码根目录 `.`、detach 和 debug-logs；没有新增默认 STM32/R52 选择。使用指南补充生成值及滚动操作。
- Release 编译通过（2 分 39 秒），使用既有 target、两个构建任务、关闭增量缓存。真实 Windows ConPTY 的 [8 项工作区检查](artifacts/terminal-20261007-235934-3e26217a/report.json) 和 [7 项配置选择检查](artifacts/resource-picker-f989eeffb6364814aad36d12d3bc3153/report.json) 全部通过；包含大窗口完整字段及小窗口滚动条。鼠标交互通过 App/Setup 事件测试验证，ConPTY 检查键盘和终端布局。本轮没有执行实板调试。

## 内置文件选择与旧 profile 的 Probe 修复（2026-10-07，0.10.0-readonly.5）

Tools / profile 与 SVD 的 F2 列表直接提供安装目录内的资源和外部文件入口，显示解析后的真实路径；选择包内文件保存为可迁移的 `builtin:` 引用。旧 profile 下切换 Probe 会提供兼容工具选择，取消不修改草稿，接受后应用请求的探针并保留工程字段。显式切换内置 profile 时，只迁移字节完全匹配的旧原始 SVD 副本；自定义 SVD 保留。SVD 支持内置引用、Automatic 和 Disabled，浏览器选择包内资源也使用逻辑引用。

- 完整单元测试528通过、2 ignored；严格 Clippy 与 release 构建通过。日志：`artifacts/resource-picker-unit-final.log`、`artifacts/resource-picker-clippy.log`、`artifacts/resource-picker-build.log`。初轮失败为两处旧界面预期，按新选择入口及路径提示更新后复验通过。
- 真实 Windows ConPTY 的7项 Setup 操作通过：[终端报告](artifacts/resource-picker-1d63e74d19cd40689400e039f4ef43b9/report.json)。包含旧工程加载、Probe 提示/取消/应用、内置资源浏览、SVD 选择、保存保持 ELF/Watch/任务以及退出；屏幕文本与原始终端输出随报告保留。
- 完整便携 ZIP 的14项配置/工具检查通过：[报告](artifacts/bundled-tools-1791386558885-0db9800d/report.json)。新增 SVD 逻辑路径保存和路径越界拒绝；同时验证工程移动、三种 Probe 和 R52 核选择。
- 旧配置兼容19项：[报告](artifacts/chip-profiles-1791386530054-a804c4e7/report.json)；CLI 12项：[报告](artifacts/cli-1791386550975-d97015cf/report.json)。生产 npm/ZIP 逐项内容与摘要检查通过。

本轮全部为离线软件检查；没有连接探针、复位、下载或访问物理芯片。终端测试只操作隔离工程的配置页。

## 内置工具、Probe 参数与启动目录发现（2026-10-07，0.10.0-readonly.4）

npm 与便携 ZIP 包含 `tools/bin`、`devices`、`openocd`、`svd` 和公共模板。新工程使用 `builtin:arm-openocd`，按可执行文件安装位置定位资源，不复制到工程 `.vscode`，也不保存安装绝对路径。启动目录优先读取 `debug.toml`，否则检索有效工程候选；无候选才排他创建最简配置，多候选显示列表，损坏文件保留并报错。Setup 增加 Probe 和 Chip config；工程指定芯片文件优先于用户同名文件，再使用内置默认。R52 模板仍要求补齐真实板级参数。

本轮软件验证全部通过：

- `cargo test --locked --lib --jobs 2`：526 通过、2 ignored；严格 Clippy `--all-targets -- -D warnings` 与 release 编译通过。日志：`artifacts/bundled-tools-unit.log`、`artifacts/bundled-tools-clippy.log`、`artifacts/bundled-tools-build.log`。
- 完整便携 ZIP 的13项工具/启动/探针/多核/路径检查：[报告](artifacts/bundled-tools-1791385203282-a9b71f87/report.json)。真实运行随附 GDB，并让 OpenOCD 解析后 `shutdown`；不执行 `init`。验证默认新建、重复启动、工程移动、用户及工程芯片覆盖、非法 Probe 和损坏 TOML 保留。
- 旧工程安装器兼容19项：[报告](artifacts/chip-profiles-1791385161757-4f459752/report.json)。保留既有 ELF/Watch、自定义 SVD、配置备份和相对路径逻辑。
- CLI 实际进程12项：[报告](artifacts/cli-1791385205834-c84aafe6/report.json)。
- npm/ZIP 生产打包与隔离安装14项：[报告](artifacts/register-distribution-1791385201481-6f821854/report.json)。覆盖真实安装钩子、同版重装、卸载重装、客户配置保留、失败钩子和资源逐项一致性；升级基线为明确标注的离线合成包，不作为历史 Release 升级证明。

本轮未连接物理探针或板卡；STM32/R52 的配置解析通过不代表实板调试通过。上板用例见 [芯片配置用例](tests/cases/chip-profiles.md)。后面的 `.vscode` 安装记录为旧版历史与兼容用途，当前默认布局见 [tools/README.md](tools/README.md)。

## 移除工具依赖清单（2026-10-07）

按用户要求删除 `tools/dependencies.lock.json`。安装与打包直接枚举 `bin/`，不再读取、生成或分发该清单，移除 `-UpdateLock`。重新安装时仅删除目标 `.vscode` 下遗留的同名清单，不再依据旧清单删除其他工具文件。配置备份与修改过的 SVD 保留逻辑继续使用文件内容比较。

19项离线检查通过：[报告](artifacts/chip-profiles-1791382155438-1651074c/report.json)。包括无清单安装、全部工具文件复制一致性、旧清单移除、STM32/R52 配置、工程移动和既有配置保留。新工具包 `artifacts/tools-no-lock-20261007/debugtui-tools-arm-win-x64.zip` 含81个文件，无依赖清单，逐项摘要与源文件一致。PowerShell/JavaScript 语法检查和 `git diff --check` 通过；未连接硬件。

## 工程内相对路径布局与单一安装入口（2026-10-07）

`tools/debug-project.toml.example` 改为 `tools/debug.toml`，安装到工程根目录；工具配置、芯片描述、OpenOCD、SVD 与二进制安装到工程 `.vscode/`，不增加 `tools/` 子层。工程通过 `./.vscode/debug-env.toml` 引用公共配置，其余资源使用相对路径或目录变量。安装器从随附模板创建新工程，不再另行拼装或向 `.vscode` 复制工程模板。`tools` 仅保留 `install.ps1`，移除 BAT 包装，发布打包移至 `scripts/package-tools.ps1`，同步更新发布入口和说明。

12项设备/配置单元测试通过；实际已安装 `0.10.0-readonly.3` 的19项离线检查通过，报告：[CLI 检查](artifacts/chip-profiles-1791381761805-0cdd1d1e/report.json)。覆盖 Windows PowerShell 5/PowerShell 7 安装、完整工程移动后的相对引用、不同工作目录、中文/空格路径、STM32三种探针和R52多核、同盘工程内外 ELF 路径转换、跨盘 ELF 修改前拒绝、既有配置备份及自定义 SVD 保留。没有执行上板测试。

新工具包82个文件，唯一脚本为 `tools/install.ps1`，包含新命名的 `tools/debug.toml`，逐项 SHA-256 均与源文件一致：[布局与打包检查](artifacts/tools-project-layout-20261007/report.json)。PowerShell/JavaScript 语法检查及 `git diff --check` 通过。后续旧路径记录仅保留为历史证据，当前安装布局见 [tools/README.md](tools/README.md)。

## 工具目录精简与统一命名（2026-10-07）

公共入口统一为 `tools/debug-env.toml`；移除重复的 `debug-env-universal.toml`、两个探针专属 profile 和三个旧 STM32 板级脚本。原 `chips/` 改为 `devices/`，原 `chip/` 改为 `svd/`，板级脚本及探针统一到 `openocd/`。STM32F429 只保留一个 `openocd/stm32f429.cfg`，三种探针共用。以下早期记录的旧路径仅用于历史追溯，当前布局以此条和 tools/README.md 为准。

安装器统一写入公共 profile 和独立 probe 选择，新工程使用 version 3 并由用户选择 Chip/Core。升级保留原工程偏好及版本行格式，只移除仍为原始内容的自动生成 SVD 覆盖；自定义 SVD 内容和路径保留。硬件脚本与 OpenOCD 构建配方的路径已同步更新并通过语法检查，未运行上板或重编译后端。

32项配置、12项设备/Setup单元测试通过。已安装的 `0.10.0-readonly.3` 实际执行16项工具布局、探针/R52配置、OpenOCD离线解析和安装迁移检查全部通过，报告：`artifacts/chip-profiles-1791380707923-81a05dc3/report.json`。硬件测试配置辅助函数已做无硬件检查。最终工具包83个文件，仅一个公共profile、一个STM32板级cfg，无旧目录，每个包内文件摘要均与源文件一致；SVD原始SHA-256保持不变。打包结果：`artifacts/tools-layout-cleanup-20261007/report.json`。本轮调整可由已安装程序直接使用，无需升版重编译TUI；没有修改固件工程副本或连接硬件。

## 公共工具与芯片/探针配置拆分（2026-10-07，0.10.0-readonly.3）

`tools/debug-env-universal.toml` 仅保留工具路径、芯片目录和独立探针选择。新增 `tools/chips/<chip>.toml`、R52 共用家族模板、`tools/probes/*.cfg` 和不绑定探针的 STM32F429 板级 cfg。按 Chip 查找外部文件，支持最多8层 `extends`；路径按声明文件解析，保留工程优先/显式空值、寄存器配置来源和物理核身份，旧内嵌 backend 与单芯片 profile 继续兼容。打包和安装脚本包含新目录。

验证通过：配置相关32项、设备/Setup相关12项单元测试；新增4项文件配置测试涵盖继承、中文/空格路径、优先级、配置来源、缺文件/非法字段/backend不匹配/循环/深度限制、新增芯片不改公共入口、探针独立选择。`cargo fmt --all --check`、严格Clippy、`git diff --check` 通过。Release 编译通过，用时2分33秒，沿用既有 target。

`node scripts/test-chip-profiles.cjs --binary target/release/debugtui.exe` 的16项实际程序检查通过：Windows PowerShell 5安装到隔离的中文/空格目录，STM32三种探针、R52单核/双核/非连续核/四核、OpenOCD拆分脚本离线解析、R52未配置拒绝、三个旧profile加载。报告：`artifacts/chip-profiles-1791379547445-6433c4c3/report.json`。测试子进程隔离PowerShell模块路径，并分别检查旧单核与新多核协议结构。

`node scripts/test-register-configuration.cjs --binary target/release/debugtui.exe` 的16项既有实际程序回归通过，报告：`artifacts/register-configuration-1791379559282-3af0591b/report.json`。独立工具包89个文件与源文件摘要全部一致，记录：`artifacts/chip-profiles-build-20261007/tools-package-final.json`。所有CLI检查仅查询配置/状态；OpenOCD仅解析后shutdown。没有连接硬件。延后上板用例与复验入口见 [芯片配置用例](tests/cases/chip-profiles.md)。

## 通用 Project / Tools profile（2026-10-07）

新增 `tools/debug-env-universal.toml`、`tools/debug-project.toml.example` 和待填写的 `tools/config/r52-template.cfg`。沿用既有 Chip → backend / Debug cores 解析，允许选中 backend 提供严格校验的 `[program] svd`，保持 profile 相对路径、工程覆盖和显式空值语义。ELF/source_root 仍不能放入工具 profile。

针对本次改动的验证：设备相关 12 项、配置相关 28 项单元测试通过；新增用例覆盖 STM32 → R52 → STM32 的 SVD/内存通道/寄存器路由隔离、core1 固定3334、多核/非连续 `[1,3]`、未选 Chip/非法 Core 拒绝、工程 SVD 覆盖/禁用和 backend 非法字段拒绝。`cargo fmt --all --check`、`cargo clippy --locked --lib --bin debugtui -- -D warnings`、`git diff --check` 通过，实际 Debug EXE 构建成功。沿用一个 target，关闭增量及调试信息；本次没有重复完整软件回归。

实际 EXE 在隔离用户配置下仅执行 `status` 后 EOF，STM32单核及R52单核/双核/非连续核/四核5种组合均保持 DISCONNECTED，核心名称与端口符合物理编号。随附 OpenOCD 离线解析3种既有STM32探针cfg均成功，未填写R52模板按预期报错；均未执行 `init`。共9项软件入口/配置检查通过，命令、结果与摘要保留在 `artifacts/universal-tools-20261007/report.json`。工具包检查确认包含通用 profile、Project 模板、R52 cfg 与 F429 SVD；安装脚本语法检查通过，未对用户工程执行安装。

本次未连接探针或板卡，也未验证任何实际 R52 DAP/AP/CTI/复位配置；R52模板由用户后续补充。旧v0.9.3和原v0.10.0-readonly.1二进制不能使用新backend SVD字段，需本次源码构建；旧单芯片profile保持兼容。

## 最小 tools 集与补丁版 OpenOCD（2026-10-07，claude/tools-minimal）

`tools/` 跟踪的文件从 1090 个、38.6 MiB 减到 102 个、15.0 MiB；运行包 `debugtui-tools-arm-win-x64.zip` 为 74 个文件、6.9 MB，此前发布的 tools ZIP 约 14.5 MB。移除 SEGGER J-Link（许可证要求每次再分发事先取得 SEGGER 书面授权）及其 BAT 入口，默认 `debug-env.toml` 改为 OpenOCD + J-Link；OpenOCD 换为 openocd-adapter 补丁构建（上游 d3ebb8d），`openocd.exe` 与 `source.lock.json` 的 Windows 候选摘要一致，随附文件均与构建的 PROVENANCE.json 一致；脚本保留 40 个；新增 ST-Link profile。

软件验证，未连接探针或板卡：DebugTUI 九项协议命令在 dummy armv8r target 上通过；F429 的 J-Link、CMSIS-DAP、ST-Link 配置均可解析；30 个 STM32 target × 3 种接口共 90 个组合，精简前后的脚本树结果一致（87 个可解析，`stm32x5x_common.cfg` 是被包含的片段，两边单独解析都失败）。从 git 导出的 tools 可以独立打包并启动 OpenOCD；`bin/` 有清单外的文件时拒绝打包。完整 Cargo 515 项单元测试、严格 Clippy 通过。三项集成测试在本机满载时按时限失败，单独重跑通过；`register_distribution` 在路径超过 260 字符的检出目录中失败，短路径下 14/14 通过。功能套件 cli、terminal、devices-tui、distribution 通过。

未执行：三种探针的实板回归，R52/THA6 板级 cfg 与补丁版 OpenOCD 的组合，依赖 SEGGER Server 的 `test-step-isolation.cjs`，以及改为 OpenOCD 的 `test-lifecycle.ps1`、`test-hardware.ps1`、`test-step-hardware.cjs`（只做了语法检查）。

## REG-406 STM（2026-10-06，开发分支）

完整Cargo **406单元＋175集成通过，2 ignored，共581通过**；F24 **156/156**为证据匹配模式数，非用例数。525324 ms无超时；其他**22外层功能套件未选择**。完整报告/Markdown/unit.log逐字节镜像 `artifacts/functional-1791244770256-6c55a69d/`，61份原始子报告保留于总报告引用位置；Node大产物留在C盘，核对 `artifacts/register-stm-report-mirror.json`。严格Clippy通过，日志 `artifacts/register-stm-clippy.log`；目录再生成及后端11项源锁静态核对通过，见 `artifacts/register-stm-static-audit.json`。

八项新单元、六项worker/EXE集成覆盖独立组件身份/功能、可选接口、保留编码、四核/Scope All、真实AP大端路线、身份/权限/映射拒绝、peer代次失效和后验变化停止读取；失败旧值保留来源。独立RAM驱动正向及未就绪/身份错误/参考值不符的拒绝路径已验证。软件夹具不证明芯片集成或实板权限；八项环境case全部SKIPPED，默认驱动五阶段SKIPPED；独立C基线仅离线编译。 首轮既有共享矩阵夹具自动停止竞争及修复记录见[STM自检](docs/register-stm.md)，原完整MI日志不变断言保留。只勾选REG-406，28完成／43待完成，目标active；未上板、未全局安装或发布Release。

## REG-007只读能力矩阵（2026-10-06，开发分支）

完整Cargo **398单元＋169集成通过，2 ignored，共567通过**；F24 **154/154**证据模式匹配，531352 ms无超时，其余22外层套件未选择。总报告/Markdown/unit.log逐字节镜像 `artifacts/functional-1791241698266-d38320e0/`，56份原始子报告在F盘，核对 `artifacts/register-matrix-report-mirror.json`。严格Clippy通过，日志 `artifacts/register-matrix-clippy.log`；后端源锁静态核对通过，见 `artifacts/register-matrix-static-audit.json`。

七项新单元、四项新集成验证完整类别/位宽/条件、配置路线、旧或不完整receipt、MMIO核心映射/非原子宽度、共享peer失效、实际CLI六项离线配置和FAULT导出。实际二进制软件环境驱动四阶段通过，并拒绝错误独立高字值；原失败夹具记录保留。[八项环境case](tests/cases/register-matrix.md)均SKIPPED，默认驱动四阶段SKIPPED，未上板。只勾选REG-007，27完成／44待完成；配置和观察不等于整类实板支持，见[能力矩阵](docs/register-capability-matrix.md)。未安装或发布Release。

## VFP当前Debug状态与独立源码包（2026-10-06，开发分支）

完整Cargo **391单元＋165集成通过，2 ignored，共556通过**；F24 **149/149**个证据模式匹配，538354 ms无超时，其余22外层套件未选择。总报告、Markdown及unit.log逐字节镜像到 `artifacts/functional-1791238088481-8911e18d/`，原53份子报告在F盘，核对 `artifacts/vfp-proof-report-mirror.json`。严格Clippy通过，日志 `artifacts/vfp-proof-clippy.log`。

两项新单元及两项多核worker测试验证当前EL和八种保存模式分离、Core1/Scope All、旧或伪造证明、失败原值来源与后续控制；现有四类实际EXE读流程和五类写流程完整回归。Windows/Linux完整重建、七套生产事务/七协议、实际dummy命令及Windows本机DLL/三项离线配置通过。VFP生产模型73读/166写故障点；八项TCP测试含DTR传输位变化、低EL拒绝及空异常失败报告。源ZIP补齐JSON夹具，旧包拒绝、新包全新解压后脱离工作区运行七套模型、八项TCP和后端命令通过。原专项夹具及空异常失败日志保留，最终日志/当前包记录见[VFP证明](docs/register-vfp-proof.md)、[源码自检](docs/register-backend-source.md)。

Windows包检查十一项通过，新增JSON缺失/篡改拒绝；原Git refs负向夹具补齐JSON后继续独立验证Git空目录，原九项未删减。日志 `artifacts/vfp-proof-windows-package-regression.log`，首轮夹具错误日志保留。

八项环境case全部SKIPPED，未上板、安装或发布。REG-304低EL合法读取仍未完成；总TODO保持 **26完成／45待完成**，目标active，源码/全局安装基线0.9.3。

## S/D/Q存储视图与后端源码（2026-10-06，开发分支）

四新单元、三新集成补齐同次物理pair/容量raw、D16/D32完整视图、特殊值和双核Scope All、失败原值来源；实际EXE延后驱动四种软件场景通过。四种正常startup标本对象离线编译，64字数据节独立核对；八项环境case均SKIPPED。固定Git基线重新应用补丁，十一项源锁与Windows/Linux候选/源码包一致，后端源码可获得/可构建证据完整。[存储视图](docs/register-storage-views.md)、[后端源码](docs/register-backend-source.md)。

完整 Cargo **389 单元＋163 集成通过，2 ignored，共552通过**；**F24 145/145**为证据匹配模式数，非用例数。完整运行496515 ms，无超时；其余**22外层功能套件未选择**。原始完整报告与53份子报告位于F盘，完整总报告/日志的逐字节镜像为 `artifacts/functional-1791235241817-19eb948a/report.json` 和同目录 `unit.log`；镜像核对见 `artifacts/storage-views-report-mirror.json`。严格Clippy通过，日志 `artifacts/storage-views-clippy.log`。REG-305软件范围与REG-006源码/构建自检已验收，完整TODO为**26完成／45待完成**，目标active；未执行上板、安装或发布Release。

## 银行当前Debug状态（2026-10-06，开发分支）

生产C模型37成功、41受限、162未知、1110故障点与三十条GNU编码、八模式正常固件三十参考槽已验证；Windows/Linux重建、七协议/事务、Windows本机DLL/离线配置与对应源码包逐字节检查通过。没有目标指令执行或上板结果。EL1具体模式保持Unknown，REG-303不勾选。详见[自检](docs/register-banked-proof.md)和[八项环境case](tests/cases/register-banked-proof.md)。

完整 Cargo **385 单元＋160 集成通过，2 ignored，共545通过**；**F24 138/138** 为证据匹配模式数，非用例数。完整运行489102 ms，无超时；其余**22外层功能套件未选择**。报告 `artifacts/functional-1791232474564-90d658f4/report.json`，完整Cargo为同目录 `unit.log`；严格Clippy通过，日志 `artifacts/banked-proof-clippy-final.log`。

## R52 MMIO owner 路线（2026-10-06，开发分支）

三个专项单元、三个真实 worker/EXE/MI/TCP 集成通过。独立 TRM 地址、四个非连续 core/两 cluster、缺失 owner 零访问、Scope All 选中核、实际 AP target/endpoint、64 位字序、手工副作用/WO、失败旧值来源与驱动拒绝流程有证据。完整 Cargo **379 单元＋153 集成通过，2 ignored，共 532 通过**；**F24 132/132** 是证据匹配模式数，不能当作用例数。整套 497753 ms，无超时；其余 **22 功能套件未选择**。报告 artifacts/functional-1791225291601-66910f0b/report.json，完整 Cargo 为同目录 unit.log；严格 Clippy 通过，日志 artifacts/register-mmio-clippy-final.log。

独立只读固件以 GNU Arm 11.4.0 严格警告离线编译，二十四项地址/二十六个外部字和 MIDR MRC/CPSR guard 已核对，对象未执行；报告 artifacts/register-mmio-firmware-report.json。十二项环境 case 均 SKIPPED，默认五阶段驱动报告 artifacts/register-mmio-hardware-1791224011438-fe09a965/report.json。前序原生候选、十一项源锁/对应源码/补丁仍一致，见 artifacts/register-mmio-inherited-package-audit.log。

[MMIO 自检](docs/register-mmio.md) 记录边界、失败和独立证据；[用例](tests/cases/register-mmio.md) 与 [模板](profiles/tha6-mmio.toml.example) 已准备。新鲜 MMIO 能力 Probe、完整 Debug/低 EL ICV、其余 TODO 和最终发布仍待完成，计数保持 24/47。

## GIC 当前原生容量与 AP 条件（2026-10-06，开发分支）

完整 Cargo 375 单元＋150 集成通过，2 ignored，合计 525 通过；F24 129/129 个证据模式匹配，严格 Clippy 通过。完整报告 `artifacts/functional-1791220138851-f975cb60/report.json`，日志同目录 unit.log；其余 22 功能套件本批未选择，最终完整验收仍待做。四项 GIC 单元、七项实际 EXE/worker/MI/Tcl 集成与十三项能力回归覆盖独立物理/虚拟容量、双核 Scope All、No 无数据读且完整依据保留、IAR/WO、别名/原生 32 位、权限/错误/取消/上下文变化。旧 GDB/Hyp 容量假设及 CTLR 合法低位夹具断言已经纠正，原失败报告保留。

Windows/Linux 七项原生事务/协议、两个候选完整构建、Windows 本机依赖/配置/源码包及九项包拒绝检查通过；当前十一项源锁/补丁/候选/对应源码一致性见 `artifacts/register-gic-final-package-audit.json`。只读固件 GNU Arm 离线编译及三十七条独立 MRC 反汇编检查通过，对象未执行。十项环境 case 均 SKIPPED，默认驱动五阶段 SKIPPED；软件驱动另验证独立基线、未就绪/不匹配和清理，双核隔离由独立 worker 测试验证。

只新增勾选 REG-408 的软件范围，进度 24/47。完整 Debug/GIC MMIO/低 EL ICV、其他 TODO、最终全套回归/升版/安装/Release 仍未完成；全局安装与源码基线仍为 0.9.3。[GIC 自检](docs/register-gic.md) 与 [进度](docs/registers-development-status.md) 记录支持边界和原始证据。


## 寄存器与显式内存通道开发分支（2026-10-04–05，尚未发布）

2026-10-05 Timer 一致性批次：完整 Cargo **368 单元＋137 集成通过，2 ignored**，共 **505 通过**，F24 **118/118**；严格 Clippy、格式及差异检查通过。新增 u64 极限/原生位宽/旧证据单元与实际 EXE 的回绕、冻结、倒退/异常高字/过旧基线集成；生产 C 新增48个动态 pair 回读，Windows/Linux同头文件模型/离线命令通过，原有候选未重编译、当前源码包已更新并原生验证。主机起止区间与单项 MRC32/MRRC64 绑定来源，不推导跨项/跨核同时性。首轮 G: ENOSPC 失败日志保留，完整归档生成目录释放空间后整套重新通过；证明见 [开发进度](docs/registers-development-status.md) 和 [一致性自检](docs/register-timer.md)。仅新增 REG-403，已完成／未完成 **22/49**；其余22套件未选择，[十四项环境 case](tests/cases/register-timer.md) 均 SKIPPED，未上板、安装或发布。


2026-10-05 专用 Timer 后端批次：完整 Cargo **367 单元＋136 集成通过，2 ignored**，F24 **114/114**；严格 Clippy、格式及差异检查通过。生产 C 覆盖十五项独立编码/474 故障点，Windows/Linux 新目录候选构建及原生 Windows 验证通过；双核实际 EXE/MI/Tcl 覆盖完整原始值/当前 Debug 证据、权限原因、旧值来源、取消/上下文变化及恢复未知 FAULT。旧 MRRC 与新 Timer 独立固件基线驱动各六软件阶段通过，未执行目标代码。完整报告和限制见 [开发进度](docs/registers-development-status.md) 与 [Timer 自检](docs/register-timer.md)。仅完成 REG-402，已完成／未完成 **21/50**；完整低 EL 权限与采样一致性仍待完成，其余22套件未选择，[十三项环境 case](tests/cases/register-timer.md) 均 SKIPPED，未上板、安装或发布。


2026-10-05 Timer 独立基线批次：完整 Cargo **365 单元＋131 集成通过，2 ignored**，F24 **107/107**；严格 Clippy、目录比对、格式及差异检查通过。专用 Hyp/ready 门禁、四个稳定六十四位独立符号、两个计数器和 peer 保持的软件驱动通过；错误模式/未就绪/基线不同在正确阶段停止并清理。GNU Arm 11.4 离线编译全部九项 MRC/六项 MRRC 编码通过，没有 Timer/模式写指令或目标执行。正常 EL 与 Debug state 权限区别已核对，完整应用权限适配仍未完成。其余22套件未选择；[Timer 自检](docs/register-timer.md)、[开发进度](docs/registers-development-status.md) 记录证据，[十二项环境 case](tests/cases/register-timer.md) 均 SKIPPED。总计已完成／未完成 **20/51**，未上板、安装或发布。


2026-10-05 Timer 阶段批次：完整 Cargo **365 单元＋130 集成通过，2 ignored**，F24 **106/106**；严格 Clippy、目录重新生成比对、格式及差异检查通过。新增三项单元、一项协调器/MI/真实 Tcl 软件集成，独立验证十五项编码、位宽/字段/访问说明、Unknown/No 零请求、选定物理核、高低位及失败原值保留。首次完整回归发现旧 GDB 夹具没有 Timer 实现声明，修正夹具后保留原断言并重新完整验证；失败与成功报告见 [开发进度](docs/registers-development-status.md)。其余22套件未选择。[Timer 自检](docs/register-timer.md)、[十二项环境 case](tests/cases/register-timer.md) 明确软件证据边界，环境 case 均 SKIPPED，REG-401/402/403 继续未勾选。总计已完成／未完成 **20/51**，未上板、安装或发布。


2026-10-05 REG-110 条件依据批次：完整 Cargo **362 单元＋129 集成通过，2 ignored**，F24 **102/102**；严格 Clippy 通过。覆盖全父链条件/WO、数量边界、声明与当前观察、成功保存依据、连续失败原值依据、双核及共享拒绝、旧 JSON 和只读详情。R52 Hyp 物理 PMU 数量限于四个，EL0/EL1 的 PMCR.N 受限值保留但不推断物理缺失；物理 ICC 限于 TRM 五位。首次完整回归的 VFP 清理失败已保留；修复测试助手 exit 早于 stdout 排空的顺序，四项确定性 Node 回归及原 VFP 驱动通过后重新完整验证。其余22套件未选择。[条件自检](docs/register-eligibility.md)、[开发进度](docs/registers-development-status.md) 给出完整报告；[八项环境 case](tests/cases/register-eligibility.md) 均 SKIPPED，未上板、安装或发布。

2026-10-05 REG-107 目录交付批次：完整 Cargo **353 单元＋125 集成通过，2 ignored**，F24 **89/89**；严格 Clippy、优化构建通过。实际生产 npm／EXE／ZIP、空用户扩展目录、postinstall、CMD／PowerShell 入口、客户数据保留、损坏 override 及非连续多核来源的十四项 case 通过；发布方摘要校验后的真实 v0.9.3 包独立替换也十四项通过。代码尚为 0.9.3，历史包验证是同版本不同内容替换；最终升版与公网安装仍待执行。完整报告、此前 300 秒超时及调整为 600 秒后成功回归的记录见 [目录交付自检](docs/register-distribution.md) 和 [开发进度](docs/registers-development-status.md)。[八项环境 case](tests/cases/register-distribution.md) 均 SKIPPED，未上板或改系统安装。

2026-10-05 状态与取消批次：308 项单元、83 项集成通过，2 项 ignored；严格 Clippy 通过。实际 worker／MI 与真实 Tcl 验证取消后整批丢弃、末项丢弃、旧样本／Probe 保留、selector 仍恢复、未知恢复仍 FAULT、直接 MPU／VFP 以及 Scope All 当前 worker 隔离；九项 Ratatui 状态／计数／说明与取消测试通过。统一 runner 的 `--only unit` 记录为 `artifacts/functional-1791150195450-fbd63962/report.json`，F24 42 个模式全部有通过证据；其余 20 suites skipped。上板与实际终端视觉用例 [已准备但未执行](tests/cases/register-status-cancel.md)，不能把软件测试记为该验收完成。

2026-10-05 VFP 批次：专用 Hyp VFP 通道、十五项能力采样、D16/D32 与 S/D/Q 共享物理 pair 已接入。生产 C 事务覆盖 63 个故障点、R0/R1 恢复、完整 DSPSR／FPEXC／HCPTR 变化拒绝、未使能与安全权限拒绝；Windows/Linux 全新目录构建和真实离线协议／参数／状态检查通过。REG-H03 默认 4 skipped；实际二进制双核软件驱动的 D32、D16、未使能、TCP10 受限四种流程各 5 阶段通过，独立 GNU Arm 固件钩子编译通过。没有执行上板测试；EL1/Guest 合法 VFP 读取和其他 TODO 缺口仍保留。完整测试和打包记录见 [开发进度](docs/registers-development-status.md)。

2026-10-05 MPU／MAIR 批次：完整测试通过 288 项单元测试、53 项集成测试，另 2 项既有环境测试 ignored；严格 Clippy 通过。新增全部 256 种 MAIR 编码、各实现数量的直接索引计划、EL1／EL2 控制与权限、MAIR 两半独立失败、跨核心／停止点／帧隔离、原始失败值保留、宽窄 Ratatui 键鼠和手工读取测试。真实 worker／TCP／Tcl 软件测试覆盖完整区域、不可用项目、身份／数量／实际模式变化前拒绝、末尾模式变化丢弃整批及旧值过期、目录末项重定向前拒绝、EL2 数量为零、恢复失败隔离和 Scope All 当前物理核心。

完整并发回归曾发现 Python／tkinter 每次 MRC 启动导致软件夹具超过实际 TCP 500 ms 超时。夹具现先启动并复用一个 Python 解释器，经 JSONL 为每个请求执行原有真实 Tcl 控制流，生产超时保持原值。修复后完整回归和严格 Clippy 通过。

`node scripts/test-mpu-regions-hardware.cjs` 默认 4 skipped，不访问目标；显式 `--run --project FILE --core NAME --case JSON --binary FILE` 才执行 REG-H04 的完整 MPU／MAIR 子集。示例 `tests/fixtures/mpu-regions-board.example.json` 包含全部 24 个 EL1 和 20 个 EL2 软件区域，实际板卡需替换全部原始值、MAIR、CPU 修订与独立证据来源，并去掉 `software_example` 标记。固件钩子 `mpu-regions-board.c` 不设置 MPU 或选择器；主机 GCC 严格编译检查通过，不能作为 ARM 目标构建证明。实际 DebugTUI 二进制＋MI／Tcl 双核软件夹具的 5 阶段通过，核对当前／peer 控制、选择器、工程文件及所有区域；报告 `board_tests_executed=false`。未执行上板测试。

显示与偏好批次：完整 `cargo test --locked --quiet` 通过 282 项单元测试、44 项集成测试，另 2 项既有环境测试 ignored；`cargo clippy --locked --all-targets -- -D warnings` 通过。覆盖 1–128 位整数边界、IEEE 负零／无穷／带载荷 NaN／次正规数、向量分量次序、实际 Ratatui 宽窄布局及键鼠、取消、搜索提交、芯片／核心／目录隔离与旧偏好迁移。测试不连接板卡，显示格式测试不证明真实 VFP 后端能力。

`node scripts/test-register-display.cjs --binary target/debug/debugtui.exe` 使用两个独立的实际 DebugTUI 进程和故意不存在的 GDB 路径，六阶段通过：并发保存不同核心、旧全局设置保留双方记录、无效输入不修改文件、目标上下文及 MI 无访问、两客户端退出。用例发现并修复 Windows 跨进程保存导致的配置覆盖及部分 TOML 写入：保存现使用独占句柄、重新合并最新配置和原子替换。既有 capability TCP 夹具的接收连接也显式改为阻塞模式，避免 Windows 非阻塞状态继承引起的超时失败。

该历史批次当时先保留本地。当前交付约定是每轮开发、测试和自检后提交并推送非主分支；完整 TODO 完成后才发布 Release，最新提交和进度以开发记录为准。

选择器批次：完整测试通过 273 项单元测试、43 项集成测试，另 2 项既有环境测试 ignored；严格 Clippy 通过。新增实际 Tcl 8.6／worker／TCP 软件测试：EL1／EL2 MPU 和 PMU 的原选择器／target 保存、索引边界、成对读取及恢复；MCR 已接受后报错、ISB 错误、恢复写／回读／屏障失败分别覆盖。恢复未知时 FAULT 且不重试，普通失败的旧值标不可用，数量变化清除 Probe；双核 Scope All 只改当前物理核心。界面显式操作、单个在途请求和迟到错误隔离也有测试。

`node scripts/test-register-selectors-hardware.cjs` 默认 4 skipped，不连接目标。显式 `--run --project FILE --core NAME --case JSON --binary FILE` 才运行 REG-H04／REG-H06 的选择器子集；`tests/fixtures/register-selectors-board.example.json` 的地址／值来自软件模型，必须替换为实际专用固件的独立证据，并填写 CPU 修订、暂停函数、实际区域数量及可选另一核心。钩子 `register-selectors-board.c` 不修改 MPU／PMU／CP15BEN；计数器案例要求固件已暂停计数（PMCR.E=0），脚本不会配置固件。每个成对值同时对照声明值和独立直接索引读取，逐次确认所有选择器／控制寄存器及原工程未变。实际二进制＋MI／真实 Tcl 软件夹具通过 5 阶段；报告 `board_tests_executed=false`，上板未执行。真实 OpenOCD／CP15BEN 未设置时的 ISB 路径、MAIR 内存类型及完整区域视图仍待验收。

能力探测批次：267 项单元测试、37 项集成测试通过，2 项既有外部环境测试 ignored；严格 Clippy 通过。新增十项显式 R52 能力采样及带来源的上下文事实；实际身份未适配、ID 读取失败、非 Hyp、异步 RUNNING、前后实际线程／帧变化及过期上下文分别验证，不把 Unknown 当作未实现。双核 worker 夹具确认 Scope All 仅读取当前核心；界面使用物理停止代次，观测事实不写回客户配置。目录补齐 EL1 MPU 16–23 区域并新增 ICH_VTR；物理／虚拟 GIC 解码各自保留来源。

`node scripts/test-register-capabilities-hardware.cjs` 默认 4 skipped，不连接目标。显式 `--run --project FILE --core NAME --case JSON --binary FILE` 才执行 REG-H01/H14 的能力子集；案例见 `tests/fixtures/register-capabilities-board.example.json`，可选独立固件钩子见同目录 `register-capabilities-board.c`。示例中的 MIDR、区域数量、非 Hyp 预期及稳定寄存器必须按实际核、修订和已验证通道填写，MCAL/Bao 和 core0/core1 分别运行。实际二进制＋MI 软件夹具通过 5 阶段（含清理），报告 `board_tests_executed=false`，原工程不变。`--gdb FILE`／`--openocd FILE` 可记录操作员指定的主机工具路径、哈希与版本；不能将该文件身份当作已经运行的调试服务器身份。完整目标描述、可选类别、工具构建对应关系与其余 REG-H 用例仍未验收。

变量写入批次：256 项单元测试、27 项集成测试通过，2 项既有环境测试 ignored；严格 Clippy 通过。真实 GCC/GDB 的 `node scripts/test-variable-write-gdb.cjs target/debug/debugtui.exe` 8 阶段全部通过，覆盖精确 32/64 位数值、指针、struct／数组成员、const 及别名拒绝、浮点负零、调用者帧 Locals 和过期草稿；项目哈希不变。键鼠／窄窗口、表达式校验、根存储先检查、权限／类型／地址改变、优化掉、函数调用策略恢复、发送后错误／断连／超时、清理失败和无重试另有 MI 管道及 Ratatui 证据。

`node scripts/test-variable-write-hardware.cjs` 默认生成 4 skipped，不连接目标；`--run --project FILE --core core0 --case FILE --fixture-function debug_write_fixture --binary FILE` 才使用已暂停的独立普通 RAM 夹具。夹具源码见 `tests/fixtures/variable-write-board.c`，案例格式见 `tests/fixtures/variable-write-board.example.json`；成员顺序、符号、owner、帧、字节序必须按实际固件填写，Watch 根需已在工程中声明。core0/core1 及双核共享 owner 分别运行并保留结果；driver 不运行／暂停／复位 CPU。通过实际二进制＋MI 软件夹具验证 5 阶段（含清理），独立 RAM 回读、两侧符号及原项目哈希检查通过，报告 `board_tests_executed=false`。128 位变量、位域／引用／特殊浮点和其他上板场景仍待补足；本批未升版或发布。

RAM／MMIO writer 批次：250 项单元测试、19 项本地集成测试通过，2 项既有外部环境测试 ignored；严格 Clippy 通过。新增 64 位地址／4096 字节边界及两侧哨兵、声明 RAM／Flash／MMIO 区域拒绝规则、字节序、非零帧、权限变化、回读不符及发送后错误无重试。实际 TCP TCL 软件夹具验证一个 MMIO 字访问、布局／AP 大小端转换、保留位、邻接 W1C 中性值、WO 和读副作用对象无回读、物理 CPU 状态及 Apply 重新查询字节序。共享 owner 不能由 JSON 伪造 peer 状态；All scope 不广播写入，其他核心缓存与草稿失效。

`scripts/test-memory-write-hardware.cjs` 默认 4 项 skipped、零目标连接；通过实际 DebugTUI 可执行文件及 MI 软件夹具执行全部 5 阶段（含关闭），原工程保持不变，RAM 与哨兵明确恢复；报告 `board_tests_executed=false`。本批未执行上板测试。当前本地 ARM GDB 只读探测确认 `data-write-memory-bytes` 存在、`may-write-memory=on`，不作为目标写权限证明。MMIO、缓存一致性及其余延后上板案例仍需补足。

写入后续批次：247 项单元测试、10 项本地集成测试通过，2 项既有外部环境测试 ignored；严格 Clippy 通过。新增 SVD 写元数据／枚举／约束继承、精确数值及混合特殊位规划、Core 的真实 MI 预览／应用／取消和独立 writer 校验。Ratatui 用例覆盖当前选中行、键鼠、45×12 窄窗口、迟到预览、错误保留输入及发送失败；双核 Scope All 只发一次当前核写入，切核／帧／线程／权限／重连均拒绝旧草稿，发送后超时／断连不自动重试或回写旧值。

准备 `scripts/test-register-write-hardware.cjs`，默认四项 skipped、零目标连接；Core 流程在 `--software-fixture` 模式验证成功并恢复普通工作寄存器，报告明确 `board_tests_executed=false`。首次软件运行发现退出时无变化也重写工程；已修正，最终原工程 SHA256 保持不变。新增 F26 软件证据；完整 TODO 的其他 writer、读取能力、延后上板用例和发布交付仍待完成，尚未更改当前正式版本或安装包。

- 230 项单元测试和 5 项本地集成测试通过，2 项既有外部环境测试 ignored；`cargo clippy --locked --all-targets -- -D warnings` 通过。
- Setup 通道草稿、保存／取消、核心限制及窄窗口经过测试。Watch、Peripherals、Memory 均有可见入口；Memory 范围和通道按芯片／核心保存，读取失败不重试或回退，运行中不隐式停核。
- MI 管道和 TCP 夹具覆盖连续完整字节、64 位地址与原始值、单项寄存器失败、RUNNING 通知及会话／核心／停止点／栈帧／范围变化后迟到响应失效。双核心夹具确认 Scope All 不广播内存读取。
- `tests/functional-coverage.json` 新增 F24／F25 关联本批软件证据与限制。未运行完整功能套件或新上板验收，未升版、全局安装、推送或发布；完整待完成范围见 [开发进度](docs/registers-development-status.md)。

## 0.9.3 本地升版与安装（2026-10-03）

- Cargo、Cargo.lock 工程条目和 npm 包版本同步为 **0.9.3**，README 当前版本更新；包含下述配色、宽屏布局、圆角填色及 Setup 默认 Project / 字段箭头循环修改。
- 新版本 Release 编译通过；**185 项单元测试、1 项集成测试通过，2 项外部环境测试忽略**。CLI 进程验证 **12/12**：[报告](artifacts/cli-1791016093959-46e43a5c/report.json)；隔离 npm 包生命周期 **7/7**：[报告](artifacts/distribution-20261003-162818-7a2dc171/report.json)，覆盖升级、重复安装、CMD/PowerShell 入口、渲染、配置保留、卸载和重装。
- 已打包 `artifacts/debugtui-cli-0.9.3.tgz` 并全局安装 `@debugtui/cli@0.9.3`；EXE、CMD 和 PowerShell 入口均显示 `debugtui 0.9.3`。安装 EXE 与 Release 构建 SHA256 相同：`03e7ab0f2d78df9becc16b7cda2e92fc67b5518eaf880842b942af7ef9f47950`；15 份工程与芯片目录文件校验值保持不变：[安装报告](artifacts/install-0.9.3-20261003/report.json)。
- 实际安装版本的 Bao 隔离项目 Setup 验证 core0 / core1 / 双核选择、保存和退出通过：[界面报告](artifacts/bao-setup-95387f79e56b473cb6051fa2f09076f5/report.json)。该次安装验证未连接、复位或下载板卡；提交和远端发布在随后单独执行。
- 发布前同步现有 LaTeX 手册到 0.9.3，覆盖新配色、圆角填色、宽屏工具栏和 Project 默认焦点。内置编译器报缺少标准目录；使用已有 XeLaTeX 脚本三轮编译，最终 **93 页**，无 Overfull、缺字或未解析引用。全篇联系图及重点页面已检查，修正操作步骤跨页和旧 TERM-03 说明，自动检查无越出页面的文本。PDF SHA256：`8cb4b9ce5790cd7e86971977bd5442f4e8779ff1d2777e3b74430e8669532a3e`；证据保留在 `artifacts/release-0.9.3-doc-qa/`。

## 布局、配色与 Setup 焦点优化（2026-10-03，本地构建）

- 深蓝灰分层面板、蓝色焦点、柔和语义色与低强度核心背景色；宽屏横排工程操作、核心选择和 Symbols，共用一行调试操作栏，源码和 Inspector 页签对齐。右侧窄面板缩短 Regs / Periph 标签；保留短窗口、换行、鼠标区域和快捷键。
- 按钮及搜索框仅填充圆角边框内部；悬停、按下和输入反馈也限制在内部。覆盖普通、选中、悬停、禁用和核心身份色的边界检查，以及动画过程中外围背景不变的回归检查。
- Setup 默认聚焦 Project；上下箭头只循环可用配置项，跳过未启用的 Debug cores / ELF path prefix 和全部顶部按钮；从鼠标选中的顶部按钮按箭头回到 Project。Tab / Shift+Tab 仍可访问字段与按钮，鼠标和启动、保存、退出快捷键保留。同步 README 操作说明。
- **185 项单元测试、1 项集成测试通过，2 项外部环境测试忽略**；严格 Clippy、Release 编译和差异空白检查通过。实际 Ratatui 单元格导出的 [工作区](artifacts/ui-refined-controls-20261003/workspace.png)、[多核](artifacts/ui-refined-controls-20261003/multicore.png)、[Setup](artifacts/ui-refined-controls-20261003/setup.png) 和紧凑窗口已检查。
- Release 原生 ConPTY 回归 **8/8 通过**：[终端报告](artifacts/terminal-20261003-130030-9643da21/report.json)。[芯片选择报告](artifacts/devices-tui-f3edc5d3d91a41a384108182a75eb688/report.json) 验证 Project 默认焦点、字段箭头循环、core1 / 双核选择、保存重载及自定义芯片；[Bao Setup 报告](artifacts/bao-setup-e03bc3edb36c4bb8b5ae17dda59caffe/report.json) 验证真实工程隔离副本的 core0 / core1 / 双核选择、保存和退出。
- 首次终端回归的 Files 点击用例失败；独立 crossterm 事件探针确认该 ConPTY 宿主未交付注入的 SGR 鼠标事件。原生用例改用 Tab 切换后通过，鼠标命中与交互保留 App 测试覆盖；首次失败报告仍保留。此限制不等于真实终端中的鼠标功能已由该宿主验收。
- 构建仍为 **0.9.2**，产物为 `target/release/debugtui.exe`；本轮未升版或替换全局安装，也未连接、复位或下载板卡。

## 0.9.2 本地升版与安装（2026-10-03）

- Cargo、Cargo.lock 工程条目和 npm 包版本同步为 **0.9.2**；构建并打包圆角按钮修改，最终 181 项单元测试、1 项集成测试通过，2 项外部环境测试忽略。CLI 进程验证 12/12 通过。
- 隔离 npm 包生命周期最终 **7/7 通过**，覆盖升级、重复安装、CMD/PowerShell 入口、渲染、配置保留、卸载和重装：[分发报告](artifacts/distribution-20261003-114126-dac7d5fd/report.json)。首次验证中 Windows PowerShell 将 npm 的 stderr 警告误判为终止错误，已修正驱动为保留诊断并按真实退出码判断，保留首次失败报告。
- 已全局安装 `@debugtui/cli@0.9.2`；EXE、CMD 和 PowerShell 命令均显示 `debugtui 0.9.2`。安装目录 EXE 与 Release 构建 SHA256 一致：`a7948dc3ae81cf779958cda57c2fc3f04140d22a47eb1200b6d4757683dc637f`。安装前后 15 份 MCAL/Bao 工程 TOML 及用户芯片目录的校验值一致：[安装报告](artifacts/install-0.9.2-20261003/report.json)。
- 已安装版本的实际渲染包含新圆角按钮；原生 ConPTY 在 Bao 工程隔离副本中验证 core0/core1/双核选择、保存和退出通过：[界面报告](artifacts/bao-setup-978073d8ea0e4b40921371bc0b1c6129/report.json)。本次未连接、复位或下载芯片，未执行提交、push 或远端发布。

## 圆角按钮边框（2026-10-03）

- 将按钮的文字 `[ ]` 边界替换为与 Watch `+ Add` 共用的 Ratatui 圆角边框；覆盖工作区页签、调试操作、工程操作、核心切换、Setup 和 Watch 操作。普通窗口使用完整三行边框，短窗口使用真实左右边界；选中、悬停、禁用状态保留，点击区域包含完整边框。
- 调整换行行高和 Watch 空间分配；源码 Copy / Add to Watch 预留独立区域，文件名左对齐并为关闭按钮留出空间。更新相关布局、退出入口及边框渲染检查。
- 最终 **181 个单元测试 + 1 个集成测试通过，2 个外部环境测试忽略**；改动文件格式检查、严格 Clippy、Release 编译及差异空白检查通过。检查实际渲染单元格导出的 [工作区预览](artifacts/ui-rounded-controls-20261003/workspace.png)、[核心预览](artifacts/ui-rounded-controls-20261003/core0.png)、[Setup 预览](artifacts/ui-rounded-controls-20261003/setup.png) 及紧凑布局。
- Release 原生 ConPTY 验证芯片/核心选择、保存/重载和退出：[芯片选择报告](artifacts/devices-tui-ef3a8729bd5c4a0f99caa4aea9160ded/report.json)；Bao 工程隔离副本的 core0/core1/双核选择与保存通过，并在 Windows PowerShell 中复测了新版边框的匹配规则：[Bao Setup 报告](artifacts/bao-setup-de43b9836903427e8cb967729e2760a8/report.json)。本轮验证未连接、复位或下载芯片。
- 本地构建仍为 0.9.1，产物为 `target/release/debugtui.exe`；未安装或替换全局命令。

## 手册核对与 PDF 视觉检查（2026-10-02）

- 以本地 0.9.1 工作树、近期修改、现有测试报告及 THA6206/Bao 参考工程为依据，更新原有 `docs/usermanual.tex`，未新增重复说明文档。补齐芯片目录与核心选择、格式 3 配置与迁移、源码重映射扫描设计、Save config 与按钮状态、Bao 构建产物校验和连接/复位顺序；源文件索引核对为全部 48 个 Rust 文件。
- 历史测试保留日期、版本、固件及适用范围；明确区分 smoke 的双 GDB 调试、双 VM 调试与 core1-only 验证，保留 ConPTY Files 鼠标项 7/8 的未通过记录。本次是文档审查，未重跑实板测试、Build 或 Download。
- 内置 LaTeX 编译器仍报 `Unable to find standard directories for platform`；使用仓库现有 `docs/build-usermanual.ps1` 和本机 XeLaTeX 三轮编译，最终 PDF 为 **92 页**。最终日志无 Overfull、缺字、字体警告或未解析引用；12 条 Underfull 间距提示已结合页面图像检查。
- 全文栅格化并检查全部 92 页，修复短配置示例及测试命令跨页；最终变更页重新检查，其余页面像素哈希与已检查版本一致。重点放大复核 Setup 表格、按钮/保存语义、芯片目录和启动流程图、Bao 构建/连接步骤、源码扫描架构及测试结果表；未发现裁切、重叠或缺字。自动检查无越出页面的字符、重复标签或失效交叉引用。
- 最终 PDF SHA-256：`f8921acbebf7ee42ac0c72c8d3fbd55dbc717aa594a4e0b2bce1f1a75eb2b599`。本地证据保存在被忽略的 `artifacts/docs-review-20261002/`：`visual-review.json`、`pdf-check.json`、`source-check.json`、全页图像和编译日志；克隆仓库后可用现有构建脚本重新生成 PDF。

## 0.9.1 保存入口和按钮对比度（2026-09-29）

- Setup 删除 `Save to project` 开关，只保留顶部 `Save config / Ctrl+S`。手动保存不启动或重连调试；Start 固定在启动前保存工程配置，Exit 可放弃尚未保存的草稿。同步现有 README、LaTeX 手册与 PDF。
- Setup、项目/调试工具栏、核心切换、源码文件页签、右侧检查页签、Watch/Locals 及源码操作统一增强底色和 `[ ]` 边界。选中项使用亮底深字，保留悬停、禁用和核心身份颜色；不增加按钮行高或改变原点击区域。
- **181 个 Rust 单元测试 + 1 个集成测试通过，2 个外部环境测试忽略**；严格 Clippy、Release 构建、差异空白检查通过。现有键鼠命中测试和 45/80/120/160 列布局检查通过；实际渲染单元格的颜色/文本预览：[Setup](artifacts/ui-controls-20260929/setup.png)、[双核工作区](artifacts/ui-controls-20260929/core0.png)、[80 列 Setup](artifacts/ui-controls-20260929/setup-narrow.png)。
- 最终 Release 使用 Bao 工程隔离副本完成无参数启动、core0/core1/双核选择、Ctrl+S 保存及正常退出，验证新按钮存在、旧开关消失：[报告](artifacts/bao-setup-e289d136233f44bd8fbb88def01064ad/report.json)。夹具独立设定初始 core0，兼容用户原项目已选择双核；没有改动原项目或连接/复位/烧录板卡。
- 原生 ConPTY 通用套件 **7/8 通过**，TERM-03 的鼠标注入没有激活 Files：[新版结果](artifacts/terminal-20260929-203508-1db44c3a/report.json)。用同一夹具复测已安装旧版 0.9.0 同样为 7/8：[旧版基线](artifacts/terminal-20260929-203913-b0abe7d5/report.json)。不能将该鼠标端到端项计为通过；当前未证明是产品或夹具问题。应用内按钮/页签鼠标命中由 Rust 测试覆盖。
- 0.9.1 Release EXE SHA256：`58FA493F0AA7611509B50395E9A28361750D4CA57B365E08B298D33B6E1D175C`。内置 LaTeX 编译器无法找到标准目录；仓库已有 XeLaTeX 流程三轮编译成功，无 Overfull、缺字或未解析引用，保留少量 Underfull 排版提示。

## Bao Build/Download 故障修复（2026-09-29）

- `session-20260929-134940-542.log` 在 WSL 路径转换阶段失败，尚未启动编译器或 Download。独立 `/bin/true` 同样返回 `Wsl/Service/E_UNEXPECTED`；Windows 在 13:40、13:45、13:53 记录虚拟内存不足。确认发行版停止后执行 `wsl --shutdown`，重新启动后工具链恢复。未修改全局 WSL 内存配置或关闭用户应用。
- Bao Build 改用 `wsl --cd`，原始输出不再被 `$wslScript` 吞掉，WSL/PowerShell 使用 UTF-8；默认 `-Jobs 2`，支持选择发行版。成功时生成 ELF/BIN/HEX 校验清单，开始下一次 Build 即撤销旧清单；Download 校验三份产物并保留烧录器退出码与完整日志。`.gitattributes` 固定 WSL 构建脚本的 LF。
- 新增 `scripts/test-bao-tasks.cjs`：6 项隔离测试通过，覆盖缺失清单、空格/中文路径、ELF 不匹配、WSL 原始错误及旧清单失效、带 stderr 的原生程序成功、原生程序失败后不复位。[脚本报告](artifacts/bao-tasks-1790663763716-5b7a1cdd/report.json)。复测曾发现本次新增检查的 PowerShell 局部 `$LASTEXITCODE` 遮蔽问题，已改成明确读取原生命令写入的全局退出码，并增加成功/失败回归。
- 已安装的 DebugTUI 0.9.0 完成双核连接 → Build → Download → 自动重连 → Flash 回读 → C 源码断点/单步 → Continue/Pause，5 项通过。读回 56,596 字节逐字节一致，SHA256 为 `3f61771288e0889d461f4fa965bf1e42393942cdf84d12135aec47618c55ec72`，Guest 标记和心跳正确。[双核报告](artifacts/bao-workflow-1790663333421-4fde5892/report.json)。本轮已按用户要求切换为 Bao smoke 固件，取代下面早先“板上仍为 MCAL”的状态。
- core0 单核配置按同样流程复测，最终 5 项全部通过，包含修正退出码读取后的真实下载和系统复位。[单核最终报告](artifacts/bao-workflow-1790663775392-2c3ae114/report.json)。本轮使用 smoke 固件，未把这些结果等同于双 VM 固件或 core1-only 构建/下载验证。
- 本轮修改的是 Bao 工程脚本和验证驱动；使用现有已安装 EXE，无需更换 DebugTUI 二进制。原始项目、profile、Watch、断点由测试前后哈希保护；测试只保存隔离配置。

## Bao 工程 Chip 选择报错修复（2026-09-29）

- 错误发生于启动前配置解析：Bao 的旧 `scripts/tha6206/debug-env.toml` 没有 `backend="tha6"`。该工程未随 MCAL 一起迁移，新 Chip 选择无法匹配工具后端。此次仅修复 Bao 配置和验证驱动，DebugTUI EXE 仍使用已安装的 0.9.0。
- 工具环境补充 backend 和 core0/1 的端口/就绪信息；两个 Bao 项目改用格式 3 和每核模板。连接包含 core0 时，其他选中核先连接，再由 core0 复位一次并清 `_barrier`，只恢复未选辅助核；Run 不再逐核执行 chipreset。共享 Reset 仅由 core0 执行，core1-only 为附加模式。保留原 ELF、构建/下载命令、Watch 和断点，并复制原 AHB 策略到显式核心名称。
- 启动校验：smoke / dual 各 core0、core1、双核，共 6 种通过；不支持的 core2 和不匹配的 STM32 后端正确拒绝。ConPTY 使用真实 Bao 配置的隔离副本验证无参数启动、三种选择和保存，原报错消失：[界面报告](artifacts/bao-setup-b3da01da5aec4d5c84f70bcf72d6a104/report.json)、[配置校验](artifacts/bao-profile-fix-20260929-124227/config-validation.json)。
- 实板前置检查发现 Flash 与本地 Bao smoke/dual 不匹配，读回前 56,596 字节与 vscodegdb/THA6XXX_MC_AS440 的 MCAL HEX 对应地址全部匹配。未据 Bao 符号执行复位、写 `_barrier` 或下载，后续运行验证需用户确认切换固件。[Flash 检查](artifacts/bao-devices-1790656955274-0079b5a0/report.json)。第二次整块读取出现 TCL 超时，验证驱动已改成 4096 字节分块读取，避免单条大请求长时间无响应。
- Bao 四个修改文件的原始内容备份在 `artifacts/bao-profile-fix-20260929-124227/`；两个项目原有未提交改动已保留。新增 `test-bao-setup.ps1` / `test-bao-devices.cjs` 可重跑界面及有固件匹配门槛的实板验证。

## 0.9.0 本地芯片目录和核心选择（2026-09-29）

- 安装/首次启动初始化用户 `profiles/devices.toml`，升级不覆盖；TUI 选择芯片、勾选核心、新增客户芯片。项目格式 3 存 `[debug]`，偏好按芯片和物理核心保存；格式 1/2 的原单核、多核入口保持兼容。新增 THA 通用工程/环境模板。
- Rust：**181 个单元测试 + 1 个集成测试通过，2 个外部环境测试忽略**。包含所有指定 THA 组合、稀疏核心、非法选择、后端匹配、端口检查、并发追加不丢条目、模板筛选、偏好隔离和 TUI 键鼠命中。
- 开发版 ConPTY 新增流程通过；真实原生 GDB 覆盖 10 种组合（THA6104/6206/6412、STM32F429、客户 s32k144），检查物理核心名称、Run/Continue/Pause、Watch 持久化。此处芯片均为会话配置测试，不是这些芯片的实板验证。证据：[新增套件](artifacts/functional-1790652838798-c9b44074/report.json)。
- **THA6206 实板 core0、core1、core0+1 三种模式通过**：使用参考工程新增的 `debug-chip.toml` / `.vscode/debug-env-chip.toml`，验证 connect/run/pause/continue、每核寄存器和 Watch；允许 chipreset，无 Build/Download，原配置哈希不变。开发版和最终 Release 各完成一轮，证据：[最终 EXE 实板报告](artifacts/devices-tha6206-1790653885250-7b45250a/report.json)。core1-only 是附加模式，需要 core0 已初始化。没有 THA6104/6412、STM32 或 S32K 实板结果。
- 扩展回归：CLI、源码重映射、npm 分发、环境、旧多核、关联断点六套件通过；旧终端套件 7/8，TERM-03 仍因 ConPTY 鼠标注入未切换 Files 而失败。与 0.8.7 已记录的旧版基线现象相同，未计为通过。证据：[扩展回归](artifacts/functional-1790653230069-97470785/report.json)。
- 最终 Release EXE SHA256：`6DF9BB74EDD41EA509E5A532A1762F747C75BCC7218C707950217ADEC3E28A89`。严格 Clippy、修改文件 rustfmt、差异空白检查通过。对该 EXE 运行 CLI、新增 TUI、10 种真实 GDB 组合、npm 生命周期四套件全部通过：[最终发布版验收](artifacts/functional-1790653767481-7b669b3b/report.json)。
- 内置 LaTeX 编译器缺少标准目录；已有 XeLaTeX 脚本三轮编译成功，无 Overfull、缺字或未解析引用。更新原有 README / 手册 / PDF，未新增重复说明文档。

## 0.8.8 Setup 重映射布局与禁用状态（2026-09-28）

- Source remap、ELF path prefix 移到 SVD file 后面。No 时 prefix 整行灰显，方向键与 Tab 跳过，鼠标及旧选择状态不能打开或编辑；切回 Yes 恢复操作，已有规则保留。占位提示改为 `(Enter: choose ELF directory to map to Source root)`。
- **175 个 Rust 单元测试 + 1 个集成测试通过，2 个外部环境测试忽略**。新增回归覆盖字段顺序、灰显颜色、双向键盘导航、鼠标禁用及启用恢复。严格 Clippy、launch.rs 格式检查、Release 构建及差异空白检查通过。
- 对最终 Release 使用 Bao 真实 ELF 运行 ConPTY 流程：扫描、预览、选择、保存、重新打开、关闭和开启均通过。使用隔离配置，未启动板级服务或连接、复位、烧录板卡。证据：[报告](artifacts/source-remap-20260928-192306-272b899d/report.json)、[关闭时界面](artifacts/source-remap-20260928-192306-272b899d/project-elf/setup-disabled.screen.txt)、[启用后界面](artifacts/source-remap-20260928-192306-272b899d/project-elf/setup-enabled.screen.txt)。
- Release EXE SHA256：`E174C795D597BC564104A342A38554DB7FC79FB3FD8F46E04E284B702D6F478B`。同步 README、现有 LaTeX 手册及 PDF；内置编译器仍报告缺少标准目录，仓库现有 XeLaTeX 三轮构建成功，无缺字、未解析引用或 Overfull。

## 0.8.7 Setup 源码重映射（2026-09-28）

- 增加 Source remap 开关、ELF path prefix 目录选择、父/子层级导航及本地文件匹配预览。后台复用配置的 GDB 读取元数据，不执行 profile 参数、初始化、服务或目标动作。关闭保留规则，重新启动调试后 GDB 和 Source 同步停用；各核共享映射。
- 最终 Release EXE SHA256：`2985D8D821DBB5B84FED281D6399CC4F9E446013BED73E06A1659BE6A2DAEB32`。**174 个 Rust 单元测试 + 1 个集成测试通过，2 个外部环境测试忽略**；严格 Clippy、修改文件的 rustfmt、Release 构建及 `git diff --check` 通过。没有将已有 `src/svd.rs` 的全仓格式差异算作通过。
- 对此 Release 执行 CLI 12 项，以及 `source-remap` 专项：4 种构建机路径（POSIX、Windows 反斜杠、混用、空格）的真实 ConPTY 配置流程、本地 GDB 源码读取与 file:line 断点解析；POSIX 样例同时验证 core.0/core.1 的映射继承。取消、超时、无源码记录、GDB 错误、扫描中退出 5 个场景均回收扫描进程。两套件通过，其余 16 个功能套件本次未运行；不表述为全部功能或实板回归。
- THA6206 GHS ELF：识别 563 个源码条目，工程根下 **559/559** 文件匹配。Bao ELF：排除编译器虚拟文件后识别 104 个条目，`/mnt/d/.../bao-hypervisor` 映射至本地根后 **101/101** 文件匹配。两者均验证扫描、预览、保存、重新打开、关闭和开启；使用隔离配置，没有连接板卡、改动原工程配置、编译或烧录固件。
- 终端通用测试为 7/8；TERM-03 的鼠标注入未激活 Files。对已安装旧版 0.8.6 使用同一夹具复测亦为 7/8，因此没有将其计为新功能验收通过。新目录列表、Apply/Cancel 按钮的鼠标命中和父/子操作由 Rust 测试覆盖，ConPTY 专项使用键盘。修正夹具按块读取 UTF-8 输出，避免 StreamReader 等待填满字符缓冲而漏掉静止界面尾部。
- UNC 路径结构及别名由单元测试覆盖；不可达网络主机会使部分 GDB 的路径解析超时，即便关闭源码读取也可能发生。此时显示错误并回收 GDB，可用原有手工 source_map 绕过扫描；没有将该网络情况记为端到端通过。
- README、现有 LaTeX 手册及 PDF 已更新。内置编译器报告缺少标准目录；使用仓库现有 XeLaTeX 流程三轮构建成功，无缺字、未解析引用或 Overfull。Windows 一次临时 PDF 占用失败日志保留在 `artifacts/release-0.8.7/document-build-retry/`，重新构建成功后清理了 docs 中该次临时目录。

证据：[最终套件汇总](artifacts/functional-1790593099227-bd938aaf/report.json)、[源码映射专项](artifacts/source-remap-20260928-185832-fc7b30da/report.json)、[双核映射](artifacts/source-remap-20260928-185832-fc7b30da/posix/multicore-result.json)、[THA6206 GHS](artifacts/source-remap-20260928-185818-5ad1d7ee/report.json)、[Bao](artifacts/source-remap-20260928-185819-c190171a/report.json)、[旧版终端基线](artifacts/terminal-20260928-185050-ab1b4222/report.json)。产物保存在本地被忽略的 artifacts，克隆仓库后可用 scripts 中的驱动重跑。

## 功能验收扩展（2026-09-27）

- 新增 45 项带稳定 ID 的用例：CLI 12、真实 Windows ConPTY 终端 8、隔离 npm 安装/升级/卸载/重装 7、STM32F429 FreeRTOS 工程实板 18。用例保存在本仓库 `scripts/` 和 `tests/`，运行产物保存到被忽略的 `artifacts/`。
- 新增 `scripts/test-functional.cjs` 统一入口，默认 17 个套件，开启 STM32 实板时 20 个；`tests/functional-coverage.json` 将实现和验收证据映射到 22 个功能组。运行条件、命令、用例范围和限制见 [tests/README.md](tests/README.md)。
- 直接对本机已安装的 **0.8.6 release** 验收：EXE SHA256 为 `A3C9F1609B19C5081D3118FD68F1542027730B9F500098C414BABAA15FB488E9`。最终 **20 个套件通过，0 失败**，22 个功能组均取得所列自动化证据；新增用例 **44 通过，0 失败，1 跳过**。跳过的是真正烧录 HW-16；复位/Run HW-15 已在匹配固件上执行。
- 当前源码的 Rust 测试仍为 **168 个单元测试 + 1 个集成测试通过，2 个忽略**（Windows 原生剪贴板、THA6206 Source-to-Watch 实板）；新增用例属于进程/终端/安装/实板验收，不冒充新增 Rust 单元测试。退出策略的既有 26 个场景及全部本机 GDB 专项也由统一入口复跑通过。
- 使用 `G:/Data/GitFiles/Keil/STM32_CubeIDE/FreeRTOS_Project` 的既有 ELF、CMSIS-DAP 和 OpenOCD。只读段与 ELF 一致，源码/指令单步、Watch 树、GDB/AHB/暂停核心读取一致、运行时零 MI 轮询、复位至 main、重连/进程重启、Write/Read/Access 数据观察点均通过。原工程 `debug.toml`、profile、SVD、ELF 的保护哈希不变；自有 DebugTUI/GDB/OpenOCD 退出检查通过。没有重新编译或烧录用户固件。
- 终端夹具补齐 ConPTY 输出的擦除字符指令，并将 Files 用例限定为实际验证的输入/清空行为，避免把旧 Source 文字当作筛选结果。PowerShell 7 避免 Windows PowerShell 5 将预期 stderr 负例转换为终止错误；清理检查使用进程列表，避免将已退出 PID 的 ObjectNotFound 错误码误判为进程残留。前期驱动失败日志保留，最终结果以本节链接为准。
- 这不是代码行/分支覆盖率或全硬件认证。系统剪贴板、物理拔插/断电、多小时稳定性、其他宿主/探针、多核实板及公共 Release 下载/历史正式版升级仍单独验收；本轮多核是多个真实本机 GDB，安装升级使用隔离旧版本号夹具，不替代上述验证。

最终汇总：[report.md](artifacts/functional-1790516421615-820f090c/report.md)、[report.json](artifacts/functional-1790516421615-820f090c/report.json)。实板逐项结果：[report.json](artifacts/project-hardware-1790516539340-6c5ac2ff/report.json)。本轮只增加测试和说明，没有修改 DebugTUI 的产品实现、版本号或全局安装。

## 0.8.6 发布前验证（2026-09-23）

- 168 项 Rust 单元测试和 1 项 CLI 集成测试通过，2 项显式外部环境测试默认忽略；严格 Clippy `--locked --all-targets -- -D warnings`、Release 构建和 `git diff --check` 通过。
- 新 Release EXE SHA256 为 `A3C9F1609B19C5081D3118FD68F1542027730B9F500098C414BABAA15FB488E9`，与下述单核/多核实板回归使用的已安装 0.8.6 完全相同，无需将其他构建的实板结果冒用为本次结果。
- `test-exit-policy.cjs` 的 26 个场景通过，覆盖 remote/extended-remote/local、各退出策略、运行/暂停/READY，以及 continue/release/exit 失败、GDB hang/crash。`test-search-gdb.cjs` 的真实本机 GDB 和 MCAL ARM ELF 两组测试通过，确认符号位置、有界查询、不改变目标执行状态。
- README 和用户手册同步为 0.8.6，更新工程专用 Setup、当前 Examples、Files/Symbols 搜索、工具/工程配置职责以及本轮 MCAL 实板结果和残留限制。XeLaTeX 三轮编译无缺字和未解析引用；PDF 共 83 页。
- 日志：`artifacts/release-0.8.6/`。全仓 `cargo fmt --check` 仍报告 HEAD 已有的 `src/svd.rs` 排版差异；该文件未被本次修改，不计为格式检查通过。

## 0.8.6：THA6206 单核/多核实板回归（2026-09-23）

- 直接测试已全局安装的 0.8.6，EXE SHA256：`A3C9F1609B19C5081D3118FD68F1542027730B9F500098C414BABAA15FB488E9`。工程为 `D:/LiBo/Files/DATA/FreeRTOS/vscodegdb/THA6XXX_MC_AS440`，使用实际 GHS ELF、ARM GDB、OpenOCD 和 THA6206；UI 通过真实 Windows ConPTY 输入/读取验证。
- **16 个功能组通过**，包括 core0/core1 独立调试、默认单 GDB 模式、双核 Scope/联停/复位、条件和多核断点、逐核 8 个硬件代码断点及第 9 个拒绝后恢复、运行中 Live Watch、Files/Symbols 跳转、源码映射、偏好保存、任务失败恢复和外部附加。退出/重连另有 **21 个实板组合场景**全部通过。两核联停由软件协调，不是 CTI 同时停机；本轮 peer STOPPED 日志延迟样本为 124–266 ms。
- 初始板上 Flash 与本地 ELF 不匹配，已通过工程 Load.bat 下载现有配套 MCAL HEX，随后及最终分别校验向量/代码段匹配。本次实际烧写了固件，但没有重新编译应用、修改用户源码或替换已安装软件；构建成功生命周期使用 echo，另行验证真实下载流程及非零返回码恢复。
- 修正两类工程工具配置问题并复测：默认环境补充板级 Run 动作，避免 `-exec-run` 触发不支持的通用 reset；默认和外部 core1 环境补充 17 个通用寄存器白名单，避免复位入口自动读 VFP 引发 DSCR.ERR。只改工程 `.vscode/debug-env.toml` 和 `.vscode/debug-env-core1.toml`，附原因注释；19 个受监测文件中的其余 17 个哈希未变，四份工程 TOML 保持原样。
- **残留问题保留记录**：首次调试复位仍可报 `Failed to write memory at 0x00000000`，后续 examine/Run/Flash 校验成功但不证明复位写入确认；GHS 深层栈存在 `pc 0x80`、未知帧及栈回溯时的调试访问错误。当前帧/断点/单步测试通过，不将其解释为完整调用栈可靠或日志零错误。
- 最终两核暂停在 `0x08000000`，按 detach 结束，自有 DebugTUI/GDB/OpenOCD 和 3333/3334/6666/4444 监听清理。未覆盖物理断电/拔插、多小时压力、浮点调试和其他固件；未提交、push、升版或安装。

详细结果、配置修正、原始失败分类及验证边界：[`artifacts/regression-0.8.6-20260923/report.md`](artifacts/regression-0.8.6-20260923/report.md)。机器可读汇总：[`report.json`](artifacts/regression-0.8.6-20260923/report.json)。

## 0.8.5 工作区：Setup 仅编辑工程配置（2026-09-23）

- 移除 GDB executable、GDB arguments、Target mode、Endpoint、Start service、Timeout 六个工具编辑项。Tools / profile 仅选择环境引用；On exit、ELF、源码、任务、SVD、日志及保存开关保留。标题提示当前核心名称。F4 模板不再生成内嵌工具参数；旧配置覆盖值继续兼容。
- 168 项单元测试、1 项 CLI 集成测试通过，2 项外部环境测试仍忽略；严格 Clippy 和 Release 构建通过。新增覆盖普通单核、core0 单核、core1 单核、双核编辑/保存、切换 profile、逐核运行时偏好合并、工具文件不改写、遗留工具值保留和工具字段从界面移除。
- 新 EXE 的真实 Windows ConPTY 验证：MCAL debug-core0/debug-core1/debug-multi，以及 Bao debug/debug-dual 共 5 份工程副本，点击 On exit 后 Ctrl+S 保存，只改变工程退出策略，其余配置完全一致。原始工程、工具 profile、已安装 EXE 的哈希均未变化。 Bao 的 debug-dual.toml 当前没有 [[cores]]，属于双核固件、单个 GDB 会话的配置；此次保持该连接模式，TUI 多会话覆盖由 MCAL debug-multi 和独立双 GDB 用例验证。截图和结果：`artifacts/project-setup-20260923/setup-*.png`、`native-setup-results.json`。
- `node scripts/test-exit-policy.cjs target/release/debugtui.exe` 的 26 组退出流程回归通过。另用隔离 profile 验证普通单核、core0、core1、双核 × detach/resume/disconnect，共 12 组 MI 回归通过；Scope=core 时退出仍释放所有配置核心。证据：同目录 `profile-core-exit-results.json`、`*.mi.txt`。最初测试暴露严格 fixture 缺少多核连接后的 register-cache flush 响应，已补充该精确命令；终端脚本也改为等待完整表单绘制，避免读到半帧。这些是测试驱动问题，没有修改实际退出执行逻辑。
- 本轮未连接板卡，未安装、未升版。本地 Release SHA256：`EDC7A875ABDF09DCB8DDB5957E410CBFE1390F432C73B1B0DCFB77AD61ED5515`；已安装 0.8.5 仍为 `0DB5B82AEFBD1FE1A242CDC7EAA045346D518AAA075F1153953D7CE6550B3E5C`。详细日志位于 `artifacts/project-setup-20260923/`。

## 0.8.5 工作区：工程与工具配置职责说明（2026-09-23）

- 修正 README 和 Setup 帮助：工程配置保存程序、构建/下载、源码映射、Watch/断点、核心选择及退出策略；工具环境保存 GDB/OpenOCD、服务启动、连接默认值及通信超时。现有 `[session].timeout_ms` 是工具通信超时；`on_exit` 属于工程退出策略。明确区分解析器的继承兼容能力与建议的字段归属。
- 本轮只修改说明，未更改解析、优先级或保存行为，未迁移 MCAL/Bao 配置。Setup 修改工具项仍会写项目覆盖；README 已明确此限制。
- 15 项启动配置测试通过，Release 构建通过，未安装。日志：`artifacts/config-layering-20260923-{tests,build}.txt`。构建产物 SHA256：`F862820B93A1E40B967A7A41C3500499D9BE7AEB94B8379CDC559D957FDA120C`。

## 0.8.5 工作区：输入区可见性与配置说明（2026-09-23）

- Symbols、Files 和 Watch 使用独立背景及完整边框，聚焦时强化边框/底色，Watch 的 Add 按钮有独立边界；高度不足时保留紧凑单行。Watch 面板为边框预留空间，保留原有可见变量容量。Setup 的 profile 说明明确继承值与项目覆盖，并修正远程 resume 的过时 detach 描述。
- 165 项单元测试、1 项 CLI 集成测试和严格 Clippy 通过，2 项外部环境测试仍忽略；Release 编译通过。既有布局、点击区域、Watch 删除/添加及自动补全测试覆盖 45×12 至 180×50。日志和实际 Ratatui 颜色预览：`artifacts/input-visibility-20260923/`。
- 新 Release EXE 在真实 ConPTY 中加载 Bao 的 `bin/tha6xxx/tha6206-smoke/bao.elf`：点击 Symbols 边框，输入 `uartclock` 并跳到 `tha6xxx_uart.c:12`；点击 Files 边框输入 `boot` 并打开 `boot.S`；点击 Watch 边框输入表达式、核对光标位于框内、清空并点击 Add。三组通过。使用隔离 TOML 和 local 模式，仅加载 ELF；没有连接探针或运行目标。验证时仅对测试子进程移除工具环境中的 `NO_COLOR=1`，以核对正常彩色终端效果。
- 本轮**未安装、未升版**。新 `target/release/debugtui.exe` 的 SHA256 为 `0D482825B17D0636871C817F681F929E27B2A6286DD76D2636F8E5A43FF8B54C`；已安装 0.8.5 保持 `0DB5B82AEFBD1FE1A242CDC7EAA045346D518AAA075F1153953D7CE6550B3E5C`。完整终端结果和画面：`terminal-result.json`、`files-focused.png`、`symbols-focused.png`、`watch-focused.png`。

## 0.8.5：已安装版本 Files / Symbols 搜索验收（2026-09-23）

确认全局 `debugtui --version` 为 **0.8.5**，实际 EXE 为 `C:/Users/01766/AppData/Roaming/npm/node_modules/@debugtui/cli/bin/debugtui.exe`，SHA256 与本地 Release 一致：`0DB5B82AEFBD1FE1A242CDC7EAA045346D518AAA075F1153953D7CE6550B3E5C`。本轮直接运行该已安装 EXE，没有重新编译或安装。

- **16 个功能组通过**。基于 `D:/LiBo/Files/DATA/FreeRTOS/vscodegdb/THA6XXX_MC_AS440` 的实际 GHS ELF、工程 ARM GDB 16.3，通过 Windows ConPTY 输入真实鼠标/键盘事件并读取终端显示；双核部分实际连接 THA6206 和工程 OpenOCD，非 mock / demo，也不是人工 VS Code 鼠标验收。
- **Files**：从 ELF 读取 563 个源文件；输入 `sPi` 同时显示 `Spi.c`、`Spi_Irq.c`、`espi_hal.c`、`espi_std.c`。筛选后键盘分别打开三个文件、鼠标打开 `espi_hal.c`，核对 Source 标签；无匹配时 Enter 不误打开，Ctrl+U 恢复 563/563 列表。
- **Symbols**：键盘查询 `Spi_Init` 并定位 `Spi.c:4073`；鼠标查询 `uartcnt` 模糊匹配 `uart_cnt` 并定位 `Uart_Demo.c:1519`；类型 `Spi_ConfigType` 定位 `Spi_GeneralTypes.h:591`；静态变量 `SpiExternalDevice_ConfigParamCore0` 定位 `Spi_PBcfg.c:481`。逐项核对真实源码定义及 Source 选中行。无匹配、正则字符按字面处理、200 项/类别上限提示通过。
- **映射与布局**：使用隔离源码副本及 `source_map`，跳转后实际显示映射副本标记且仍为第 1519 行。80×24、45×12 的独立终端均显示结果文件/行号并正确跳转。
- **双核实板**：Core0 / Core1 分别查询并跳转 `Spi_Init`，各核日志均记录实际符号查询；跳转前后两核 PC、当前栈帧标题保持不变。两核运行时新查询显示等待提示，OpenOCD 同时确认 `running running`；显式 F6 后查询完成并跳转第 1519 行，两核为 `halted halted`。已有查询结果可在两核继续运行时用于源码跳转，不隐式停核。
- 测试使用隔离 TOML；已安装 EXE、ELF/HEX、四份工程 TOML、profile 和 OpenOCD 配置共 9 个文件前后哈希一致，没有编译/烧写固件。结束后核对调试进程和 3333/3334/6666 监听清理。早期测试驱动的路径分隔符、空结果提示文案和连接完成等待条件已纠正；原始失败记录保留，最终双核结果以 `hardware-results.json` 为准，未发现此次搜索功能缺陷。

汇总及证据索引：[`artifacts/search-installed-0.8.5-20260923/report.json`](artifacts/search-installed-0.8.5-20260923/report.json)。终端画面、输入序列和 GDB 日志位于同目录；独立 ELF 元数据验证为 `artifacts/search-arm-elf-1790135230928/result.json`。

## 0.8.5：THA6206 退出修复与实板复测（2026-09-23）

修复 0.8.4 的 `session.on_exit="resume"`：远程 all-stop 会话改为清理断点后 `-exec-continue` → `-target-disconnect`，不再对运行中的目标发送被 GDB 拒绝的 detach。本地进程仍在暂停状态下 detach。`-gdb-exit` 的错误、非零退出码和退出超时会报告失败；收到 `^exit` 后最多等待 3 秒，让 GDB 正常清理，再使用强制回收作为兜底。

- **已安装 0.8.5 并验证全局命令**；全局 EXE 与被测 Release EXE 的 SHA256 均为 `0DB5B82AEFBD1FE1A242CDC7EAA045346D518AAA075F1153953D7CE6550B3E5C`。已有工作区内的 Source 文本操作、启动配置及 Files/Symbols 搜索改动也包含在本地构建中；本轮未提交、push 或发布远端 Release。
- **安装后 21/21 实板退出与重连场景通过**：Core0、Core1、双核 × detach/resume/disconnect × STOPPED/RUNNING 的 18 项，以及三种核心配置的 `remote` 模式 resume 3 项。用独立 OpenOCD 在断开后观察实际核状态、AHB 计数器，再重连验证源码行、Watch 和禁用断点恢复。单核 resume 仅对应核运行，双核 resume 两核运行；detach/disconnect 在此工程保持暂停。双核 resume 的计数器示例为 1270→1372、1952→2054。不能将这些目标状态推定到其他 Server。
- **常规实板回归通过**：Core0 的源码/寄存器/栈、单步、条件/忽略次数/临时硬件断点、Watch 结构体/数组/指针、三类硬件数据断点、内存和外设读数、运行时 AHB、重连/复位；Core1 独立调试时 Core0 计数器仍增长；双核控制 27 项检查、跨核断点 61 次请求、两核运行时 Live Watch 5 组采样。Flash 向量与代码段再次 `compare-sections` matched。旧回归脚本的 `number="all"` 和 `mode` 参数错误已在隔离测试副本中改为 `all=true`、`access`，断点专项复测通过，不能把该脚本错误计为产品缺陷。
- **真实 ConPTY 界面 4/4 组通过**：源码双击选择 `uart_cnt`、Ctrl+C 系统剪贴板复制、Add to Watch；两核运行时右侧 Watch 实际显示 LIVE 且数值 1165→1246→1407→1488→1650；Pause/F2/Esc 保留会话；运行中 Ctrl+Q 按 resume 正常结束自有调试进程。测试等待复制完成的界面提示后读取剪贴板，结束后恢复原剪贴板。
- **自动化与构建**：165 项单元测试、1 项 CLI 集成测试通过，2 项依赖外部环境的单元测试保持忽略；严格 Clippy、Release 构建通过。新增 `node scripts/test-exit-policy.cjs` 的 26 项生命周期测试通过，覆盖本地/远程、未 Run、停止/运行状态、真实错误传播、延迟退出、退出挂起/异常码；严格 fixture 在旧 0.8.4 上复现原来的运行中 detach 错误。Pause 的 6 种事件/超时场景、本机多核 GDB 13 组、Files/Symbols 的本机与 THA ELF 离线查询也通过。完整 `cargo fmt --check` 仍指出原有 `src/svd.rs` 格式差异，本轮未改动该文件。
- **本地安装包**：生成 0.8.5 EXE/ZIP/TGZ，验证校验和、无捆绑工具、隔离 npm 升级/卸载/重装、PowerShell/CMD 入口和渲染。TeX 用户手册已同步退出说明并成功重新生成 PDF。

记录集中于 [`artifacts/exit-policy-fix-20260923/`](artifacts/exit-policy-fix-20260923/)；安装后矩阵为 `installed/matrix.json`，常规回归为 `functional/`，最终终端记录为 `terminal-hardware-1790133832/`。本轮使用 `D:/LiBo/Files/DATA/FreeRTOS/vscodegdb/THA6XXX_MC_AS440` 的既有 GHS ELF/HEX，没有重新烧写固件；Build/Download 的新增复测只验证连接生命周期，使用输出标记的测试命令，真实编译/烧写结果见下方 0.8.4 完整回归记录。未在本轮重测 STM32 或 Bao 实板。

额外发现的范围外限制：本机 MinGW GDB 16.2 对 Windows 测试进程执行 detach 后再退出 GDB，进程的文件心跳停止；直接运行同一 GDB/MI、完全绕过 DebugTUI 也复现（`native_direct.py` / `native/direct-mi.log`）。因此这里只确认本地 detach 命令被接受，不宣称该后端的进程在 GDB 退出后持续运行；没有用假通过掩盖此现象。该本机进程问题不影响上述 ARM GDB 16.3 + OpenOCD 的 THA6206 实板结果。

## 0.8.4：Files 与 Symbols 模糊搜索（2026-09-22 验证，09-23 收尾）

- 最终代码通过 **165 项单元测试、1 项 CLI 集成测试**；2 项依赖实板或系统剪贴板的测试保持忽略。严格 Clippy 与 Release 编译通过。日志：`artifacts/search-ux-20260922/{unit-tests,clippy,build}.txt`。本次只编译，未安装或替换全局 EXE。
- UI 回归覆盖 `spi` 对四类文件名的大小写及模糊匹配、筛选后键盘/鼠标打开正确文件、空结果、清空与粘贴焦点隔离；符号查询的防抖、单个在途请求、输入变化/切核后的旧响应丢弃、运行状态门控、错误重试，以及 CI 路径映射后跳转行号且保持当前栈帧不变。45×12、80×24、120×36 的实际 Ratatui 文本预览位于同一目录，长路径保留文件名和行号。
- 真实本机 GDB 离线查询通过：函数、全局/静态变量、类型、模糊字符顺序、正则特殊字符按字面输入、无匹配和结果上限。`Spi_Init` 与 `SpiCounter` 分别返回测试源码第 6、3 行；25 条查询命令均为符号/文件元数据。记录：`artifacts/search-native-1790070992527/result.json`。
- 工程内 ARM GDB 加载 GHS 生成的 `THA6206_Demo_Prj.elf` 离线查询通过：`Spi_AsyncCheckJobLinkStatus` 定位到 `Spi.c:1882`，`SpiExternalDevice_ConfigParamCore0` 定位到 `Spi_PBcfg.c:481`，已核对本地定义行。12 条查询命令均为元数据查询。记录：`artifacts/search-arm-elf-1790070991205/result.json`。两组测试始终为 READY，没有连接探针或执行目标程序；这不是 THA6206 单核/多核实板运行验证。
- 可用 `node scripts/test-search-gdb.cjs target/release/debugtui.exe` 重跑本机验证；设置 `DEBUGTUI_TEST_GDB` 为工程 GDB 路径，并在 EXE 参数后追加 ELF 路径，可重跑对应 ELF 的离线验证。上述真实 GDB 验证后仅修改搜索 UI 的长路径显示及粘贴焦点保护，最终代码已重新执行全部单元测试、Clippy 和构建。
- 最终产物：`target/release/debugtui.exe`，版本 `0.8.4`，3,658,240 字节，SHA256：`97BF95AA283355A381229C4016BFD8D4A782A06F757389AE21BC9E7A42CA413B`。

## 0.8.4：未通过项的专项复测与原因（2026-09-22）

上轮的唯一失败功能组是 **`session.on_exit="resume"`：结束调试前恢复目标运行**。针对同一个已安装 0.8.4 EXE 重新执行了 16 个专项场景：9 个对照/恢复场景通过，7 个 resume 场景稳定复现失败。这里的场景数是对一个失败功能组的展开，不是新增 7 类缺陷。记录：[`artifacts/exit-policy-retest-20260922/report.json`](artifacts/exit-policy-retest-20260922/report.json)。

| 策略与场景 | 本轮复测 | 退出后的实际观测 |
| --- | --- | --- |
| `detach`：Core0 / Core1 / 双核，各自从 STOPPED 和 RUNNING 退出 | 6/6 通过 | DISCONNECTED；被调试核暂停。本适配中默认退出不会自动保持运行。 |
| `resume`：Core0 / Core1 / 双核，各自从 STOPPED 和 RUNNING 退出 | 6/6 失败 | 返回 FAULT，但相应核实际仍在运行；Core0 或双核运行时 AHB 计数器持续增长。 |
| `disconnect`：Core0 / 双核，从 RUNNING 退出 | 2/2 通过 | DISCONNECTED；退出前暂停。 |
| DebugTUI 自己启动 OpenOCD 的原始使用方式，Core0 `resume` | 再次失败 | 同一 GDB 错误，排除独立观察用 OpenOCD 的服务归属差异。 |
| 失败后重新连接，读取 Watch，再用默认 `detach` 退出 | 通过 | 能恢复连接和读取变量，正常清理退出。 |

原因定位到 `src/session.rs` 的 `Engine::disconnect`：它先完成暂停/断点清理，再在 resume 分支发送 `-exec-continue`，收到 GDB `^running` 后仍发送 `-target-detach`。当前 GDB 16.3 在这个运行状态拒绝 detach：`Cannot execute this command while the target is running`。随后 `-gdb-exit` 关闭 GDB，DebugTUI 把前述错误汇总为 FAULT。应用内的 DISCONNECTING 标签不会改变 GDB/硬件的实际运行状态。

本轮保留一个由测试持有的独立 OpenOCD，在每次断开后检查两个核的真实状态及 AHB 数据：单核 resume 只留下相应核运行，双核 resume 留下两个核运行。例如双核暂停状态退出后，`uart_cnt` 为 872→977；从运行状态退出后为 1223→1325。故此次失败是退出顺序和结果处理错误，Continue 本身已成功。不能仅凭 FAULT 标签判断板卡已停止。

当前工程三个用户配置均从 `.vscode/debug-env.toml` 继承 `on_exit="detach"`，失败策略只在隔离测试副本中启用。后续修复需要为后端实现合适的脱离/恢复顺序，并验证退出后硬件状态；不能通过忽略 detach 错误伪造通过。现有 `tests/mock-gdb.cjs` 对 detach/disconnect 不区分运行状态而直接返回 done，缺少此次实板拒绝条件，应补相应回归。

本轮没有修改退出实现或升版；没有重新编译/烧录固件。已安装 EXE、ELF/HEX、三个用户配置的前后哈希一致；测试结束无调试进程和 3333/3334/6666 监听残留。

## 0.8.4：已安装版本 THA6206 实板功能回归（2026-09-22）

**结论：27 个功能组通过，1 个退出策略缺陷复现，不能判定为全部通过。** 本次运行的是 npm 全局安装的 `debugtui.exe`，版本 0.8.4，SHA256 为 `D879688D0112AA554E765A6C987903618F3DA0410E2AE47BD753F3380C47FBBD`。完整矩阵、产物哈希和证据索引：[`artifacts/regression-0.8.4-20260922/report.json`](artifacts/regression-0.8.4-20260922/report.json)。测试期间工作区其他未构建改动不属于此固定 EXE 的回归范围。

实际工程目录为 `D:/LiBo/Files/DATA/FreeRTOS/vscodegdb/THA6XXX_MC_AS440`；GHS 2023.1.4、GDB 16.3、工程定制 OpenOCD、CMSIS-DAP SWD 5 MHz、THA6206 双 Cortex-R52。使用三个工程配置的隔离副本，用户 TOML 配置哈希保持不变。

| 范围 | 实板结果与证据 |
| --- | --- |
| Build / Download / 符号 | 实际 GHS Build、厂商 HEX 烧录及系统复位通过；DebugTUI 下载后自动重连两核，Run 命中 `_main`。`ROM.INT_VECTOR_TABLE` 和 `EX_CODE` 的 GDB `compare-sections` 均 matched。连接状态下 Build 也恢复双核符号、Watch 和禁用断点。 |
| Core0 / Core1 单独调试 | Core0 启动、Step/Next/Finish/StepI、断点及变量读取通过。Core1 在 Core0 初始化并运行后，重复命中 `DemoApp_MainCore1`，完成各类单步；Core1 暂停期间 Core0 的 AHB 计数器继续递增。未 examine 的 Core0 状态不能直接当作 STOPPED，验证使用计数器实际进展。 |
| 多核 | 27 项组控制核对通过：All/Core 的 Continue/Pause、断点联停、命中核自动切换、单步保持另一核 PC、重复 Run、共享复位和重连。源码断点默认当前核；升级/降级多核、条件/忽略次数同步、删除隔离和重连恢复通过。 |
| 断点 / 数据观察点 | 条件、忽略次数、批量启停、临时和硬件断点通过。Write、Read、Read-write 三类硬件观察点分别核对 GDB 类型和实际停止原因，不把三次 Write 当作三类覆盖。 |
| 数据和视图 | Watch 表达式、结构体、指针、32→64 项数组分页、错误行和移除通过；源码列表、栈帧、Locals、17 个系统寄存器、反汇编、Memory、GDB/AHB 读取及运行状态门控通过。SVD 的 BASETIMER0 展开显示实际 LOAD/VALUE/CTRL 等值。 |
| 运行中 Watch | 旧 `live_watch` 的五个双核采样窗口通过；切核、删除再添加、停止刷新和复位后采样正常。已安装 EXE 的可见 Watch 数值连续变化；界面设置 AHB 100 ms 后，十六进制 LIVE 样本为 52079、52347、52618、52953、53212（此处转为十进制列示），采样时两核均保持运行。 |
| 新增 Source 文本操作 | Windows ConPTY 驱动已安装 EXE，验证双击/拖选、键盘整行复制、右键 Copy、实际 Windows Unicode 剪贴板读回、Add to Watch、拒绝跨行表达式、F9 当前核断点命中。Core0/Core1 分别添加 `uart_cnt`，退出保存后各核恰有一项。原剪贴板已恢复。 |
| 配置和终端交互 | Setup 修改 timeout 为 9000、Ctrl+S 保存、F5 启动并命中 `_main`、F2/Esc 保持会话；Files/Asm/Memory 实际内容、Help、Appearance 保存、文件列表、80×24 / 45×12 / 180×50 布局及终端 F5/F6 通过。属于伪终端自动化，不是人工 VS Code 鼠标验收。 |
| 源码映射 | ELF 中的绝对编译目录映射到另一个含空格的本地目录，`main.c:105` 实际断点命中；ELF、profile、source_root 和映射目标的相对配置解析通过。连接时拒绝直接换 ELF，断开后重载通过。 |
| 负例和清理 | Build 返回 17、Download 返回 23 时不自动重连，可手动恢复；构建中 Quit 取消并正常退出。测试结束无 DebugTUI/GDB/OpenOCD/jtag 残留进程，3333/3334/6666 无监听。 |
| **未通过：退出后运行** | **`session.on_exit="resume"` 两次复现错误。** 当前实现先 `-exec-continue`，随后 `-target-detach` 被 GDB 拒绝：`Cannot execute this command while the target is running`；断开响应失败、状态为 FAULT。默认 `detach` 路径通过。证据：`exit-resume-repro/result.json` 和 `core1-prepare-core0/requests.json`；本次没有修改该实现。 |

初始实板存在 Flash 读取/启动失败，通过本次实际 Build、厂商 Download 和系统复位恢复。下载前 Flash 不可读，不能据此断言旧固件不匹配；下载后代码段校验通过。17:21 左右的 IDR/APB-AP 故障，用户确认同期有拔插、断电或复位操作；通过 Reconnect 恢复后继续测试通过，将其记录为中断恢复场景。

本轮使用 GHS MCAL 固件，未覆盖 Bao 固件、其他平台实板、冷上电时序、长时压力、CTI 硬件同时暂停和断点资源耗尽。Source 内编辑及外部编辑器自动构建下载链尚非本阶段实现内容。旧测试驱动的参数、坐标/时序和返回结构断言错误已纠正；原失败尝试及后续证据保留在回归目录，最终取值规则写入 `report.json`。

## 0.8.4：Source 选择、复制和加入 Watch（2026-09-22）

- 147 项单元测试、1 项任务集成测试与严格 Clippy 检查通过。覆盖拖选、双击选词、跨行/反向选择、Tab/中文、键盘扩选、右键菜单、窄终端、横向滚动、快捷键冲突和核心切换清理选区；行号栏及 F9 保留断点行为。
- Windows 原生剪贴板测试通过，实际读回中文、Tab 和 CRLF；测试后恢复原剪贴板。普通 UI 测试使用内存剪贴板，不修改用户剪贴板。
- 基于本机 `THA6XXX_MC_AS440` 工程的 `debug-core0.toml`、`debug-core1.toml` 和 `debug-multi.toml`，使用实际 Source/UI 事件处理与 Ratatui TestBackend，连接工程内的 GDB / OpenOCD 和 THA6206 实板：从实际 `Uart_Demo.c` 选择 `uart_cnt`，复制并加入当前核 Watch，通过值读取和每核列表隔离断言。测试禁用配置持久化，核对源码与 ELF 字节未改变，没有烧录固件。这是自动化 UI 到实板测试，不是终端鼠标手工验收。
- 额外 Reset/Run 验证未在 8 秒内命中 `_main`；OpenOCD 有 `DSCR.ERR`，GDB 校验 `EX_CODE` 时出现 target memory fault。完整固件启动流程未验证通过，不能将 Source 功能测试结果视为全部调试场景正常。证据与测试范围：`artifacts/source-selection-20260922/result.json`。

## 0.8.3：正式发布验证（2026-09-20）

- 正式版本重新执行 `cargo test --locked -- --test-threads=1`：137 个单元测试和 1 个 CLI 集成测试通过；`cargo clippy --locked --all-targets -- -D warnings` 与 release 构建通过。记录：`artifacts/release-0.8.3/`。
- 使用正式 release EXE 重跑三个真实 GDB 的多核断点测试，99 个请求全部通过，涵盖核心关联、持久化、失败回滚和混合运行状态。记录：`artifacts/linked-breakpoints-1789906062625/verification.json`。
- 本版合入下面三个本地修复版本；THA6206 实板、实时 Watch 渲染和单核/多核回归的范围与证据见对应记录。

## 0.8.3-local.3：多核断点（2026-09-20，本地修复版）

- 137 个 Rust 单元测试、1 个 CLI 集成测试、严格 Clippy 检查通过。新增默认单核请求、非法核心/运行中拒绝、相同位置独立断点、不同 GDB 编号关联、回滚队列、鼠标选核/键盘应用、错误保留及 45×12 至 160×45 布局验证。
- `scripts/test-linked-breakpoints-gdb.cjs` 使用三个真实 GDB，99 个请求通过：部分核心关联、编辑/启停/批量操作、重连/进程重启、删除隔离、第二核缺失符号导致添加失败后的回滚、退回单核、混合运行状态及断点联停。证据：`artifacts/linked-breakpoints-1789900258546/verification.json`。
- 原多核 13 类场景与单核断点 60 请求回归通过：`artifacts/multicore-1789900841736/verification.json`、`artifacts/breakpoints-native-1789900841737/`。
- THA6206 Release 实板 61 请求通过：core0 / core1 默认独立新增、跨核关联、条件/忽略次数同步及重连；分别在 `Gpt_Notification_UartPrintf` 和 `DemoApp_MainCore1` 命中并核对两核实际 halted；取消关联、保留无关断点、删除后两核运行且 uart_cnt 增长。证据：`artifacts/multicore-breakpoints-20260920/hardware-release/result.json`。
- 实板 OpenOCD 分别报告 core.0 和 core.1 各 8 个硬件断点、8 个硬件观察点。此结果是每核资源报告，不是整片芯片共用配额，也不是对所有芯片的固定限制；Flash 普通源码断点由 GDB 自动使用硬件资源。

## 0.8.3-local.2：运行中 Watch 刷新（2026-09-20，本地修复版）

- 修复旧 `[live_watch]` 采样只写 Console、Watch 仍显示停止快照的问题。每次读取发送结构化 `live_watch` 事件；Watch 显示实时值和 `LIVE`，正常采样不再刷 Console。原始 8/16/32 位全局量保留 `<raw N-bit>` 标注，显式单项刷新策略优先。
- `cargo test --locked -- --test-threads=1`：132 个单元测试与 1 个 CLI 集成测试通过；Clippy `--all-targets -- -D warnings` 通过。新增真实 Watch 行渲染、进制转换、旧快照覆盖防护、过期/错核/已删除/停止后事件丢弃、失败恢复及显式刷新策略优先测试。
- THA6206 双核实板、GHS ELF、AHB_3、200 ms：core0 和 core1 的 `uart_cnt` 连续变化，与独立 AHB 读取区间一致；五个采样窗口均保持两个核运行，没有隐式 Pause/Continue。切核、状态查询、删除再添加、全核暂停及整片复位后恢复采样通过。记录：`artifacts/tha6206-live-watch-20260920/hardware-final/result.json`。
- 将上述实板 JSON 事件逐条交给实际 App 和 Ratatui Watch 渲染器，断言每个成功采样值及 `LIVE` 均出现在 Watch 数值行。回放测试通过，彩色缓冲保存在 `artifacts/tha6206-live-watch-20260920/ui-replay/`；这是实板事件的渲染回放，不是 VS Code 截屏。
- 停止后使用 GDB 快照。软件组暂停存在核间时间差，共享变量可能在先停核刷新后继续变化；实板校验在全核停止后显式 Refresh，再与 AHB 比较。AHB 读取仍受目标缓存一致性约束；本次验证对象是工程中该全局计数器，不代表任意结构体/表达式均可运行时求值。

## 0.8.3-local.1：THA6206 软件多核组控制（2026-09-20，本地修复版）

- `cargo test --locked -- --test-threads=1`：128 个单元测试与 1 个 CLI 集成测试通过；`cargo clippy --locked --all-targets -- -D warnings` 通过。覆盖组范围/部分失败/断点焦点/非阻塞组等待/单次共享复位和全核刷新、UTF-8 服务日志分片、混合运行状态下的按钮可用性及窄窗口布局。
- `scripts/test-multicore-gdb.cjs target/release/debugtui.exe`：两个真实本机 GDB 的 13 类场景通过，包括组 Continue/Pause、断点联停与等待、单步隔离、重复 Run、范围切换、旧独立/单核行为及连接/服务失败清理。证据：`artifacts/multicore-1789892910291/verification.json`。
- THA6206 双 Cortex-R52、GHS 2023.1.4 ELF、工程 GDB 16.3 + 定制 OpenOCD、CMSIS-DAP SWD 5 MHz：最终 Release 122 请求、27 项核对通过。每核重复断点联停，单步不改变另一核 PC，运行中 AHB counter 递增，整片复位后两核 GDB PC 同为 0x08000000，独立/组模式切换及重连成功。证据：`artifacts/tha6206-multicore-fix-20260920/hardware-release-safe-regs/result.json` 与日志。
- 不承诺 CTI 硬件同时暂停；保留底层 GHS ABI 和部分复位寄存器访问诊断，详见该审计目录的 `REPORT.md`。未烧写或修改被测 ELF。

## 0.8.2：顶部启动按钮与返回配置（2026-09-20）

- 118 项 Rust 单元测试 + 1 项 shell 集成测试通过；Clippy `--all-targets --locked -- -D warnings` 无警告，Release 构建完成。新增覆盖顶部按钮在 45×12 / 80×24 / 120×36 下的命中区域、默认 Enter 启动、正在编辑字段的提交、无效配置不启动、Workspace / Esc 返回及草稿保留。
- Ratatui 单元格预览位于 `artifacts/ui-launch-0.8.2/`，检查 setup / setup-narrow / setup-compact / workspace。启动按钮固定在表单上方，主界面 Project 栏提供独立的 ← Setup。
- 安装后的 0.8.2 在真实 Windows 终端连接 STM32F429，使用 tools 中的 GDB + OpenOCD。Enter 从默认启动按钮连接；鼠标点击 ← Setup 后，TUI / GDB / OpenOCD PID 保持不变。将 timeout 改为 9000 后通过 Workspace 返回，再打开配置，草稿仍在且磁盘仍为 8000。
- 在字段编辑状态输入 9500 并按 Ctrl+R，无需先按 Enter：配置成功保存，旧 GDB / OpenOCD 清理后重新连接；TUI PID 始终为 9264。GDB PID 为 37128 → 26436，OpenOCD 为 18340 → 30860。重新连接后 `p xTickCount` 返回 82729393，Watch 及两个禁用的代码/读数据断点记录成功恢复。
- 再次通过 F2 打开配置、鼠标点击顶部 Start 完成第二次重启，GDB / OpenOCD PID 分别为 35376 / 36192；随后 F2 / Esc 返回工作区正常。Ctrl+Q 退出码 0，三份 session 日志均包含连接、`monitor resume` 和 GDB 退出，结束后无调试进程及 3333/6666 监听。证据：`artifacts/terminal-launch-0.8.2/verification.json`、该目录的进程快照和 `logs/`。测试使用单独配置文件，没有改动用户工程配置或下载固件。
- 本地 npm 全局安装验证 `debugtui --version` 为 0.8.2；安装后 EXE 与 release SHA256 均为 `1E29FF6A70732403E2AF7D51F94B00A401D6DA69E096B2D32238B4F08A47691E`，EXE 大小 3,437,056 字节。

## 0.8.1：断点开关、数据断点与保存恢复（2026-09-19）

- 114 项 Rust 单元测试 + 1 项 shell 集成测试通过；Clippy `--all-targets -- -D warnings` 无警告，Release 构建完成。新测试覆盖勾选禁用/启用、重复操作抑制、Delete 与输入隔离、数据读写模式、失败保留编辑器、核心切换和列表选择、45×12 / 80×24 / 140×40 布局、旧配置与详细断点的往返保存。
- `scripts/test-breakpoints-gdb.cjs` 使用真实本机 GDB：条件/忽略次数/实际命中计数、临时断点、写数据断点、强转地址表达式、无效条件回滚、部分创建失败清理、批量启停、立即保存、重连与进程重启恢复、无法恢复的记录保留及明确删除。`--multi` 使用两套真实 GDB，验证启停只影响当前核心，两个核心分别保存和恢复。
- STM32F429 + J-Link V8 / tools OpenOCD：同一脚本 `--hardware` 先执行 `compare-sections .text` 确认固件匹配；不重新下载固件。代码断点、条件/忽略次数、临时/硬件代码断点，以及 `*(unsigned int *)&xTickCount` 的 **Write / Read / Read-write** 三种数据断点均实际触发暂停；禁用与重新启用保持编号，三类禁用数据记录与条件代码断点在重连和重启后完整恢复。首次通过记录：`artifacts/breakpoints-stm32-1789827910744/verification.json`（75 个请求）。
- 实测发现远程 GDB 在自动跨过被忽略的断点时偶尔拒绝 `-thread-info`；已修复为在原超时内继续等待，未伪造 STOPPED。`scripts/test-pause.ps1` 新增 query-rejected 场景，六种暂停竞态/超时测试通过；真实运行目标仍按期报超时。
- 原 Watch 树/补全/强转表达式和 11 组多核回归通过：`artifacts/watch-tree-1789828265871/`、`artifacts/multicore-1789828276937/verification.json`。多核硬件仍未做实板验收，当前硬件是单核 STM32F429。
- 实际 Ratatui Buffer 渲染预览：`artifacts/ui-breakpoints/{breakpoints,breakpoints-narrow,breakpoint-editor}.png`；逐项检查标签、开关、对话框及窄终端命中区域。
- 安装后的 0.8.1 再次通过本机断点 60 请求、双 GDB 23 请求和 STM32 75 请求：`artifacts/breakpoints-native-1789828383385/`、`artifacts/breakpoints-multi-1789828387086/`、`artifacts/breakpoints-stm32-1789828381945/`。实际终端用 Space 启用/禁用同一代码断点，通过编辑器选择 Read 并创建 xTickCount 数据断点，再用鼠标点击勾选框禁用；两个禁用记录均立即写回测试工程。日志：`artifacts/terminal-breakpoints-0.8.1/logs/session-20260919-223353-374.log`。Ctrl+Q 返回 0，测试结束后 debugtui / GDB / OpenOCD 进程及 3333/6666 监听端口均为 0。
- 本地全局 npm 安装完成：`debugtui --version` 为 0.8.1；release 与安装后 EXE SHA256 一致：`3D621BC656EC56DC73DB4CEE8343DD2EF045EE5B88F6C53C854475ACC842D84F`，文件 3,432,960 字节。无需新增运行时依赖。

## 0.8.0：核心按钮、内存通道和可见项实时刷新（2026-09-19）

- 107 项 Rust 单元测试 + 1 项 shell 集成测试通过；`cargo clippy --all-targets -- -D warnings`、`cargo build --release --locked` 通过。测试涵盖核按钮命中/窄窗口、核心底色及 Source/Asm 内容切换、刷新策略持久化/分核隔离、延迟响应丢弃、轮询间隔、隐藏/删除项、运行状态访问门控、SVD 副作用寄存器禁用轮询、手动位域刷新父寄存器，以及内部 TCL 回显识别。
- 双真实本机 GDB + 模拟 TCL：`artifacts/multicore-1789825041872/verification.json`，11 组回归通过，包括两核独立状态、失败回滚、单核兼容、多个 CTI 初始化顺序、借核通道的核心限制和共享 AHB 在某一核运行时读取。**没有多核实板，CTI 触发/多 AP 硬件路由尚未做实板验收。**
- Native GDB 内存通道测试：`scripts/test-memory-access-gdb.cjs`，验证有符号/浮点/64 位类型、嵌套成员和地址强转、无地址表达式拒绝、TCL 超时/断线/错误恢复、64 位端序、禁止隐式停核、读完仍可 Pause/Step/Asm。release 记录：`artifacts/memory-native-1789825035841/verification.json`，已安装 exe 记录：`artifacts/memory-native-1789825387920/verification.json`。
- STM32F429 实板：J-Link V8 探针 + tools 内 xPack OpenOCD，SWD 1000 kHz；建立 `stm32f4x.cpu` 和 AP0 `stm32f4x.ahb`。先发现旧 `Debug/FreeRTOS_Project.elf` 与板上固件不符；改用 `build/FreeRTOS_STM32F429.elf`，GDB `compare-sections .text` 报告 **matched**。未下载或改写固件。
- 实板暂停时，GDB、CPU target 和 AHB 对 `xTickCount` 的值一致；AHB/GDB 的 RCC.CFGR、GPIOB.MODER、DBGMCU.IDCODE 一致。运行时按 100 ms 请求间隔采样 20 次并读 GPIOB，tick 持续增长、状态保持 RUNNING、generation 不变、没有新增 GDB 命令；默认 GDB 和 stopped-only Core 通道被拒绝。之后 Pause、单指令步进、反汇编、系统寄存器和 Watch 正常。自动启动和退出自有 OpenOCD 已验证，退出后 3333/6666 端口释放。
- 已安装 TUI 的真实终端测试：`artifacts/terminal-0.8.0/`。从 Watch 的 `f → r` 菜单选择 AHB，启用 100 ms 刷新并保存；确认 `[ui.refresh."single|watch:xTickCount"]` 写回，运行后 LIVE 数值持续变化，Ctrl+Q 清理退出。测试发现并修复 OpenOCD 将 TCL 返回值同时回显到 GDB target 和自有 Server stderr 导致 Console 刷屏：内部返回加唯一标记，统一归入 diagnostic，原始目标输出仍保留。
- 原 Watch 树/自动补全/日志轮转、ARM/RISC-V/x64 模拟 MI、五种 Pause 竞争场景均回归通过。release 记录：`artifacts/watch-tree-1789825034262/`、`completion-native-1789825040444/`、`logs-native-1789825043464/`、`environment test 工程 20260919-213719/`、`pause-20260919-213726/`。
- UI 预览从 Ratatui 实际缓冲导出并检查：`artifacts/preview-0.8.0/core0.png`、`core1.png`、`refresh-settings.png`；不是设计稿。npm 包不包含 OpenOCD/GDB/Node/Python，tools 独立校验打包通过。
- 旧 J-Link Server 实板尝试未通过：当前 USB 设备为 WinUSB/libwdi (`oem8.inf`)，SEGGER V7.94e 在 TUI 内及独立 CLI、SWD 4000/1000 kHz 下均报告 `Could not connect to J-Link`。未更改系统 USB 驱动；本次实板通过项均使用 OpenOCD，不能宣称 J-Link Server 实板回归成功。记录：`artifacts/svd-hardware-1789825411709/`。
- 本地 FreeRTOS 工程 `debug.toml` 已将 ELF 改为匹配的 build 文件、Tools profile 改为 `debug-env-openocd.toml`，其余工程设置保留；原配置备份为 `artifacts/debug-before-live-0.8.0.toml`。Git 提交和发布不在本次操作范围。
- 最终安装版本 `debugtui 0.8.0`；release 和 npm 全局安装 exe 的 SHA-256 均为 `7D2B0539E6FF5CCFC8C6B0B650F56951BCE31B3355BB485318E5C018185E20C5`。最终已安装 exe 实板 70 个请求全部通过：`artifacts/memory-stm32-1789825648786/verification.json`。
- 最终终端复测：`artifacts/terminal-0.8.0/logs/session-20260919-214757-727.log`，恢复已保存的 Watch 100 ms 策略，RUNNING 下 LIVE tick 持续增长；隐藏 Watch 后不继续读取它，在 Peripherals 展开 ADC1、为 CR1 选择 AHB 和 500 ms 策略，显示 `0x0 hex LIVE`。100 次 AHB 采样时 Console 内部回显为 0，寄存器读和 Watch 读均不写硬件；退出码 0。当前采样时 TUI 工作集约 12.22 MiB / private 5.59 MiB（含加载 F429 SVD，不含 GDB/OpenOCD，非性能保证）。

## 0.6.6：Watch 面板直接增删

- Watch 底部变量输入框增加 **+ Add** 按钮，复用自动补全及 Enter 提交流程；空输入点击聚焦，重复提交不重复排队，失败保留输入，旧响应不清除新草稿。
- 每个变量名称行右侧增加 **×**，点击直接删除对应表达式，无需预先选中；滚动至只有数值行可见时仍保留该项删除入口。点击目标按表达式识别，避免异步列表刷新后误删相邻项。原有 Delete / Del Remove 保留。
- 78 项单元测试 + 1 项实际 shell 集成测试通过，Clippy 零警告，Release 构建通过。新增测试覆盖鼠标增删、运行状态、错误重试、输入隔离、Help / Locals 焦点、45×12 至 160×42 布局、滚动和刷新后的点击目标。
- 原生终端 + 真实本机 GDB 实测：断点停在 main，点击 + Add 聚焦、输入 `coun` 并 Tab 补全为 `counter`，点击添加显示 17；再点击添加 `counter_total` 显示 23；直接点第二项的 × 仅移除第二项，再点第一项的 × 清空。整个 Watch 增删过程未输入调试命令。正常退出码 0，保存配置 `watch = []`。
- 交互记录：`artifacts/ui-0.6.6/terminal/interactions.json`。使用独立本机测试程序，未连接物理 MCU。

## 0.6.5：Watch 删除与焦点修复

- 修复前用三个回归用例复现：删除末项后选中行越界，无法继续删除；删除前面的项目后选择偏移到其他表达式；Console 历史焦点仍触发 Watch 的 Delete。修复后均通过。
- Watch 名称行和数值行对应同一个选择；列表变化后按表达式保持选择，被删除时移动至相邻项并修正滚动位置。删除请求完成前抑制重复发送，失败后可重试，按住 Delete 的重复事件不会连续清空列表。
- Watch 标题增加 Del Remove 鼠标入口；输入框、Console、Help 焦点不误删 Watch，输入草稿保留。测试覆盖滚动后的名称/数值行、鼠标按钮、连续删空、错误响应和运行中移除。
- 73 项单元测试 + 1 项实际 shell 集成测试通过，Clippy 零警告。真实本机 GDB 回归额外覆盖停止时删除两项、运行中删除一项：删除没有额外 MI 请求，没有暂停目标或改变变量；重连并再次停止后已删除项未恢复，保存的项目 Watch 列表为空。
- 原生 GDB 验证记录：`artifacts/completion-native-1789653131310/verification.json`。测试仅操作独立本机测试程序，不连接物理 MCU。
- 本地安装版真实终端验证：断点暂停后选中最后一项，连续两次 Delete 分别移除 `counter_pair.value`、`counter_total`，再鼠标点击 Del Remove 移除 `counter`；空列表再次 Delete 正常提示，退出码 0，保存的配置为 `watch = []`。交互输出：`artifacts/ui-0.6.5/terminal/interactions.json`。

## 0.4.2：Source root 命令与独立 Project 操作区（2026-09-17）

- 启动页在 Source root 后增加 Build command / Download command，保留引号并保存至项目 `[tasks]`。配置解析、相对路径、tools 下载动作回退和未生成 ELF 时进入工作台均有回归覆盖。
- 主界面顶部独立 Project 栏显示 Build / Download，与 Run / 单步工具栏分开；45×12 至 160×42 均可见。未配置时禁用；执行中抑制重复任务；下载确认显示具体命令及工作目录。
- 40 项单元测试 + 1 项 Windows 实际子进程集成测试通过，Clippy 零警告，Release 构建通过。子进程测试覆盖中文/空格工作目录、带引号脚本路径和参数、stdout/stderr、非零退出码、超时及 Ctrl+Q 取消。
- 本机 GDB 完整测试：先从 Source root 实际 GCC 编译，再连接、断点运行；已连接时重新 Build，释放 GDB 后成功替换 EXE 并重连；配置的 Download 测试脚本执行成功后再次重连。两次重连后的运行都命中恢复的 main 断点。记录：`artifacts/ui-0.4.2/project-tasks/events.jsonl`。
- 实际 Windows 伪终端验证 F2 配置展示、点击 Build 编译与重连、点击 Download 确认并运行脚本、正常 Exit。记录：`artifacts/ui-0.4.2/terminal-interactions.json` 与 `interactive-logs/`。
- 原有真实 GDB 调试回归（断点、变量、单步、寄存器、反汇编、清理）通过：`artifacts/native-gdb-20260917-010958/`。彩色布局预览：`artifacts/ui-0.4.2/`。

Download 使用本地验证脚本检查执行目录、参数与会话恢复，没有烧写 STM32，也未重测探针长会话问题。

## 0.4.1：单步命名、Exit 与 Help（2026-09-17）

- 工具栏改为 Step In / Step Over / Step Out，继续分发 `step` / `next` / `finish`；F11 / F10 / Shift+F11 保持原有行为。
- 新增 Exit，复用 Ctrl+Q 的会话清理与退出流程。45×12 极矮窗口也保留 Exit / Help；退出等待期间禁用重复点击和快捷键请求，demo 的 Exit 同样结束程序。
- CommandList 改为 Help，包含 Commands / Shortcuts 两页，支持点击页签和 Tab 切换；`?` / `:help` 直接打开快捷键页。命令列表执行、带参数命令填入 Console 的行为保持不变。
- 38 项 Rust 测试、Clippy 零警告和 Release 编译通过。新增测试覆盖不同状态的 Exit 分发、取消标志、重复退出抑制、demo 退出，以及两种窗口尺寸下 Help 页签、键盘导航和命令执行。
- 真实 Windows 伪终端 + 本机 GDB：断点停在 main.c:4；点击 Step In 进入 helper.c:2；点击 Step Out 返回 main.c:4；两次 Step Over 分别到 main.c:5、main.c:6，第二次越过函数调用。Help 的鼠标页签和 Tab 切换正常；点击 Exit 返回码 0，日志确认 `-target-detach` 和 `-gdb-exit`，测试进程退出。
- 完整终端交互与 MI 日志：`artifacts/ui-0.4.1/native-interactive/`；Ratatui 缓冲彩色预览：`artifacts/ui-0.4.1/`。

本轮验证使用独立的本机程序，没有重新验证 STM32/J-Link 长会话问题。

## 0.4.0：终端视觉层改造（2026-09-17）

- 深色分层面板、真彩色主题、图标工具栏、状态徽标、选中/悬停反馈、圆角 Console 和弹窗；全行执行位置与变化值背景；启动配置页统一视觉样式。新增轻量 C 类语法着色，跨行注释状态只在当前文件载入时计算。
- 36 项 Rust 测试、Clippy 零警告与 Release 构建通过。保留原有点击、滚动、文件标签、Console 和数据加载回归；新增验证全行背景、弹窗不透出底层字符、悬停不发送调试命令、45×12 至 180×50 的控件边界与输入焦点，以及字符串/Unicode/跨行注释着色。
- 极矮终端保留 Continue / Pause / CommandList 三个主要按钮，为源码留出可见空间；其余命令继续由 CommandList 提供。80×24 及常规窗口显示完整工具栏。
- 真实本机 GDB 回归验证连接、断点、变量求值、Step、x86 寄存器、反汇编和退出清理。记录：`artifacts/native-gdb-20260917-002657/`。
- Windows 伪终端真实交互：Console 输入 `:break main`、`:run`，在 main.c:4 停止；发送 F11 后进入 helper.c:2，两个源码标签保留；Ctrl+Q 正常退出。完整 ANSI 输入/输出与 GDB 日志：`artifacts/ui-0.4.0/native-interactive/`。
- 真彩色模式独立验证：仅在测试子进程去除 `NO_COLOR` 并设置 `COLORTERM=truecolor`，启动实际 EXE，初始画面捕获 149 次 RGB 颜色控制指令、14 个不同前景/背景颜色序列；Ctrl+P 命令弹窗正常。程序继续遵循调用者的 `NO_COLOR`，不修改系统或用户终端设置。
- 最终 EXE 1,479,680 字节，较 0.3.4 的 1,462,784 字节增加 16,896 字节（16.5 KiB，约 1.16%），没有增加 Cargo 或运行时依赖。
- 本机单次 5 秒空闲采样：真彩色 demo 命令面板私有内存 1.26 MiB、工作集 6.42 MiB，CPU 时间增量 0 ms；真实 GDB 暂停会话 UI 私有内存 1.72 MiB、工作集 7.18 MiB，CPU 增量 0 ms。这是本机短采样，不代表所有工程或操作负载。
- 从真实 Ratatui 单元格缓冲导出的彩色预览：`artifacts/ui-0.4.0/`，包含 workspace / console / narrow / compact / commands / files / assembly / setup；PNG 用于检查视觉效果，不进入运行时安装包。

本轮修改视觉与本地输入反馈，没有修改 GDB 会话命令实现、tools 或用户固件，也没有重新验证或修复下述 STM32/J-Link 长会话问题。

## 0.3.4：Source 多文件标签与变量区分割线（2026-09-16）

- Watch / Locals 上方恢复贯穿右侧面板的横向分割线。
- Source 文件通过暂停/断点停止、栈帧切换、Files 和 `:open` 打开后保留为标签，可点击切换或 `×` 关闭，独立记住浏览位置。真实路径去重、同名文件路径后缀、中文/长文件名显示及关闭按钮命中均有覆盖。
- `[<]` / `[>]` 与标签栏滚轮支持浏览隐藏标签，`[Files N]` / Ctrl+O 打开可搜索文件列表，Ctrl+PgUp/PgDn 切换，Ctrl+W 关闭。标签仅缓存元数据，当前源码文本仍限制为单文件 2 MiB / 30000 行。
- 33 项 Rust 测试全部通过；Clippy `--all-targets --locked -- -D warnings` 通过；Release 构建通过。新增测试覆盖 120 个标签、45×12 至 160×45 的缩放与活动标签可见性、筛选隐藏文件、关闭活动/最后一个标签、关闭后普通刷新不重开、新停止重新打开、源码路径别名的断点定位，以及无源码停止时保留标签但不冒充旧执行位置。
- 真实本机 GDB + Windows 伪终端验证：在 main.c 设置断点运行，Step 进入 helper.c 后显示两个标签；点击 main.c 只浏览源码，GDB 仍停在 helper.c；点击 helper.c 的 `×` 后，下一次 Step 停止重新打开该文件；打开 `[Files 2]`，输入 `main` 筛选并切换成功。正常 Ctrl+Q 退出。
- GDB 记录：`artifacts/ui-0.3.4/native-tabs/logs/session-1789572980354.log`。第 55、71 行为两次 Step；停止位置分别为 helper.c:2 和 helper.c:3。浏览/关闭标签没有发送 `-stack-select-frame` 或 `-break-delete`。终端输入与 ANSI 输出保存在 `artifacts/ui-0.3.4/native-tabs/terminal-interactions.json`。
- 布局预览：`artifacts/ui-0.3.4/source-tabs.txt`、`source-tabs-narrow.txt`、`open-files.txt`。
- Release EXE SHA256：`06E7F0F483E36C0AF8B8FC5351E98058D80739081D69DB21571B58034BFE4B2F`。

本轮为源码浏览交互改动，真实调试验证使用独立的本机测试程序；未重新验证下述 STM32/J-Link 长会话问题。

## 0.3.3：版本标题、统一滚动与 Console 输入（2026-09-16）

- 工作台及启动配置页显示 `DebugTUI v0.3.3`，版本来自 Cargo 元数据。
- 左侧 Source / Asm / Files / Log 均有滚动轨道，内容超过一页时显示可拖动滑块。右侧为 Regs / Stack / Memory / Breaks，下方为 Watch / Locals，标签独立切换。右侧列表也复用滚动逻辑。
- Console 内固定显示 `gdb>` 输入框，鼠标点击或 `/` 聚焦。支持 GDB 原生命令、`:命令`、连续提交、↑ / ↓ 历史、Esc 退出输入；输入期间保留功能键调试操作。
- `cargo test --locked`：28 项全部通过；`cargo clippy --all-targets --locked -- -D warnings`：通过；Release 构建通过。新增覆盖标签位置及窄窗口可达性、Asm/Files/Log 首尾滚动及标签位置保留、滚动后的文件点击定位、Log 旧记录浏览和恢复跟随、Console 请求分发及连续输入/历史/长 Unicode 命令。
- 布局预览：`artifacts/ui-0.3.3/`。覆盖 45×12、80×24、120×36、180×50 渲染；实际 Windows 伪终端验证 Console 鼠标聚焦与提交。
- 已安装版本连接真实本机 x86 GDB，在独立 sample.c 测试程序执行 `:break main`、`:run`、`p/x counter`、`next`，再用 ↑ 历史重发 `p/x counter`：结果依次为 `0x1`、`0x2`。Asm 自动加载实际反汇编，鼠标拖动至末尾可浏览后续指令；Files 和 Regs 切换正常。日志：`artifacts/ui-0.3.3/native-console/session-1789571838149.log`，第 54/70/86/102/104 行分别记录求值、Next、再次求值、反汇编及文件列表请求。Ctrl+Q 正常退出并关闭自有 GDB。
- `scripts/test-native-gdb.ps1`：真实本机 GDB 的启动、断点、变量求值、Step、动态寄存器、反汇编和清理全部通过；记录位于 `artifacts/native-gdb-20260916-231700/`。
- `scripts/test-npm.ps1`：隔离安装、升级、CMD/PowerShell 入口、配置保留、卸载及重装全部通过。已通过 npm 全局安装更新本机入口，`debugtui --version` 返回 `debugtui 0.3.3`。
- 安装 EXE 与 Release EXE 的 SHA256 一致：`E0573E7407C802BBB02733D2C8F8009EAC11E3F90C91E46B3A9C8CB633B77B93`。

本轮验证的是界面交互和真实本机 GDB 命令链路，没有重新测试或修复下述 STM32/J-Link 长会话访问异常，没有更换调试环境工具或修改用户固件。

## 追加隔离实验：Next / Step / Stepi / Monitor（2026-09-16）

**新结论：在本套环境中，不发送任何单步命令，停在断点后保持连接 60 秒，也能复现底层读取失效。不能继续将根因概括为 F11 或源码单步算法。根因部件仍未确定，尚未修复。**

### 条件与方法

- 直接运行 tools 中的 ARM GDB/MI 与 J-Link GDB Server 7.94e，绕过 DebugTUI；同一 V8 探针、SWD 4000 kHz、STM32F429IG、同一 ELF。
- 每组独立启动 Server/GDB，复位到相同程序状态，检查 Flash 的 6 个只读段与 ELF 匹配，然后停在 `user_task.c:40`（`0x080007e2`）。不下载固件。
- 每轮先 Continue 到该断点，再只执行该组的一种命令；核对 PC、SP、xPSR，不能只以 `^done` / `^running` 判断成功。
- 预期：Next 到 `0x080007e6` / `user_task.c:44`；Step 到 `0x080005fc` / `bsp_led.c:32`；Stepi、`monitor step` 到 `0x080005f8` / `Led_Task` 入口。Monitor 每轮直接读取 `monitor regs`，并刷新 GDB 寄存器缓存后交叉读取。
- 日志逐行记录距启动的毫秒时间。发生错误先保留日志并尝试读取寄存器、CPUID、DHCSR、CFSR、HFSR，不在组内复位/重连恢复后继续计数。

### ① 四种操作独立对照

短会话：每种方式 **3 个会话 × 100 次全部通过**，合计 1200 次；每会话约 18–23 秒。初步 5 次校验不计入这些数字。

进一步将每种方式的单会话上限提高至 500 次：

| 分组 | 异常前完整通过轮数 | 首个异常时间（距 Server 启动） | 实际失败阶段及证据 |
|---|---:|---:|---|
| Next | 241 | 52.522 s | 第 242 轮 Continue 回断点时异常，Server 的 xPSR 等寄存器读数失真，随后等待停止超时 |
| Step | 249 | 53.325 s | 第 250 轮 Continue 时 Server 报 `No more breakpoint resources left`、断点插入失败；GDB 返回 `Command aborted` |
| Stepi | 268 | 52.575 s | 第 269 轮 Continue 时同样发生断点插入失败和 `Command aborted` |
| Monitor Step | 310 | 52.893 s | 第 311 轮 Continue 后 SP/LR/PC/xPSR 均为 `0xab78`；直接 `monitor regs` 返回全零，系统寄存器读取 Failed |

**注意：本次四个长会话的失败阶段都是重新 Continue 定位断点，并非可以据表格认定四种单步命令本身都失败。** 不同操作次数却在接近的时间失效，因此追加静置对照。

Step/Stepi 的完整协议还显示：同一 token 先返回 `^running`，随后返回 `^error,msg="Command aborted."`。首次测试脚本只收到前者时报告停止等待超时，原始日志保留了后者；脚本现已识别这种后续错误。不能把这两次解释成 CPU 正常运行而单纯丢失停止通知。

### ② 绕过源码单步与静置对照

| 测试 | 结果 |
|---|---|
| `monitor step` + `monitor regs` | 短会话 300 次通过；长会话前 310 次通过，失效后直接 monitor 读取也不正常 |
| 停在同一断点，不发送任何命令 30 秒 | 读取仍正常，后续 10 次 monitor 单步全部通过 |
| 同样保持 60 秒，第 1 次 | 尚未执行单步，`monitor regs` 已返回全零；刷新缓存后 GDB 的 PC/SP/LR/xPSR 均为 `0x5428` |
| 同样保持 60 秒，第 2 次 | 尚未执行单步再次失败；GDB 的 PC/SP/LR/xPSR 均为 `0x9768` |

60 秒失败现场直接读取 `0xE000ED00`（CPUID）、`0xE000EDF0`（DHCSR）、`0xE000ED28`（CFSR）、`0xE000ED2C`（HFSR）均返回 `Failed`。这是 monitor 命令的输出内容；即使 MI 外层是 `^done`，也不能算硬件读取成功。

这将问题范围收敛为 **连接/暂停持续一段时间后，Server/探针/目标侧的访问状态失效**，而不是 F10/F11 专属操作。52–53 秒为本次四个连续操作会话的观测值，不等于已经证明存在固定超时设置。仍需区分 J-Link Server/DLL、探针固件/硬件、目标复位/调试状态及连接条件；未验证的软件或硬件原因不得标为已确认。也不能把错误读出的 PC 当作真实程序执行地址。

### 复现与证据

脚本仅用于开发测试，不进入运行时安装包：

```powershell
node scripts/test-step-isolation.cjs <ELF> 100 3
node scripts/test-step-isolation.cjs <ELF> 500 1
node scripts/test-step-isolation.cjs <ELF> 10 1 monitor 30000
node scripts/test-step-isolation.cjs <ELF> 10 1 monitor 60000
```

每个会话保存 `trace.log`、`timeline.json`、`result.json`，总目录有 `results.json`。

- 1200 次短会话：`artifacts/step-isolation-2026-09-16T13-50-39-156Z/`
- 长会话四组：`artifacts/step-isolation-2026-09-16T13-54-58-531Z/`
- 首次静置 60 秒：`artifacts/step-isolation-2026-09-16T13-59-37-549Z/`
- 静置 30 秒对照：`artifacts/step-isolation-2026-09-16T14-01-10-862Z/`
- 重复静置 60 秒：`artifacts/step-isolation-2026-09-16T14-01-45-671Z/`

ELF SHA256：`076B3D1D9CC4DC04BBBBB6269E816673BA78CBF9730E54BF8B9A4718D842FCDD`。本轮未修改 TUI 执行逻辑、工具默认配置或用户工程配置。

## 0.3.2：断点后 Step / Next 异常的实板复现

日期：2026-09-16。结论：**底层异常已经复现，尚未修复；0.3.2 修复的是异常位置显示，不能作为底层稳定性修复。** 未更换工具二进制、未修改默认 SWD 配置、未重新烧写固件。

用户日志：`G:/Data/GitFiles/Keil/STM32_CubeIDE/FreeRTOS_Project/debug-logs/session-1789564435915.log`。

- 断点 `user_task.c:40` 正常命中 9 次后，日志第 1573 行发出 `105-exec-continue`。随后 J-Link 报告断点移除/设置失败，GDB 提示程序不可写，PC 变为 `0x00006978`、函数 `??`，多个寄存器返回相同异常数值。
- 第 1926–1934 行的三次 `-exec-step` 均返回 `Cannot find bounds of current function`。这是异常 PC 之后的结果；本份日志没有 `-exec-next`，不能声称记录到了 Next 的按键操作。
- 测试前 `compare-sections -r` 的 6 个只读段全部匹配板上 Flash。正常时 Step 进入 `Led_Task` 的 `bsp_led.c:32`，Finish / Next 返回 `user_task.c:44`；当前 ELF 未启用 `VTASKDELAYUNTIL`，实际延时代码在 44 行而非 42 行。

| 对照 | 实际结果 |
|---|---|
| 已安装 0.3.1、默认 4000 kHz | 连续命中断点 40 次通过；第 14 轮 Step/Finish/Next 序列中 Next 后 PC 变为 `0x00007efc`，PC/SP/LR/xPSR 返回相同异常数值 |
| 临时覆盖为 1000 kHz | 第 64 次继续到断点时 PC 变为 `0x00006920`；降频没有解决 |
| 不启动 DebugTUI，直接 GDB CLI + J-Link | 24 轮完整序列通过，第 25 轮 Step 后 PC 变为 `0x0000a490`、无调用帧；复现不依赖 TUI/MI 调度 |
| 直接 GDB，关闭 Flash 断点并使用 hbreak | 26 轮完成后再次出现异常寄存器读数并持续等待；终止本次测试的 GDB，Server 随连接关闭退出；该设置也未解决 |
| 失败后刷新 GDB 寄存器缓存 | 异常读数仍存在；读取 `0xE000ED28` 故障状态寄存器失败 |
| 本地安装 0.3.2 短程回归 | 10 次继续到断点 + 10 轮 Step/Finish/Continue/Next/Next/Continue 通过，检查了函数、源码行、xPSR 和异步停止通知；没有使用 wait_stopped 掩盖通知问题 |
| 通用回归 | 24 项 Rust 测试、Clippy 零警告、Release 构建、5 个 Pause 异常分支通过；安装 EXE 与构建 EXE 的 SHA256 相同 |

上述结果把问题范围缩小到 GDB 以下的调试链路及目标状态，尚不能区分 J-Link Server/DLL、探针固件/硬件、SWD/USB 连线或目标侧问题。后续需用另一探针/线缆或另一套已验证驱动做单变量对照，不能把重连恢复、短程测试通过当作长期故障已修复。

0.3.2 的界面修复：没有源码位置的停止事件会清除上一次源码视图，并显示实际 PC、函数名与检查提示；顶部位置取自 GDB 当前帧，手动浏览文件不会改写停止位置；Step/Next 错误附带执行地址和函数。未自动改为 stepi、复位或重连。通用 TUI 没有加入 STM32/J-Link 特例。

复测脚本：`node scripts/test-step-hardware.cjs <ELF> [EXE] [轮数=40] [SWD_KHZ]`。该开发测试针对当前 STM32 FreeRTOS 固件、创建隔离配置，不改用户工程配置；生成命令结果、停止位置及完整 MI/Server 日志，不进入运行时安装包。

证据目录：

- 4000 kHz 复现：`artifacts/step-hardware-2026-09-16T13-22-58-427Z/`
- 1000 kHz 复现：`artifacts/step-hardware-2026-09-16T13-25-11-193Z/`
- 直接 GDB 与硬件断点对照：`artifacts/step-direct/`（含 GDB 命令文件、GDB 输出及 Server 日志）
- 安装版短程：`artifacts/step-hardware-2026-09-16T13-31-06-255Z/`
- Pause 回归：`artifacts/pause-20260916-213107/`

对照命令依据：[SEGGER 的 GDB Server monitor 命令](https://kb.segger.com/J-Link_GDB_Server)、[GDB Step / Next / Stepi 语义](https://sourceware.org/gdb/current/onlinedocs/gdb.html/Continuing-and-Stepping.html)。

## 0.3.1：Pause 超时后的状态核对

日期：2026-09-16。已本地安装 0.3.1，安装 EXE 的 SHA256 与 Release 构建相同。

用户报告设置源码断点、继续运行后 Pause 等待停止超时。旧版实板连续 50 轮未重现原始间歇故障；在“interrupt 返回确认、GDB 线程已停止，但缺失停止通知”的受控模拟中，旧版稳定出现同样的等待超时，退出清理也重复等待。不能据此断言原始故障一定由 J-Link 或 GDB 丢通知引起。

修复为：保留异步停止通知处理，同时每 250 ms 使用标准 `-thread-info` 核对线程状态；只有线程确认为 stopped 才同步并刷新上下文。若仍运行，最多补发一次 interrupt；无法暂停时仍有界报错，不伪造 STOPPED，不隐式复位或重连。退出清理使用同一逻辑。日志增加 MI 异步记录，便于继续追踪间歇故障。协议依据：[GDB 的异步执行与线程状态查询](https://sourceware.org/gdb/current/onlinedocs/gdb.html/Asynchronous-and-non_002dstop-modes.html)。

| 验证 | 结果 |
|---|---|
| 受控旧版复现 | 缺失停止通知时复现 `Timed out waiting for target to stop` |
| 5 个异常分支 | 缺失通知、目标已停止、通知延迟、首次 interrupt 未生效均可在原连接恢复并继续单步；真正持续运行时仍正确超时 |
| 安装版 STM32 循环 | 200 轮断点/next/continue/pause 全部通过；重连次数 0；最大 Pause 响应 141 ms |
| 实板完整回归 | 断点、观察点、内存、反汇编、帧切换、复位、重连、run、错误清理均通过，无重烧写 |
| 通用环境 | ARM/RISC-V/x64 模拟通过；真实本机 MinGW GDB 回归通过 |
| 编译检查 | 23 项单元测试通过，Clippy 全目标零警告，Release 构建通过 |

证据：旧版 `artifacts/pause-20260916-210354/`；安装版模拟 `artifacts/pause-20260916-211048/`；200 轮实板 `artifacts/pause-hardware-2026-09-16T13-10-40-349Z/`；完整实板 `artifacts/hardware-20260916-211207/`。

复现：`scripts/test-pause.ps1`；`node scripts/test-pause-hardware.cjs <ELF> [EXE] [循环次数]`。脚本仅用于开发测试，未加入运行时分发。发生现场问题时，可用 `debugtui --project . --log-dir ./debug-logs` 保存 MI 日志。

## 0.3.0：工作台布局与鼠标交互

日期：2026-09-16。本轮编译并安装到本机全局 npm 入口，未发布远端 Release。

| 验证 | 结果 |
|---|---|
| Rust / Clippy | 23 项单元测试通过；全目标 Clippy 零警告；Release 构建通过 |
| 布局 | 45×12、80×24、120×36、180×50 渲染通过；右侧五个标签切换保留源码，无底部重复调用栈 |
| 鼠标 | 工具栏映射、灰色按钮、CommandList 点击、右侧栈帧选择、源码滚轮与滚动条点击/拖动通过 |
| 异步视图 | Asm 打开自动加载，停止后刷新；等待复位等前台命令完成后再查询；失败显示错误且不循环重试 |
| 真实终端 | 安装版通过 ConPTY 点击 Asm、Step、Reset、Run、Reconnect；拖动滚动条可从 tasks.c 首部到末尾 5310 行；CommandList 弹窗可点击打开 |
| STM32 实板 | 安装版完成断点、源码/指令单步、观察点、内存、反汇编、栈、暂停、复位、run、错误清理；显式断开/连接和 reconnect 动作均通过 |
| 汇编显示 | 确认实际 GDB 指令非空，制表符展开为可显示的空格；复位后显示 Reset_Handler 的指令，已消除旧指令短暂回填与重复查询 |
| 通用环境 | ARM / RISC-V / x64 MI 模拟，以及通用服务启动与清理通过 |
| npm | 打包不含 tools，安装/升级、CMD/PowerShell 入口、配置保留、隔离卸载/重装通过；全局安装版本为 0.3.0，EXE SHA256 与构建产物相同 |

原生 EXE 为 1,431,552 字节，比 0.2.0 增加 8 KiB；没有新增运行依赖。本轮未重新测量内存占用。

测试未重新烧写固件。使用现有 `FreeRTOS_Project/Debug/FreeRTOS_Project.elf` 和 STM32F429IG/J-Link 配置。实板日志：`artifacts/hardware-20260916-203738/`；终端 MI 日志、渲染预览：`artifacts/ui-0.3.0/`；npm 验证：`artifacts/npm test 工程 20260916-203708/`。这些生成文件均忽略入库。

复现新增布局预览：设置 `DEBUGTUI_RENDER_DIR` 为输出目录，再执行 `scripts/build.ps1 -Test`。`scripts/test-hardware.ps1` 新增了 reconnect 动作以及反汇编非空/无制表符断言。

## 0.2.0：无参数启动与工程选择

日期：2026-09-16。在环境解耦基础上增加启动配置页，仍使用原生 Rust/Ratatui，无新增运行依赖。

- 16 项单元测试及 Clippy 零警告通过。新增覆盖：CLI 参数值不误判为开关、文件浏览/字段编辑/启动动作、项目相对路径保存、切换工程不携带旧配置、保留会话断点和监视、外部修改冲突检测、中文编辑，以及 45×12 / 80×24 / 120×36 / 180×50 配置页渲染。
- 在真实 ConPTY 终端无参数启动，从空目录打开配置页；键盘选择工程 A，F2 浏览并选择工程目录，自动发现工程内 tools 配置，F5 保存并连接真实 MinGW GDB。运行后停在 main，表达式 counter 返回 1。
- 在同一 TUI 中通过 F2 切换工程 B，旧 GDB PID 22544 退出，新 GDB PID 15888 启动；运行后 main 处 counter 返回 42。最终 Ctrl+Q 退出码 0，无残留调试子进程。
- 失败恢复：本机 MinGW GDB 14.2 对位于含中文路径的测试 EXE 返回 No such file or directory；TUI 显示错误并保留操作能力。通过 F2 修改为可加载的 EXE 路径后连接成功。这是已观察到的该 GDB 路径兼容性限制，不能把配置页支持中文路径等同于所有 GDB 都支持中文程序路径。
- 原有严格 ARM/RISC-V/x86 模拟、真实本机 GDB、STM32/J-Link 实板核心场景、外部 BAT 服务接入、强制终止清理及重连均回归通过。
- npm 安装版直接执行 debugtui，无启动参数时进入配置页，确认没有启动 GDB/Server；在页面选择 STM32 工程后连接实板，p/x g_w25q_jedec_id 返回 0xef4018，F10 源码单步成功，Ctrl+Q 正常清理。参数启动 debugtui --project 工程B 则直接连接到 READY，保持两种入口可用。
- 本轮 npm 安装、升级替换、卸载与重装、便携 ZIP 校验及渲染通过；构建取消 36 ms 退出，未遗留调试进程。

本轮 Release EXE 为 1,423,360 字节（约 1.36 MiB），较前一轮环境解耦构建增加 80 KiB。运行内存未在本轮重新测量。

真实会话日志位于 artifacts/launch ui 工程 20260916-121245/A/logs 和 B/logs。其他回归批次为 environment test 工程 20260916-121306、native-gdb-20260916-121310、hardware-20260916-121502、lifecycle-20260916-121510。配置页操作说明见 [使用指南](USER_GUIDE.md#首次配置与启动)。

## 0.2.0：环境解耦验证

日期：2026-09-16。以下为 Windows x64 版本 0.2.0 的发布前验证。TUI 包与 tools 包已经分离；发行渠道采用 GitHub Release，npm Registry 尚未发布。

| 项目 | 结果 |
|---|---|
| Rust 检查 | 11 项单元测试、Clippy 全目标零警告、Release 构建通过；没有新增第三方 crate |
| 无 tools 运行 | 将 EXE 单独复制到包含中文和空格的目录，严格 MI 模拟会话通过 |
| 目标架构 | ARM、RISC-V、x86-64 三组模拟寄存器全部显示，未发送隐式 monitor/reset/download 命令 |
| 默认 GDB 行为 | local 连接进入 READY；运行使用 -exec-run；未配置 restart/download 时明确拒绝 |
| 服务配置 | 自定义命令可启动；stderr 上无换行的就绪标记能识别；仅清理本程序创建的服务 |
| 真实本机 GDB | MinGW GDB 14.2：启动前设 main 断点、run、表达式、step、x86 寄存器、反汇编、继续和清理均通过；未使用 tools 配置 |
| STM32/J-Link | 通过 tools/debug-env.toml 完成核心调试、源码/指令单步、硬件观察点、内存、栈、reset、run、错误清理及重连 |
| 生命周期 | 外部 BAT 启动服务后接入成功；占用失败保留外部进程；强制结束 TUI 后清理自身子进程；重新连接恢复成功 |
| 构建取消 | 30 秒模拟构建收到 quit 后 15 ms 退出并清理子进程 |
| 分开安装后实板验证 | npm 安装版 TUI + 单独解压到中文/空格目录的 tools，完成全部硬件场景及下载；compare-sections -r 的 6 个只读段全部 matched |
| npm / ZIP | 不含 tools；本地安装、模拟旧版替换、CMD/PowerShell 入口、用户配置保留、卸载及重装通过；便携 ZIP 校验和、版本与渲染通过 |
| 安装版原生程序 | npm 安装版再次通过真实本机 GDB 流程，证明不依赖仓库旁的 tools |

环境解耦首次构建的原生 EXE 为 1,341,440 字节，约 1.28 MiB；当时含文档和许可证的 npm 包约 0.72 MiB，便携 ZIP 约 0.84 MiB。可选 STM32/J-Link 工具 ZIP 约 14.1 MiB，单独打包、单独更新。加入启动配置页后的大小见上方新记录；下方运行资源测量属于历史 0.1.0。

### 复现本次验证

```powershell
.\scripts\build.ps1 -Test
.\scripts\build.ps1 -Release
.\scripts\test-gdb-environment.ps1
.\scripts\test-native-gdb.ps1 -Gdb C:/MinGW/bin/gdb.exe -Compiler C:/MinGW/bin/gcc.exe
.\scripts\test-hardware.ps1 -Elf path/to/FreeRTOS_Project.elf -Download
.\scripts\test-lifecycle.ps1 -Elf path/to/FreeRTOS_Project.elf
.\scripts\test-build-cancel.ps1
.\scripts\release-assets.ps1 -SkipBuild
.\scripts\test-npm.ps1
.\tools\package.ps1
```

硬件测试脚本从 tools 配置获取服务及连接参数。它专门检查现有 FreeRTOS 固件的符号；不是对任意芯片的通用验收脚本。脚本支持用 -Binary 和 -Tools 分别指定安装版 EXE 与外部工具目录。

本次原始请求、事件、stderr 和 MI 日志保存在忽略入库的 artifacts/ 中，主要批次为 environment test 工程 20260916-094245、native-gdb-20260916-094721、hardware-20260916-094710、lifecycle-20260916-094251、npm test 工程 20260916-094613。

### 当前验证边界

- RISC-V 使用严格 MI 模拟验证，没有连接 RISC-V 实板；OpenOCD 配置是接入示例，本次未执行 OpenOCD 实板测试。
- 已验证 Windows x64 宿主；未验证 Linux/macOS 分发或进程清理。
- 环境解耦阶段 UI 检查覆盖单元测试中的多种终端尺寸及安装包快照渲染；上方记录补充了 0.2.0 启动配置页真实终端操作，下方保留 0.1.0 历史记录。
- detach/resume/disconnect 的最终目标行为受 GDB Server 控制；不会保证跨服务的“退出后保持暂停”。J-Link 的 monitor go 已移到工具配置。
- npm 替换测试使用复用当前 EXE 的 0.0.0-fixture；未在公共 Registry 发布或验证从真实 0.1.0 自动迁移项目配置。

---

# 历史记录：DebugTUI 0.1.0

以下结果保留用于历史对照，其中包内 tools、自动查找及退出策略描述不适用于 0.2.0。

日期：2026-09-16。结果针对 Windows x64、STM32F429IG、J-Link ARM V8（V7.94e Server）、SWD 4000 kHz，以及本机 FreeRTOS_Project 固件。测试使用真实探针和目标板。

## 结果

| 项目 | 结果与证据 |
|---|---|
| Rust 单元测试 | 10 项通过：MI 嵌套/重复字段/转义/异步停止、配置校验与缺省偏好保存、Unicode 输入、4 种终端尺寸、控制台结果保留 |
| Clippy | `cargo clippy --all-targets --locked -- -D warnings` 通过 |
| 发布构建 | `cargo build --release --locked` 通过；原生 EXE 1,323,520 字节 |
| 真实终端 | ConPTY 80×24；连接实板，F5/F6 继续/暂停、F9 断点切换、F10 单步、GDB 表达式、Tab 切换、Ctrl+Q 清理退出通过；命令面板和 Esc 关闭在 demo 中通过；最终 npm 入口在工程目录之外启动并自动找到包内 tools 通过 |
| 普通调试 | 核心 25 项请求及自动退出通过，见下表 |
| 执行控制补充 | `next`、`finish` 返回 Task100ms、控制台 `p/x`、`run` 复位后执行到 main 通过 |
| 下载 | ELF 写入、复位；`compare-sections -r` 的 6 个只读段全部 matched |
| 失败与重连 | 无效表达式使脚本返回非零且跳过后续请求；自动退出清理成功；同一进程断开/重连成功 |
| 外部 Server | 先运行 `tools/start_server.bat`，再 `--connect 127.0.0.1:3333` 成功 |
| 进程所有权 | 探针/端口占用失败不结束外部 Server；强制终止测试 TUI 后，其两个 GDB/Server 子进程均结束；新会话恢复成功 |
| 输入错误 | ELF 缺失、tools 缺失、服务拒绝连接均返回错误，无残留调试子进程 |
| 构建取消 | 模拟 30 秒构建，收到 quit 后 37 ms 退出并结束构建子进程 |
| npm | 本地 tgz 安装、模拟旧版替换为 0.1.0、CMD/PowerShell 入口、配置保留、卸载和重新安装通过 |
| 迁移 | npm 安装目录和 ELF 路径均包含中文与空格；使用安装包内 tools 的实板核心、错误及重连测试通过 |
| 依赖 | 安装后的 11 个 GDB/J-Link 运行资源及许可证哈希与锁文件一致；包不含 Python、Rust 开发缓存或 node_modules |

npm 升级使用 `0.0.0-fixture` 测试包验证替换机制，该 fixture 复用当前原生 EXE，不是一个历史软件版本。未向公共 npm registry 发布，包名 `@debugtui/cli` 尚待正式发布时确定。

## 核心实板请求与结果

| ID | 请求 | 实际结果 |
|---|---|---|
| 1 | connect | 连接成功，MI 异步支持 true |
| 2 | evaluate g_w25q_jedec_id | 15679512，即 0xef4018 |
| 3–5 | 临时断点 DEBUG_PRINTF，continue，wait_stopped | breakpoint-hit，bsp_log.c:51 |
| 6–7 | step，wait_stopped | bsp_log.c:53，PC 0x08000692 |
| 8–9 | stepi，wait_stopped | PC 0x08000694 |
| 10–12 | data_break Log_Tx_En，continue，wait_stopped | watchpoint-trigger，记录 1 → 0 |
| 13 | memory &Log_Tx_En，32 字节 | 返回 0x20000000 起的 32 字节 |
| 14 | disassemble | 返回当前 PC 附近指令 |
| 15 | files | 返回 ELF 调试信息中的源码文件列表 |
| 16–17 | frame 1，frame 0 | 调用栈切换成功 |
| 18 | delete_break（全部） | 会话断点删除成功 |
| 19–20 | continue，pause | 运行中 MI interrupt 成功暂停 |
| 21 | restart | 复位并暂停 |
| 22–24 | 临时断点 main，continue，wait_stopped | main.c:79 |
| 25 | status | STOPPED 快照 |
| 自动退出 | quit | 恢复运行、detach、GDB/Server 结束，退出码 0 |

## 资源测量

真实终端、板子暂停、已有符号与变量快照，采样 5 秒。以下是单次本机测量，不是所有工程的性能承诺。

| 进程 | 私有内存 | 工作集 | 5 秒内 CPU 时间 |
|---|---:|---:|---:|
| debugtui.exe | 2.52 MiB | 8.24 MiB | 15.62 ms |
| arm-none-eabi-gdb.exe | 26.91 MiB | 37.78 MiB | 15.62 ms |
| JLinkGDBServerCL.exe | 56.58 MiB | 58.20 MiB | 156.25 ms |

测量使用最终 npm 安装版，两个监视表达式，包含实际调试操作后的有界 Console 缓冲。运行进程只有原生 TUI、GDB 和 Server，Node 不参与持续调试。工具集约 32 MiB，原生 TUI 约 1.26 MiB，附带完整许可证与文档后 npm 包约 14.7 MiB、解压约 35.6 MiB；精确大小见本机 `artifacts/npm-pack.json`。

原生 EXE 的 PE 导入只有 Windows 系统 DLL：KERNEL32、msvcrt、ntdll、USER32、api-ms-win-core-synch-l1-2-0。无需分发 GCC 或 Rust 运行库 DLL。

## 已修复的问题

1. J-Link Server 就绪提示无换行，按行读取会一直等到超时：改为增量读取并识别就绪文本。
2. 本工具集 GDB 把带引号的 `-target-select` 地址当串口文件：主机名校验后用未加引号的地址参数。
3. 初始 TOML 缺少 watch/breakpoints 字段时保存偏好触发异常：改为表键插入并加入回归测试。
4. 后台异常退出曾可能让 headless 返回成功：现在缺少正常结束事件即返回失败。
5. 极快的停止事件可能被继续命令覆盖成 RUNNING：增加停止代次检查。
6. 进程被强制结束可能留下 GDB/Server：加入 Windows Job Object，仅管理本程序创建的进程。
7. 大量 Server 读寄存器日志会顶掉 GDB 表达式结果：底部 Console 独立保留命令输出。
8. 功能名断点在源码处按 F9 可能重复添加：按文件和行号识别已有断点。
9. 长时间构建期间退出响应慢：退出信号取消构建并清理子进程。
10. 原型的 on_exit=halt 无法保证 J-Link 退出后保持暂停：明确拒绝该值，正常退出使用 resume。

## 复现

```powershell
.\scripts\build.ps1 -Test
.\scripts\build.ps1 -Release
.\scripts\test-hardware.ps1 -Elf path\to\FreeRTOS_Project.elf -Download
.\scripts\test-lifecycle.ps1 -Elf path\to\FreeRTOS_Project.elf
.\scripts\test-build-cancel.ps1
.\scripts\generate-notices.ps1
.\scripts\package.ps1 -SkipBuild
.\scripts\test-npm.ps1
```

`artifacts/` 保存每次测试的请求 JSONL、全部事件输出、stderr、MI/Server 日志，以及安装包清单和性能测量。原始日志不打进 npm 包。

## 验证边界

- 当前仅验证 Windows x64 和上述 J-Link/STM32 组合；OpenOCD 不在这个最小工具包内。
- 没有做探针物理拔插、休眠恢复、不同板卡或多小时稳定性试验；不把进程终止测试当作 USB 拔插测试。
- 通用变量树、FreeRTOS 专项面板、源码编辑器、多人共享会话尚未实现。
- npm Registry 发布、其他 npm 版本和其他操作系统未验证；本地安装、替换、卸载及程序调试已验证。
- `session.on_exit=resume` 是当前后端支持的退出策略。强制终止后目标状态需要重新连接确认。

## 0.5 SVD 验证

- 单元测试覆盖 STM32F429 的 84 个外设、GPIOB 继承、数组/cluster 展开、位域、大小端和读取副作用。
- UI 通道测试检查收起、隐藏、运行时不读取；展开后只读可见行；同一停止代次不重复读取；手动单项刷新、错误恢复、鼠标命中、滚动条及三档窗口布局。
- 配置页测试覆盖 SVD 文件选择、相对路径保存、清空和无效路径提示。
- `node scripts/test-svd-gdb.cjs --native`：真实本机 GDB 验证 8/16/32 位读取、值变化、错误恢复、运行中拒绝读取，并确认不会覆盖 Memory 页数据。
- `node scripts/test-svd-gdb.cjs --hardware`：使用 tools 配置连接 STM32F429，对比 RCC.CR、GPIOB.MODER、DBGMCU.IDCODE 的外设读取与直接 GDB 表达式结果；不下载或复位固件。
- 其他芯片的 SVD 和硬件行为仍需相应设备验证。自动读取的副作用判断以 SVD 声明为准。

## 0.5.1 输入与补全验证

- UI 测试覆盖输入防抖、候选键盘/鼠标选择、两处输入隔离、添加 Watch 成功/失败、重复回车、F10 保留、过时响应丢弃、运行中不查询、弹窗遮挡和 45×12 至 160×42 的布局。
- `node scripts/test-completion-gdb.cjs`：使用真实本机 GDB，验证命令/子命令/参数补全、全局与静态变量、排除函数名、结构体字段、64 项上限、Watch 添加和运行/暂停后的补全恢复；确认补全不改变变量值、PC 或停止代次。
- `node scripts/test-completion-gdb.cjs target/release/debugtui.exe PATH/FreeRTOS_Project.elf`：使用 tools 中 ARM GDB 离线加载 ELF，验证 `uxCurrentNumberOfTasks`、`xTickCount`、`p/x` 参数等候选，无需连接开发板。
- 彩色 UI 缓冲区可通过 `DEBUGTUI_RENDER_DIR` 环境变量导出。测试记录在 `artifacts/completion-*`，不进入安装包。

## 0.6.0 effects, warm theme and per-item radix

- 61 unit tests plus the real shell-task integration test cover configuration merging, independent variable/register/field/byte formats, exact signed/unsigned 128-bit conversion, unsupported natural values, keyboard and right-click routing, popup clipping and input focus.
- Effects tests verify actual connection phases, confirmed stop events, real source traces, bounded caches, idle scheduling, focus loss, Off mode and persistent task outcomes. Color-buffer previews cover 45×12, 80×24 and wide layouts, numeric/appearance menus and sampled animation frames.
- Native GDB integration saves all three motion modes and individual formats: zero additional MI commands, unchanged watched value, PC, generation and STOPPED state. FreeRTOS ARM ELF symbol completion is also exercised offline; the SVD native-memory read suite is rerun. These checks do not certify a physical R52/STM32 target.
- Real terminal smoke: connect native GDB, breakpoint main, run to breakpoint, switch register format from hex to decimal, use Appearance and exit cleanly. A fast Tab→f race discovered here was fixed by deriving the keyboard selection from current data before the next draw.
- Runtime sample (native GDB stopped, Full mode, one terminal): TUI working set 7.46 MiB, private bytes 1.73 MiB, CPU 0.03125 seconds over 3.046 seconds. This is a local sample, not a performance guarantee.
- npm fixture checks install, upgrade, CMD/PowerShell shims, configuration preservation, isolated uninstall and reinstall.

Artifacts: `artifacts/ui-0.6.0/`, `artifacts/completion-native-1789607444647/`, `artifacts/completion-arm-elf-1789607447807/`, `artifacts/svd-native-1789607448656/`.

## 0.6.1 Console 历史与会话时间戳

- `cargo test --locked`：67 项单元测试 + 1 项实际 shell 集成测试通过；`cargo clippy --locked --all-targets -- -D warnings` 通过。
- Console 新增测试覆盖滚轮、轨道点击、滑块拖动至两端、历史期间追加输出、2,000 行缓存淘汰后的记录锚点、窄窗口缩放、弹窗隔离、Latest / End 恢复跟随、输入草稿保留和实际命令分发。浏览历史不发送调试请求。
- `scripts/test-logs-gdb.cjs` 使用真实本机 GDB 与独立 C 测试程序，执行断点、Run、Next、错误命令、多行 printf、Reconnect、Disconnect / Connect。14 个请求生成 3 份日志（114 / 24 / 24 行），检查旧文件字节不变、全部物理行带时间戳、161 条 log 事件携带时间字段且原始文本保持不变。记录：`artifacts/logs-native-1789609065500/verification.json`。该测试不连接物理 MCU。
- 时间测试覆盖 Windows 时钟格式、UTC 闰日、同毫秒文件名碰撞不覆盖、毫秒会话时长和多行/空行逐行前缀。文件通过 `create_new` 排他创建。
- UI 预览由真实 Ratatui 缓冲导出：`artifacts/ui-0.6.1/console-live.png`、`console-history.png`、`console-history-narrow.png`。保留底部输入框，Console 右边是独立轨道，浏览状态与新消息计数在标题行。
- 本地安装 0.6.1 后，以真实终端连接本机 GDB：多行 printf 输出 35 条记录，Shift+PgUp / Home 浏览首条记录；保持历史视口执行新命令，Latest 增加 3 条记录且旧内容保留；End 恢复实时跟随，Ctrl+Q 退出码 0。日志位于 `artifacts/ui-0.6.1/terminal/`。原生命令/变量补全回归同样通过：`artifacts/completion-native-1789609102557/verification.json`。
- npm 独立安装、升级、卸载和重新安装均通过；本机安装的可执行文件 SHA-256 与 release 构建相同。

## 0.7.2 多核健壮性与单核回归（2026-09-18）

- 89 项 Rust 单元测试 + 1 项实际 shell 集成测试通过；Clippy `--all-targets -- -D warnings` 通过。新增覆盖 ELF32/64 正确偏移、截断/越界、TCL 完整响应/错误/超时、Live Watch 断线重连与 target-specific 读取、并发分核偏好保存、启动页合并和切核视图缓存失效。
- `scripts/test-multicore-gdb.cjs target/release/debugtui.exe`：两个真实本机 GDB，稳定核序号、按序连接、立即切换快照、不同 Watch/断点保存与恢复、Run、单步、重连、Build 全流程通过。另测第二核连接失败回滚、后台核 Run 失败上报、单个显式 core 的共享服务、服务启动失败清理，以及多核 Build 中通过 Console quit 退出取消。最终记录：`artifacts/multicore-1789740186149/verification.json`。
- 无 `[[cores]]` 的旧单核工程仍直接进入原 Session；真实 GDB connect/break/run/step/quit、Watch 增删与命令/符号补全通过。单核 Snapshot 不增加 core 字段。
- ARM / RISC-V / x64 的严格 MI 模拟服务通过：寄存器来自 GDB，不注入芯片命令；本地 READY、未配置动作报错、无换行 stderr 就绪消息和自有进程清理通过。
- Pause 五种情况通过：缺失停止事件、已停止错误、延迟事件、补发中断，以及目标确实仍运行时有界超时。脚本使用 PowerShell 7 (`pwsh`) 执行；Windows PowerShell 5 会将预期的 stderr 负例提前变成终止错误。
- 会话日志回归通过：14 个请求、3 份独立日志、全部物理行带时间戳、旧日志字节不变。记录：`artifacts/logs-native-1789739873974/verification.json`。
- 这些验证不连接物理 MCU。真实多核芯片的 CTI 联动、独立核暂停能力、AP 访问与缓存一致性仍需对应硬件验收；当前共用 ELF/GDB/SVD，不视为异构多镜像支持。

### 2026-09-19 补充回归与收尾

- 复现并修复：重复 Connect 原本进入失败回滚、拆掉现有多核连接；现在在派发前拒绝，READY/STOPPED 会话保持有效。
- 多核 `set_elf` 明确拒绝单核局部切换，引导通过 F2 Setup 更换共享 ELF 并整体启动。单核 `:elf` 保留；界面只在后端确认成功后更新路径，失败或发送失败保持原路径。
- 单核/多核首次 Run 前的断点新增、恢复及 Console 删除均验证持久化。退出前读取实际 GDB 断点列表，避免 READY 状态的变更遗漏；读取失败保留之前已保存列表。
- TCL 同步初始化在每条命令前检查退出取消；退出时不再执行余下配置命令。共享服务就绪标记跨读取块且包含空格、无换行时仍可识别；退出后验证自有服务 PID 已消失。
- 90 项单元测试 + 1 项实际 shell 集成测试、Clippy、release 构建通过。最终 release 双 GDB 脚本 9 组场景通过：`artifacts/multicore-1789819904726/verification.json`。
- 单核真实 GDB Watch/补全：`artifacts/completion-native-1789819906319/`；日志轮转/时间戳：`artifacts/logs-native-1789819909213/verification.json`。ARM/RISC-V/x64 模拟寄存器与五种 Pause 场景回归通过。

### 2026-09-19 Watch 补全、结构体与地址表达式

- 96 项 Rust 单元测试 + 1 项实际 shell 集成测试通过，Clippy `--all-targets -- -D warnings` 和 release 构建通过。树形 UI 新增回归：鼠标展开、Enter/左右键、成员独立进制、折叠后的选中项、数组分页、滚动后关闭顶层项、禁止从成员行误删其他观察项、运行时展开限制和窄窗口点击区域。
- `node scripts/test-watch-tree-gdb.cjs target/release/debugtui.exe`：真实本机 GDB 验证全局/静态符号、`.` / `->` / 嵌套成员和强转成员补全；结构体展开、数组 32/64/70 项分页、成员变化刷新；结构体/整数的地址强转和解引用；无效地址、未知类型、失效路径；256 节点与 8 层限制；折叠/删除零 MI，以及所有临时变量对象释放。记录：`artifacts/watch-tree-1789821399632/verification.json`。
- 修复 GDB 对不可读地址仍成功创建变量对象、却返回空 value 的边界情况：取得读取诊断并标记错误，避免显示为有效类型。折叠后重开数组从首批 32 项开始，避免一次读回所有历史分页。
- `scripts/test-completion-gdb.cjs` 原有单核补全、Watch 增删和零目标写入断言通过：`artifacts/completion-native-1789821345670/verification.json`。使用工程 `Debug/FreeRTOS_Project.elf` 和 tools 内 ARM GDB 的离线全局变量补全通过：`artifacts/completion-arm-elf-1789821348389/verification.json`。
- 双真实 GDB 的 10 组回归通过，新增同名结构体在两核上的数值（91/41）、展开状态与删除隔离验证：`artifacts/multicore-1789821345691/verification.json`。无 `[[cores]]` 的单核路径保留。
- ARM / RISC-V / x64 模拟 MI、通用服务启动/回收、五种 Pause 场景通过：`artifacts/environment test 工程 20260919-203549/`、`artifacts/pause-20260919-203556/`。
- 从实际 Ratatui 缓冲导出并检查 Watch 树预览：`artifacts/watch-tree-ui/watch-tree.png`、`watch-tree-compact.png`。本轮真实目标求值在本机测试程序中完成，ARM ELF 补全不连接硬件；未做物理 MCU 内存访问验证。
- npm 本地包重新安装成功，`debugtui --version` 为 0.7.2。安装目录中的 exe 与 release 构建 SHA-256 相同：`32EFDEC1FCE8D3FBE4708080C50D62012ECDE88B10829C913ACAC60395F0CD4B`；直接用已安装 exe 重跑 Watch 树测试通过：`artifacts/watch-tree-1789821508317/verification.json`。

PMU 软件与延后验收入口：生产 C 的 23 项编码、1,614 个传输失败点、190 项安全拒绝、208 项状态/scratch 变化及六个完整移动周期样本通过 Linux/Windows 模型。原生 Windows candidate 的 DLL/GPL 源码和真实 dummy 命令验证通过。会话/多核/取消/能力/驱动覆盖和本轮完整回归报告见 [PMU 证据](docs/register-pmu.md) 与开发进度。`node scripts/test-register-pmu-hardware.cjs` 默认五项 SKIPPED，十项 [环境用例](tests/cases/register-pmu.md) 未执行上板；固件只离线编译，不计为硬件通过。

新鲜MMIO Probe 的四项单元与五项真实worker/EXE/AP/独立RAM测试验证三组件身份/容量和共享epoch失效。[六项延后case](tests/cases/register-mmio-probe.md) 与 node scripts/test-register-mmio-probe-hardware.cjs 已准备；默认五阶段 SKIPPED、零目标I/O。21项只读GNU Arm固件仅离线编译，完整本轮软件报告及限制见 [MMIO Probe自检](docs/register-mmio-probe.md)。
