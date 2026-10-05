# DebugTUI 使用指南

本文按 **v0.9.3** 的实现整理，核对日期为 **2026-10-04**。快速了解“是什么、怎么用”见 [README](README.md)。DebugTUI 是基于 GDB/MI 的原生终端调试工作台；工具环境提供连接与芯片能力，工程配置提供程序、源码和调试策略。

**阅读入口：** [安装](#安装与升级) · [首次配置](#首次配置与启动) · [配置分层](#项目与工具配置) · [芯片与核心](#芯片与核心选择) · [常用操作](#常用调试操作) · [源码映射](#源码路径重映射) · [运行时刷新](#运行时刷新) · [多核](#多核工作区) · [构建下载](#构建与下载) · [排查问题](#常见问题)

工程地址：[bei-li16/DebugTUI](https://github.com/bei-li16/DebugTUI)。许可证为 [Apache-2.0](LICENSE)，第三方声明见 [NOTICE](NOTICE) 和 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)。

## 安装与升级

发行包面向 Windows x64。npm 用于安装/升级，实际运行的是原生 EXE，不需要 Node、Python 或 VS Code 常驻；便携 ZIP 方式无需安装 Node。目标调试仍需要兼容 GDB/MI2 的 GDB、调试服务器、探针及驱动，文件由工程/工具环境独立维护；本机编译另外需要工程工具链。

### npm 安装与升级

先安装 [Node.js（含 npm）](https://nodejs.org/en/download)。PowerShell 使用 `npm.cmd`，避免 `.ps1` 执行策略阻止 npm 启动：

```powershell
npm.cmd install -g --prefer-online "https://github.com/bei-li16/DebugTUI/releases/latest/download/debugtui-cli.tgz"
debugtui --version
```

该 URL 跟随 GitHub 最新正式 Release；升级时重新运行同一命令。离线安装可以下载版本化 tgz 后执行 `npm.cmd install -g ./debugtui-cli-0.9.3.tgz`。卸载使用 `npm.cmd uninstall -g @debugtui/cli`；用户芯片目录和工程配置保留。

### 便携分发与工具边界

[v0.9.3 Release](https://github.com/bei-li16/DebugTUI/releases/tag/v0.9.3) 提供 `debugtui-0.9.3-win-x64.zip`、独立 EXE、npm tgz 和 `SHA256SUMS.txt`。解压 ZIP 后运行 `debugtui.exe`；ZIP/npm 包同时提供配置模板和说明。GDB、OpenOCD/J-Link、编译器、驱动、SVD 及芯片下载算法不在这个应用包中，必须另行准备。

`debugtui --demo` 可离线预览界面；`debugtui --snapshot ./preview.txt` 输出演示文本，不连接板卡。安装后命令找不到或版本不符时，用 `Get-Command debugtui -All` 检查 PATH 中是否存在多份程序，重新打开终端后再核对 `--version`。

## 首次配置与启动

### 最小远程调试配置

已有调试服务器时，在工程根目录放置以下两个文件。先确保服务器已按目标板配置启动、板上已下载与 ELF 对应的固件，再替换工具路径、ELF 和端口。

`debug-env.toml`（工具环境；这个最小示例不负责启动服务）：

```toml
[gdb]
executable = "C:/toolchain/bin/arm-none-eabi-gdb.exe"

[target]
mode = "extended-remote"
endpoint = "127.0.0.1:3333"
```

`debug.toml`（工程）：

```toml
version = 2

[tools]
profile = "./debug-env.toml"

[program]
elf = "./build/app.elf"
source_root = "."

[session]
on_exit = "detach"
log_dir = "debug_log"
```

这是 Legacy 单核示例；`version = 2` 无需选择 Chip。THA6 工程可直接使用 [芯片与核心选择](#芯片与核心选择) 中的成套环境。若需要 DebugTUI 自动启动服务器，在 profile 中配置 `[service]`，见下一节。

### Setup 与启动参数

安装后直接输入名称即可打开终端内的启动配置页：

~~~powershell
debugtui
~~~

无需记忆启动参数。在页面中选择工程目录或 debug.toml、工具环境 TOML、ELF/本机程序和源码目录，配置构建/下载命令及退出策略。GDB、目标连接、服务启动和工具超时在所引用的 debug-env.toml 中维护；Setup 只编辑工程配置。

| 配置页操作 | 按键 |
|---|---|
| 选择字段 | 默认选中 Project；↑ ↓ 只在可用配置项之间循环，跳过禁用项和顶部按钮。鼠标可直接选择；Tab / Shift+Tab 可遍历字段及按钮 |
| 编辑路径或参数 | Enter；Ctrl+U 清空；Enter 应用，Esc 取消 |
| 浏览文件/目录 | F2；Enter 进入目录或选择文件；Space 选择当前目录；Backspace 返回上层 |
| 选择已有工程 TOML | 顶部 Projects / F3；选择后立即刷新配置页各字段。Project 的 F2 浏览保留，文件列表只显示目录和 TOML |
| 套用示例配置 | 顶部 Examples / F4；预览后按 Enter 或点击套用，再修改路径和连接参数 |
| 切换枚举/开关 | ← → / Enter |
| 保存配置 | 顶部 Save / Ctrl+S |
| 开始调试 | 点击顶部 Start debugging，或使用 Ctrl+R、Ctrl+Enter、F5；Tab 选中该按钮后也可按 Enter 启动。启动前保存到工程的 debug.toml；Save config / Ctrl+S 可单独保存而不启动调试 |
| 从调试页面返回配置 | 主界面 Project 栏的 ← Setup / F2 / :setup |
| 回到原调试页面 | 配置页顶部 ← Workspace / Esc；编辑字段或浏览文件时 Esc 先取消当前操作 |
| 退出应用 | 配置页顶部 Exit / Ctrl+Q；配置未完成、编辑值无效或正在浏览时也可退出，不保存草稿 |

无参数启动始终先显示配置页，即使当前目录已有 debug.toml；不会直接占用探针。选择工程后优先读取其配置；没有显式 tools/GDB 配置时，会发现该工程内的 tools/debug-env.toml。保存路径尽量相对于工程，保留构建、源码映射、监视与断点等原有配置，不展开并复制整份 tools 配置。

Project 默认显示 `./debug.toml`，相对路径的输入和显示均以启动 DebugTUI 的目录为基准；ELF、Source root、SVD 等资源路径仍以所选工程 TOML 的目录为基准。跨盘无法表达相对路径时保留绝对路径。没有默认文件但发现其他工程 TOML 时，先显示选择列表；没有工程 TOML 时保留未保存的默认草稿。Projects 排除 Cargo 等无关配置及 debug-env 工具配置；其他文件仍可通过 F2 手动浏览。

Examples 提供单核、本机程序和双核工程模板；工程存在 `debug-env.toml`、`.vscode/debug-env.toml` 或 `tools/debug-env.toml` 时，还可选择引用已有工具配置。模板只填入 ELF、源码目录、日志、退出策略及可选核心映射，不再生成 GDB、target、service 或工具超时配置。应用模板会替换工程草稿，但保留已选择的工具 profile 和旧工程内嵌的 gdb/target/service/timeout 覆盖项；选择另一个 profile 示例时更新引用。请核对工程路径，并通过 Tools / profile 选择适用的工具环境；本机程序需要环境配置 `target.mode='local'`。示例不会自动烧录、复位、启动工具或写文件；只有点击 Save config / Ctrl+S 或启动调试时，才写入 Project。双核示例的逐核地址在 `[[cores]]` 中编辑，板级复位动作由实际环境提供。

配置页顶部固定显示 Start、Save、Projects、Examples、Workspace 和 Exit 按钮，不随字段滚动，窄终端会换行。字段下方说明包含用途、路径基准、示例、是否可留空及相关限制。打开配置页时当前调试会话仍然有效；返回工作区会保留未保存的配置草稿。点击 Start debugging 时先校验并应用正在编辑的字段，再清理原会话、启动新配置；旧会话清理失败会在界面报错并停止切换。连接失败可以通过 ← Setup 修正配置并重试。Exit / Ctrl+Q 通过原有退出流程关闭会话和自有服务。

也可以使用命令行参数：配置完整时直接准备调试环境；参数不足时进入已填好参数的配置页。添加 --setup 可强制先查看配置。--project 同时接受工程目录和配置文件。

~~~powershell
debugtui --project ./工程目录
debugtui --project ./工程目录 --setup
~~~

发行包包含程序、配置模板、说明和许可证，不包含 GDB、调试服务、SVD 或编译器。需要用户提供与目标架构匹配、支持 GDB/MI2 的 GDB。GDB 自身需要的 DLL、资源和服务由对应环境维护。

连接已经启动的调试服务：

~~~powershell
debugtui --gdb C:/toolchain/bin/arm-none-eabi-gdb.exe --connect localhost:3333 --elf ./build/app.elf
~~~

调试本机程序：

~~~powershell
debugtui --gdb C:/MinGW/bin/gdb.exe --local --elf ./build/app.exe
~~~

使用 tools 中的环境配置，使用该环境声明的服务启动与连接流程：

~~~powershell
debugtui --tools-dir ./tools --elf ./build/app.elf
# 等价的显式配置入口
debugtui --environment ./tools/debug-env.toml --elf ./build/app.elf
~~~

先用 tools/start_server.bat 启动服务后，可跳过 TUI 的服务启动：

~~~powershell
debugtui --environment ./tools/debug-env.toml --connect localhost:3333 --elf ./build/app.elf
~~~

--connect 只禁用配置中的服务启动，保留该环境声明的 GDB 参数和连接动作。完全通用的连接不要加载芯片配置，只传 --gdb 和 --connect。未指定 --gdb 或环境时，从继承的 PATH 查找 gdb；不会自动搜索包内 tools。

--target-mode 支持 remote、extended-remote、local。--gdb-arg 可以重复使用。符号文件可省略；缺少符号时，源码和变量功能受 GDB 提供的信息限制。未配置连接目标时，在配置页的 Tools / profile 选择环境；GDB 参数、连接方式、端口、服务和工具超时在 debug-env.toml 中编辑。GDB 参数使用 TOML 字符串数组；工程专用启动动作继续放在工程 TOML 中。

~~~powershell
debugtui --demo
debugtui --snapshot ./preview.txt
~~~

## 项目与工具配置

`debug.toml` 描述当前工程调试哪个程序、如何构建/下载，以及该工程的调试偏好；`debug-env.toml` 描述使用哪些调试工具、如何启动它们和建立连接。按字段的实际职责分层，便于多个工程复用同一套 GDB / OpenOCD 配置。

| 配置 | 职责 |
|---|---|
| 项目 debug.toml | ELF、源码根目录/映射、SVD、构建/固件下载命令、Watch/断点、使用哪些核心、组控制策略、退出策略、日志目录、UI 偏好、环境引用 |
| tools/debug-env.toml | GDB/OpenOCD 路径、启动参数、工作目录/环境变量、服务就绪条件、连接方式/端口、工具通信超时、通用探针/芯片初始化命令 |
| tools/examples/ | 外部 OpenOCD、RISC-V 环境模板；需按实际工具链配置 |

项目配置示例：

~~~toml
version = 2
watch = ["counter"]
breakpoints = ["main"]

[tools]
profile = "./tools/debug-env.toml"

[program]
elf = "./build/app.elf"
source_root = "."

[session]
on_exit = "detach"
log_dir = "./debug_log"
~~~

也可以不引用独立的工具 profile，参考 [debug.toml.example](debug.toml.example) 在项目内配置 GDB。

以下工具字段在 profile 文件中维护；Setup 保留工程的 On exit 选项：

| 配置项 | 配置键 | 归属文件 |
| --- | --- | --- |
| GDB executable | `[gdb].executable` | debug-env.toml |
| GDB arguments | `[gdb].args` | debug-env.toml |
| Target mode | `[target].mode` | debug-env.toml |
| Endpoint | `[target].endpoint` | debug-env.toml |
| Start service | `[service].enabled` | debug-env.toml；同处配置 command/args 等 |
| Timeout (ms) | `[session].timeout_ms` | debug-env.toml；这是 GDB 命令超时 |
| On exit | `[session].on_exit` | debug.toml；这是当前工程退出后的运行策略 |

现有格式把 GDB 命令超时命名为 `[session].timeout_ms`，仍按工具通信参数归属环境文件；不要自行改成当前解析器不支持的 `[gdb].timeout_ms`。它与 `[service].timeout_ms` 的服务启动超时各自独立。Legacy 多核项目用 `[[cores]].endpoint` 声明端口；Chip 模式优先取同名模板的显式端口，否则取 profile 的 `core_targets`。

命令数组也按内容区分：通用 OpenOCD/探针初始化或芯片复位可放环境文件；包含工程入口、链接符号、启动同步变量或固件路径的动作放项目文件。例如 Bao 的 `_barrier` 清零、停在 `init`，以及 MCAL 的 `_main` 启动断点属于相应工程；构建和烧录哪个 ELF/HEX 的命令同样属于工程。

环境文件接受 `gdb`、`target`、`service`、`actions`、`session`、`sync`、`memory_access`、`multicore`、`core_targets`，以及 `backend` / `backends` 后端选择配置。`program`、`tasks`、`source_map`、`live_watch` 等工程字段应放在项目 TOML；旧环境中的退出策略仍可继承。加载顺序为环境 → 项目字段覆盖 → CLI 参数；数组整体替换，空数组可以清除继承动作。相对可执行路径（含 / 或 \）和 cwd 相对定义它们的 TOML 文件，裸命令名通过 PATH 查找。环境文件中的 `${profile_dir}` 展开为该环境文件所在目录，可用于资源路径参数。

**Setup 只编辑工程配置**：页面保留 Project、Tools / profile、Chip、Debug cores、Program / ELF、Source root、Build command、Download command、On exit、Log directory、SVD file、Source remap 和 ELF path prefix。Tools / profile 只修改工程的环境引用，不编辑工具文件。Chip / Debug cores 用于选择芯片和核心，Legacy 模式仍兼容旧配置；顶部提示当前单核或配置的核心名称。Chip 模式的逐核端口通常由工具 profile 提供，工程可用 `[[cores]]` 模板维护 init、after_connect、run、startup_order、Watch/断点及 multicore 策略；旧项目中的逐核 endpoint 仍保留兼容。

保存时仅修改工程草稿中的对应字段，不把合并后的工具默认值展开写入工程，也不改写共享 profile。兼容无 `[[cores]]` 的普通单核、仅列出 core0/core1 的单核，以及多核配置。旧工程内嵌的 gdb/target/service/timeout 等覆盖项继续有效且原样保留其值；要完成文件分层，应手动迁移到 debug-env.toml，再删除工程对应的覆盖键。Setup 不自动迁移，以免改变既有连接行为。调试会话在后台保存的逐核 Watch/断点也会合并保留。

`On exit` 对应工程文件中的 `[session].on_exit`，默认值为 `detach`：

~~~toml
[session]
on_exit = "resume"  # detach / resume / disconnect
~~~

| 值 | DebugTUI 的实际退出动作 |
| --- | --- |
| `detach`（默认） | 向 GDB 发送 `-target-detach`，然后退出 GDB；不额外发送 continue。目标是否自动恢复取决于调试后端的 detach 语义。 |
| `resume` | 远程调试先发送 `-exec-continue`，再 `-target-disconnect` 并退出 GDB；本机调试使用暂停状态下的 `-target-detach`，由本机 GDB 的 detach 恢复程序。 |
| `disconnect` | 发送 `-target-disconnect`，然后退出 GDB；DebugTUI 不主动恢复目标。 |

已有运行中的目标在清理前会先尝试暂停；进入 STOPPED 后删除本次调试断点并执行 `[actions].before_disconnect`，随后执行上述策略。Watch/断点配置会保存供下次使用，删除目标断点不等于删除工程里的断点列表。最终状态还受 before_disconnect、GDB、OpenOCD/探针和板级事件影响，因此 `detach` 不能等同于“保证停核”，`resume` 也不是芯片复位或重新烧录。

策略适用于 Exit / Ctrl+Q、`:disconnect`、Reconnect，以及 Setup Start 替换旧会话、外部 Build/Download 任务释放旧连接的阶段。只打开 Setup、保存草稿或返回 Workspace 不会触发断开。Setup 新改的策略用于新会话；替换时旧会话仍按它原先加载的策略清理。

多核时同一策略应用于所有配置的核心，与当前活动核心及 Scope Core/All 无关；仅配置 core1 的工程只管理 core1。各核顺序执行，不能理解为硬件同步释放。结束后 DebugTUI 关闭自己的 GDB 和自己启动的服务；外部启动的 OpenOCD 不由它终止。清理失败会报告错误，需结合日志判断目标最终状态。

~~~toml
[gdb]
executable = "./bin/custom-gdb.exe"
args = ["--data-directory=${profile_dir}/share/gdb"]
init = ["set remotetimeout 5"]

[target]
mode = "remote"
endpoint = "localhost:3333"
after_connect = []

[session]
timeout_ms = 8000 # Existing schema name for GDB command timeout.
~~~

GDB 和 service 均可配置 args、cwd、env、unset_env。TUI 默认继承环境，仅设置 LC_ALL=C；具体工具所需的环境变量调整由配置声明。

可选 service 表支持 command、args、cwd、env、unset_env、ready、timeout_ms、enabled。ready 是就绪日志片段列表，匹配 stdout 或 stderr；为空表示启动后直接尝试 GDB 连接。仅管理本程序启动的进程。项目可以用 [service] enabled=false 使用外部服务。

actions 支持 restart、run、download、before_disconnect；target.after_connect 和 gdb.init 也是命令数组。以 - 开头的命令按 MI 发送，其余按 GDB 控制台命令发送。不执行 shell 拼接。restart/download 未配置时不可用；run 未配置时使用标准 -exec-run。任一动作失败立即停止后续动作。

### 路径与格式版本

| 字段/路径 | 相对路径基准 |
| --- | --- |
| Setup 的 Project、命令行传入的路径 | 启动 DebugTUI 时的当前目录 |
| 项目 ELF、Source root、SVD、日志目录、`source_map.to`、tools 引用 | 项目 TOML 所在目录 |
| profile 中含 `/` 或 `\` 的工具可执行路径、工具 `cwd` | 定义该字段的 TOML 所在目录；裸命令名从 PATH 查找 |
| 新式 Build / Download 命令中的脚本与参数 | Source root；为空时用项目 TOML 所在目录 |
| 工具启动参数引用的资源 | 根据实际工具的参数语义；可用 `${profile_dir}` 明确指向 profile 目录 |

可迁移工程优先使用相对路径；跨盘或工程外共享工具可保留绝对路径。`gdb.args` 是传给 GDB 的参数数组，不会把其中所有字符串自动当作路径转换。TOML 中 Windows 路径优先使用 `/`，或使用单引号字符串保留 `\`。

`version = 1/2` 用于原有配置；选择芯片的工程使用 `version = 3` 并声明 `[debug].chip` 与核心列表。只把版本号改为 3、却不选芯片会校验失败。旧 `[server]` 必须迁到 `service/target/actions`，`--device/--port` 已移除；用 `--tools-dir` 或 `[tools].root` 时，目录内必须存在 `debug-env.toml`。升级程序不会代替迁移板级工具。

## 芯片与核心选择

安装 npm 包时创建 `%LOCALAPPDATA%/debugtui/profiles/devices.toml`；直接运行 EXE/ZIP 时首次启动创建。已有文件原样保留，升级、卸载不会删除客户条目。可用 `debugtui --init-profiles` 检查/补建；`DEBUGTUI_CONFIG_DIR` 可指定配置根目录（最终路径为其下的 `profiles/devices.toml`）。不依赖 VS Code，PowerShell 中同样可用。

开发分支的 `--init-profiles` 同时创建用户 `profiles/registers/` 扩展目录。目录默认为空；M4／R52／R52+ 默认定义编译进 EXE，npm／ZIP 另提供原始 TOML 模板。把客户修改放在用户扩展目录或工程中，程序与配置根分开保存；升级、卸载、重装保留外部客户文件，包括损坏目录文件。无效同名 override 会报错，不能自动回退内置；扩展路径被普通文件占用时初始化失败并保留原文件。[目录交付及当前验证范围](docs/register-distribution.md) 不代表这些开发功能已随正式 Release 发布。

默认目录包含 tha6104 `[0]`、tha6206 `[0,1]`、tha6412 `[0,1,2,3]`、stm32f429 `[0]`。它是用户维护的能力声明，不自动探测板卡。

1. Setup 的 **Chip**（Tools / profile 后）按 Enter，选择芯片。
2. 在 **Debug cores** 中用 Space / 鼠标勾选一个或多个核心，**A** 全选、**N** 清空、**Enter / Apply** 确认；至少选择一个。单独选 `[1]` 会连接物理 core1 的端口。
3. **Ctrl+S** 保存项目选择；**Start debugging** 才关闭上一会话并按新选择连接。多核使用现有 Scope All / Core、组运行/暂停、逐核单步和断点机制；软件组控制不等于硬件同步锁步。
4. 新芯片：Chip → **N / Add chip**，填写名称、核心 ID 列表、backend，**Ctrl+S / Save** 追加到用户目录。例：`s32k144` / `0` / `generic`，再选择并应用。重复名称、无效或重复 ID 会提示错误；取消不会写入。

项目只记录当前选择；未选芯片的旧项目继续使用原单核或 `[[cores]]`，可在芯片选择器按 **L / Legacy** 返回旧模式：

```toml
version = 3
[tools]
profile = ".vscode/debug-env-chip.toml"
[debug]
chip = "tha6206"
cores = [0, 1] # 可改为 [0] 或 [1]，也可直接在 Setup 勾选
```

芯片目录的 `backend` 用于匹配工具环境，**不内置烧录算法或芯片驱动**。`debug-env.toml` 用根字段 `backend = "tha6"` 声明适用后端，并提供 `[core_targets."0"]`、`[core_targets."1"]` 等的 `endpoint` 与可选 `ready`。启动服务并使用逐核 `ready` 时，须为全部所选核心提供就绪标记。不同后端可共用一份文件，通过 `[backends.tha6.gdb]`、`[backends.stm32f4.service]` 等分组覆盖公共字段（支持 gdb/target/service/actions/session/sync/memory_access/multicore/core_targets）。没有匹配后端、缺端口或重复端口时，在启动前报错。`generic` 可复用无 backend 标记的传统工具环境，但仍需用户提供支持目标芯片的 GDB Server、探针配置与下载命令。

一核对应一个 GDB 会话，所有会话共享一项服务。端口优先取对应 `[[cores]]` 模板的显式 `endpoint`，否则取 profile 的 `core_targets`；不会按核心编号自动递增端口。只有单选物理 core0 时，才可回退到根 `[target].endpoint`。项目 `[[cores]]` 可作为按 `core.0` 等物理名称匹配的可选模板，保留每核初始化和启动顺序（当前各核共享项目 ELF）；只实例化所选核心。Watch/断点按 `core_preferences.<chip>."core.N"` 保存，切换芯片或只调 core1 不会改写其他核的偏好。未选中的共享 Reset 所属核不会被悄悄替换，相关组 Reset 被停用。

### THA6 MCAL 工程

THA MCAL 可复制发行包中的 `profiles/tha6-project.toml.example` 到工程根 `debug-chip.toml`，以及 `profiles/tha6-environment.toml.example` 到 `.vscode/debug-env-chip.toml`。这**一对文件**随选择切换 THA6104/6206/6412；无需再为 core0、core1、双核和四核复制文件。使用此命名时，以 `debugtui --project debug-chip.toml --setup` 打开。这两个模板只提供配置，要求工程已有相应 `.vscode` 工具、SVD 和构建脚本。

模板字段支持 `${chip}`、`${chip_upper}`、`${core_mask}`（所选核心位掩码）、`${available_core_mask}`（芯片全部声明核心的位掩码）、`${unselected_core_ids}`（未选核心、空格分隔）。用于工具启动参数、工程产物路径和任务/启动命令；不替换 Watch、断点表达式和源码映射。`${profile_dir}` 仍按工具环境目录展开。必须使用同芯片的 ELF/HEX/SVD；切换 Chip 不会自动 Build 或 Download。

THA 示例为 MCAL 启动屏障保留辅助核：OpenOCD examine 全部物理核，但只为勾选核心创建 GDB；包含 core0 时先连接其他所选核，再由 core0 做一次 chipreset 并恢复未选辅助核。**仅 core1 等不含 core0 的组合是附加调试，要求 core0 已完成必要初始化**。这项策略属于参考工程，可按板卡软件修改；Bao 项目不应直接照搬 MCAL 的复位/入口策略。

已有 `mcal-vsconfig` 时，可按其 `quickstart.md` 部署一整套工程环境。在可访问公司 GitLab 的网络中，执行以下命令并替换工程路径：

```powershell
git clone http://192.168.110.217/mcal/tool/mcal-vsconfig.git mcal-vsconfig
$projectPath = "D:\path\to\THA6XXX_MC_AS440"
.\mcal-vsconfig\install-debugtui.bat "$projectPath"
Set-Location -LiteralPath $projectPath
debugtui
```

部署脚本复制工具、SVD 和配置到 `.vscode`，在根目录生成 `debug.toml`，覆盖前备份到 `.debugtui-backups`；不安装编译器，也不自动编译或烧录。该入口与手动复制模板得到的 `debug-chip.toml` 名称不同，选择实际生成的 Project 即可。确认 `Tools / profile = .vscode/debug-env-chip.toml`，选择 THA6206 与 `[0]` / `[1]` / `[0,1]`，再核对 ELF 和下载产物。模板默认 GHS 构建命令；本机编译需准备相应工具链及许可证，仅调试现有产物不需要编译器。

STM32 的连接方式、下载动作和 SVD 同样由 profile/工程提供，选择 `stm32f429` 不会安装 J-Link/OpenOCD。Bao 应引用其自身的工具环境、ELF 和任务脚本：入口 `init`、启动同步变量 `_barrier` 等属于 Bao 固件，不能用 MCAL 的 `_main` 和复位流程替代。

## 常用调试操作

左侧为 Source / Asm / Files / Log，右上为 System Regs / Peripherals / Stack / Memory / Breaks，右下为 Watch / Locals；三组标签独立切换。标题显示当前版本，Console 位于底部。

- Source 内保留会话中打开的文件标签：断点/暂停、栈帧切换、Files 和 `:open PATH` 共用同一组标签。点击名称切换，点击 `×` 关闭，分别记住浏览位置；同一实际文件不会因相对/绝对路径而重复打开，同名文件用路径后缀区分。
- 标签太多时使用 `[<]` / `[>]` 翻页或在标签栏滚轮浏览；`[Files N]` / `Ctrl+O` 打开可搜索的已打开文件列表。输入文件名或路径过滤，Enter / 点击切换，Delete 关闭选中文件，Esc 退出。`Ctrl+PgUp` / `Ctrl+PgDn` 切换文件，`Ctrl+W` 关闭当前文件（Console 输入时先按 Esc）。
- 标签上的 `▶` 标记当前停止位置所在文件。浏览其他标签不改变 GDB 栈帧；关闭文件不删除断点。新一次停止会重新打开对应文件；同一停止状态的普通刷新不会重新打开手动关闭的标签。切换调试项目后清空文件标签。
- Source、Asm、Files、Log 和右侧检查面板都支持鼠标滚轮、点击滚动条轨道、拖动滑块，各标签保留浏览位置；内容不足一页时显示灰色轨道。点击源码行号切换断点。
- Log 默认跟随最新输出，向上滚动后保持浏览位置，End 恢复跟随。
- Console 固定显示 `gdb>` 输入框，点击或按 `/` 输入 GDB 命令（例如 `p/x counter`、`next`），也支持 `:watch counter` 等工作台命令。Enter 执行后继续输入，↑ / ↓ 调用历史，Esc 返回面板；输入时仍可使用调试功能键。
- 源码上方的 Run、Continue、Pause、Reset、Reconnect、Step In、Step Over、Step Out、Exit 均可点击；不可用操作显示为灰色。Reset 对应环境的 restart 动作，Run 对应 run 动作。Step In / Step Over / Step Out 分别是进入函数 / 单步越过 / 跳出函数，底层仍对应 GDB 的 `step` / `next` / `finish`。
- Exit / Ctrl+Q 按 `session.on_exit` 配置结束调试会话、释放自有调试进程并退出 TUI，运行中或未连接时也可用；仅断开连接并留在工作台可使用 `:disconnect`。
- Help 按钮 / Ctrl+P 打开帮助面板，包含 Commands 和 Shortcuts 两页，点击页签或按 Tab 切换。命令列表支持鼠标点击和方向键选择；带参数的命令会填入输入栏，补齐参数后执行。`?` / `:help` 直接查看快捷键说明。
- Asm 打开时自动请求当前 $pc 的反汇编，每次停止或切换栈帧后刷新；Memory 默认读取 $sp。加载失败会显示具体错误，:refresh 可重试。
- Pause 同时等待停止通知并核对 GDB 线程状态，处理断点与暂停同时发生的情况；确认为停止后刷新上下文，无需重连。目标确实未停下时仍会报错，日志可用 --log-dir 开启。
- 点击 Stack 中的栈帧直接切换上下文。Tab / Shift+Tab 轮换视图和键盘焦点；窄于 100 列的终端只显示当前焦点所属的一组面板。
- VS Code 集成终端可在工作区设置 `"terminal.integrated.sendKeybindingsToShell": true`，使调试按键传入 TUI；也可直接点击工具栏。

| 操作 | 按键/命令 |
|---|---|
| 选择工程、tools 和启动参数 | F2 / :setup |
| 继续；本地 READY 状态启动程序 | F5 |
| 暂停 | F6 / Ctrl+C |
| 切换源码断点 | F9 |
| 单步越过 / 进入 / 跳出 | F10 / F11 / Shift+F11 |
| 切换视图和键盘焦点 | Tab / Shift+Tab，或点击标签 |
| 命令面板 | Ctrl+P |
| 搜索源码 | Ctrl+F |
| 工作台命令 / GDB 控制台 | : / / |
| 退出 | Ctrl+Q / :quit |
| 帮助 | ? |

~~~text
:connect
:reconnect
:run
:break main
:watch counter
:data-break flag
:memory $sp 256
:disasm $pc
:files
:frame 1
:restart
:download
:disconnect
~~~

寄存器名称和编号从 GDB 动态获取，可用 `gdb.registers` 限制自动读取范围。内存默认地址为 `$sp`。run/r 默认保留 GDB 启动语义；只有显式加载环境中的 actions.run 时才执行自定义流程。下载操作执行前需要界面确认。

目标运行时默认显示上次暂停快照；只有配置了可用运行时内存通道的观察项才持续更新。FreeRTOS 固件可按普通程序调试，但没有 RTOS 任务感知视图；Source 只读。SVD 外设视图已实现，详见 [寄存器外设与内存](#寄存器外设与内存)。

## Source 选择、复制与加入 Watch

- 在代码区拖动选择文本，双击选择变量名；支持跨行选择、Shift+方向键扩选、Home / End、Ctrl+Home / End 和 Ctrl+A 全选。长行可用左右键横向移动，行号栏保持固定。
- 点击顶部 **Copy**、右键菜单的 **Copy**，或在 Source 聚焦且存在选区时按 **Ctrl+C**，复制原始代码（保留 Tab 和中文，不含行号、断点符号或右侧面板）。**Esc** 清除选区。没有 Source 选区时 Ctrl+C 仍暂停调试；**F6** 始终暂停。
- 选中变量或单行表达式后，点击 **Add to Watch**、右键选择同名操作，或按 **Ctrl+Enter**，加入**当前核心、当前栈帧**的 Watch。多核之间不会自动复制观察项；切换核心、停止位置或源码文件时清除旧选区。运行中添加的表达式在下次暂停时求值，已有 Live refresh 规则不变。表达式由 GDB 按现有 Watch 规则处理，选择文本本身不会求值。
- 行号/断点栏点击及 **F9** 仍设置当前核心的断点；代码区点击和拖动只选择文本。Source 保持只读；没有内置编辑器或“打开外部编辑器并定位当前行”的按钮。修改源码需自行在外部编辑器完成，后续步骤见 [构建与下载](#构建与下载)。

Windows 使用系统 Unicode 剪贴板，无需 VS Code、PowerShell 模块、Python 或其他常驻进程；可在 PowerShell / Windows Terminal 中独立运行。若终端截获 Ctrl+Enter 或鼠标事件，请用按钮、右键菜单或键盘选择；终端自己的 Shift+鼠标选择与 TUI 选区相互独立。源码包含 macOS `pbcopy` 和 Linux `wl-copy` / `xclip` 的剪贴板分支，但当前发行及宿主验收范围仍是 Windows x64；其他平台不据此视为已支持。

## 文件和符号搜索

- Symbols、Files 和 Watch 输入区使用可见边框及独立背景；聚焦时边框和底色高亮。常规窗口显示完整输入框，小终端自动使用单行边界；Watch 的 **+ Add** 保持独立按钮。Watch 输入框用于添加监视表达式，不是文件或符号筛选。
- **Files** 页面顶部提供 **Find** 输入框：点击或在该页面按 **Ctrl+F** / `/` 输入。匹配不区分大小写，优先包含匹配，其次按字符顺序模糊匹配；`spi` 可找到 `Spi.c`、`Spi_Irq.c`、`espi_hal.c`、`espi_std.c`，也可输入路径片段。↑/↓、PageUp/PageDown 选择，Enter 或点击结果打开到 Source；Ctrl+U 清空，Esc 结束输入并保留筛选。
- 主界面顶部提供 **Symbols** 搜索框，点击、按 **Ctrl+K** 或输入 `:symbols` 打开。检索已加载 ELF 中的函数、全局/静态变量和类型，显示类别、名称、文件和行号。Enter 或点击结果跳转到 Source 对应行，沿用 `Source root` / `[[source_map]]`，不会改变当前栈帧、PC 或断点。
- 符号查询在 READY / STOPPED 时进行，输入停顿 200 ms 后查询，一次最多 200 项/类别；达到上限时提示继续缩小查询。运行中可使用已经返回的结果；新的查询需暂停，不会自动暂停或恢复目标。F4 重试，Ctrl+U 清空，Esc 关闭；切核、重连或更换 ELF 时丢弃过期结果。
- Files 的范围是 GDB 提供的 ELF 源文件列表；Symbols 使用 [GDB/MI 符号查询](https://www.sourceware.org/gdb/current/onlinedocs/gdb.html/GDB_002fMI-Symbol-Query.html)，不依赖源码文本猜测定义。未编入 ELF 的文件、预处理宏和普通函数局部变量不属于这个全局索引；缺少调试信息、文件或行号时明确提示，不跳到猜测的位置。旧版/厂商 GDB 不支持某一类别时保留其他类别的结果并提示。

## 源码路径重映射

`Source root` 用于本地源码定位，也作为新式 Build / Download 命令的工作目录；仅设置它不会自动改写 ELF 中的服务器绝对路径。使用同一次构建的 ELF/HEX，并检出对应提交的源码，再配置映射。

1. 将 **Program / ELF** 指向服务器产物，**Source root** 指向与该次构建一致的本地源码根。
2. 将 SVD file 后面的 **Source remap** 切换为 **Yes**，自动用已配置的 GDB 离线读取 ELF 的源文件目录。为 No 时，**ELF path prefix** 灰显，键盘跳过、鼠标不可选择或编辑；已保存的前缀保留。该扫描不启动服务、不连接板卡、不执行 profile 的 GDB 参数或初始化命令；无需新增工具。
3. 在 **ELF directories → Source root** 中用上下键或鼠标选择目录，左右键切换父/子层级。选择与 Source root 对应的层级，例如 `/ci/job/firmware` → `D:/work/firmware`，其后的 `src/main.c` 保持不变。预览显示本地找到数 / 该前缀覆盖数及文件示例；存在不等于版本一致。
4. 按 **Enter / Apply** 确认，**Ctrl+S** 保存；点击 **Start debugging** 以新配置重新启动调试。后续可在 **ELF path prefix** 重新扫描选择；**R** 重扫，**Esc** 取消选择或正在进行的扫描。

```toml
[program]
elf = "ci-artifacts/firmware.elf"
source_root = "."

[source_remap]
enabled = true
from = "/ci/job/firmware"
```

选择保存在项目 TOML，目标始终跟随 `program.source_root`；相对路径仍基于 TOML 所在目录。目录匹配兼容 `/`、`\` 及混用、盘符、UNC 和空格；扫描发现的其他分隔符拼写会保存在 `aliases`，供 GDB 使用。没有调试信息或无目录记录时显示原因，可取消后修正 ELF / Tools profile 再试。部分 GDB 仍会解析旧网络路径，遇到不可达的 UNC 主机可能超时；可用已有的手工 `[[source_map]]` 配置绕过目录扫描。

关闭 **Source remap** 会保留但停用该选择和已有 `[[source_map]]` 规则；旧工程未配置开关时，原规则仍默认生效。额外手工规则先应用，Setup 所选同名前缀会覆盖原规则。此开关不管理用户在 `gdb.init` 或 Console 自行执行的 `set substitute-path` 命令；建议统一移入项目映射配置。各核共享映射。修改 ELF 不会自动改变 Download 脚本使用的 HEX，也不会同步独立的 `live_watch.elf`。

### 手工映射多个根目录

需要映射多个目录或无法完成离线扫描时，在项目 TOML 中使用 `[[source_map]]`。它同时供 GDB 的 `set substitute-path` 和 TUI 的文件定位使用；按路径前缀边界匹配，不做文件名的全局字符串替换。例如将服务器源码和公共库映射到本地两个目录：

```toml
[program]
elf = "ci-artifacts/firmware.elf"
source_root = "."

[source_remap]
enabled = true

[[source_map]]
from = "/ci/work/firmware"
to = "."

[[source_map]]
from = 'C:\build\shared'
to = "../shared"
```

`to` 的相对路径基于项目 TOML。Windows 路径建议写成 `C:/build/shared` 或 TOML 单引号原义字符串；双引号中的反斜杠需要转义。重启调试后，检查 Files / Symbols 跳转、停止位置源码和本地 `file:line` 断点是否正确。映射只能解决路径差异，不能补回缺失的调试信息、被优化掉的变量，也不能纠正源码与固件版本不一致。

## 断点管理

- **Breaks** 行首 `[x]` / `[ ]` 是启用开关，点击或选中后按 **Space / Enter** 切换；禁用保留记录和配置，**Delete** 才删除。源码用 `●` / `○` 区分启用与禁用；对禁用断点按 F9 会重新启用。
- **+ Code**（Insert / n）添加 `file:line`、函数或 `*address` 断点，可选择硬件断点和命中后删除的临时断点。
- **+ Data**（d）添加变量或指针表达式，例如 `xTickCount`、`*(uint32_t *)0x20000000`，可选择 **Write / Read / Read/write**。Write 使用 GDB 的值变化语义；读、读写需要目标支持硬件 watchpoint。数量、宽度、对齐限制由 GDB/调试服务器决定，失败时显示其错误。
- **Edit**（e / 右键）设置启用状态、条件和忽略次数；忽略 N 次表示跳过接下来的 N 次命中。行下方显示当前核 GDB 的实际命中总数与剩余忽略次数。**Enable all / Disable all** 处理当前核列表，并包含列表中多核断点关联的其他核。
- 点击源码行号栏或 **+ Code** 默认只在当前核创建。选中已有代码断点后，点击 **Cores** 或按 **c**，勾选目标核心，**Ctrl+Enter / Apply** 应用；**a** 全选，**s** 只保留当前核。多核断点显示 `[N cores]`，底部列出核心名称。启停、条件/忽略次数修改和删除会应用到该断点关联的所有核；回到单核会移除其他核上的关联副本。当前核必须保留，可先切核再更改所属范围。
- 核心选择支持持久代码/硬件断点；临时断点和数据观察点仍按核管理。操作前受影响核心都必须连接并暂停；不会为了修改断点隐式暂停目标。跨核失败会尝试回滚此前的修改，回滚失败同样明确报告。相同位置的独立断点不会自动合并。
- 断点修改立即保存至工程；禁用状态、数据类型、条件与剩余忽略次数跨重连及进程重启恢复。临时断点不持久化。恢复失败的记录标为 `[!]`，仍保留，可重试启用或明确删除。
- 编辑器支持鼠标、Tab / ↑ ↓、Ctrl+U 清空、Ctrl+Enter 应用、Esc 取消。修改断点前先暂停目标；窄终端仍可用快捷键打开编辑器。

控制台支持 `:break main`、`:data-break counter`、`:data-break read counter`、`:data-break access *(uint32_t *)0x20000000`、`:enable 2`、`:disable 2`、`:disable all`。这些操作通过标准 [GDB/MI 断点命令](https://sourceware.org/gdb/current/onlinedocs/gdb.html/GDB_002fMI-Breakpoint-Commands.html) 执行。

旧的 `breakpoints = ["main"]` 配置仍然兼容；修改后保存为详细记录，例如：

```toml
[[breakpoints]]
location = "xTickCount"
kind = "write"  # code / hardware / write / read / access
enabled = false
condition = "xTickCount > 100"
ignore_count = 0
temporary = false
```

硬件断点/观察点的额度由目标实现和调试服务器分配。多核关联断点会在每个成员核分别插入，可能分别消耗硬件槽位；DebugTUI 不提供跨核共用的额外额度，也不把所有芯片统一视为“每片”或“每核”固定数量。应以对应 CPU/SoC 手册、服务器能力和实际插入结果为准。

## Watch 与显示格式

### 添加与移除

直接在 Watch 底部输入变量名或表达式，支持自动补全，点击 **+ Add** 或按 Enter 添加。输入为空时点击 **+ Add** 会聚焦输入框；添加成功后清空已提交内容，失败时保留输入供修改重试。

点击变量行右侧的 **×** 即可移除该变量，无需先选中。也可选中变量后按 **Delete**，或点击标题右侧的 **Del Remove**。删除后自动选择相邻项，可连续删除到空列表。输入框内 Delete 不删除观察项，草稿会保留；Console 焦点也不会触发 Watch 删除。运行中同样可以增删观察表达式，新加的值在下一次暂停时读取；移除观察项不修改目标变量或硬件观察点。

### Watch 结构体、数组和指针

- 输入结构体变量（例如 `object`），点击名称前的 **▸** 或选中后按 **Enter / →** 展开，**←** 折叠；支持嵌套结构体、数组和指针成员。Enter 在普通标量行仍聚焦 Watch 输入框。
- 只在暂停时读取展开的成员。折叠、删除不发送目标读取命令；运行期间默认显示上次暂停的快照；已配置有效运行时通道的可见成员可按 [运行时刷新](#运行时刷新) 采样。大数组每次显示 32 项，通过 **Load more** 继续加载；单个表达式最多 256 个节点、8 层，循环指针不会自动无限展开。
- **× / Delete / Del Remove** 只删除顶层观察表达式；成员行不能误删相邻表达式。成员数值同样支持右键或 **f** 单独选择 2 / 8 / 10 / 16 进制。
- 表达式使用 GDB 的 C/C++ 语法，括号和 `*` 使用英文字符；类型定义从当前 ELF 的调试信息读取。多核模式下，每个核独立求值并保留自己的展开状态。

| 表达式示例 | 含义 |
| --- | --- |
| `(struct MyType *)(0x20000000)` | 将地址转为结构体指针，展开后查看成员 |
| `*(struct MyType *)(0x20000000)` | 解引用，直接观察该地址的结构体 |
| `(uint32_t *)(0x20000000)` | 观察整数指针，可展开查看所指数据 |
| `*(uint32_t *)(0x20000000)` | 读取该地址的 32 位整数 |
| `((struct MyType *)0x20000000)->member` | 只观察指定成员 |

将 `MyType` 替换为项目的实际结构体名称；`uint32` 只有在项目定义了这个 typedef 时才能使用。无效地址或缺失类型在对应 Watch 项显示错误，不影响其他观察项。

### 每个数据项独立选择进制

右键具体数值，或选中数据行后按 **f**，选择 **2 / 8 / 10 / 16** 进制，Enter 或鼠标应用。长数值可在格式弹窗预览中换行查看。

- **Watch、Locals、内存字节默认十进制**；**系统寄存器、SVD 外设寄存器和位域默认十六进制**。地址始终十六进制。
- 每个变量/表达式、寄存器、位域或内存地址独立保存；Locals 按文件和函数区分同名变量。内存右键对应字节，格式不影响相邻字节。
- 显示转换使用整数运算（最大 128 位），不写目标、不修改 GDB 的全局 radix、不发送额外 GDB 查询。负数保留符号，例如 `-1 → -0x1`；不猜测有符号数据的位宽。
- 浮点、枚举名称、字符串等非整数显示保留 GDB 原文，并在格式弹窗提示。结构体可展开后单独设置成员进制，也可直接 Watch `object.member`。
- 设置保存到工程 `debug.toml` 的 `[ui]` / `[ui.formats]`；没有保存工程的临时会话只在当前会话生效。环境 tools 配置不写入 UI 偏好。

## 寄存器外设与内存

### CPU 寄存器

**System Regs** 从 GDB 动态获取寄存器名称、编号和值，暂停后更新；寄存器是否可见取决于 GDB 和目标描述。配置 CPU 目录后提供分组、字段和按需读取；已适配的 Core writer 可经 Edit value 编辑。CP15/EL2/MPU、banked 或向量寄存器是否可读，必须由调试后端提供，不能通过导入外设 SVD 自动补齐。

可用 `gdb.registers` 指定自动读取的寄存器名称，默认空列表读取所有命名寄存器。已有 THA6206 验证中，复位初态自动读取 VFP 的 d/s 寄存器曾引发 OpenOCD `DSCR.ERR`；参考工程因此配置 `registers = ["r0", "r1", "r2", "r3", "r4", "r5", "r6", "r7", "r8", "r9", "r10", "r11", "r12", "sp", "lr", "pc", "cpsr"]`。这样保留通用寄存器和程序状态的自动刷新；浮点寄存器可在固件完成初始化后通过 GDB Console 按需读取，或调整列表。程序不会伪造被排除寄存器的值；不存在的寄存器名称会报错。

### SVD 外设寄存器

启动页的 **SVD file** 支持输入路径或 F2 选文件，也可用 `--svd FILE` 指定。配置保存为 `[program] svd = "./chip.svd"`，相对路径以工程配置目录为基准；留空不启用外设描述。

右侧标签顺序为 **System Regs → Peripherals → Stack → Memory → Breaks**。System Regs 显示 GDB 提供的 CPU/系统寄存器，Peripherals 根据 SVD 显示外设、寄存器及位域。

- 点击或 Enter 展开/收起，左右键展开/收起；支持滚轮和拖动滚动条。
- 仅在目标暂停、外设页可见时，读取已展开分组中当前可见的寄存器；默认每个停止代次读取一次，无定时轮询。隐藏、收起或目标运行时默认不发起读取，缓存值标为 `cached`；显式启用可用的 Live refresh 通道后按 [运行时刷新](#运行时刷新) 规则采样。
- 选中寄存器或其位域，点击 **Refresh selected**、按 `r` 或执行 `:peripheral-refresh`，只读取对应寄存器。读取失败显示行内错误，可手动重试。
- 跳过只写寄存器；SVD 标记的寄存器/位域读取副作用会禁止自动读取，允许显式手动刷新。未在 SVD 声明的硬件副作用无法自动判断。
- 支持 `derivedFrom`、数组、cluster、属性继承和位域；按文件声明的大小端解码对齐的 8/16/32/64 位寄存器。未知字节序和不支持的宽度只展示定义，不猜测值。

SVD 解析器静态编译进 EXE，不增加运行时环境。SVD 文件按工程配置加载，不打入 EXE/npm 包；工程移交时请同时提供所引用的 SVD。



### Memory 与 Asm

Memory 默认读取 `$sp`；可在 Console 输入 `:memory 0x20000000 256` 查看指定地址的 256 字节。Asm 按当前 `$pc` 或 `:disasm EXPR` 请求反汇编，停止/切换栈帧后按需更新。读取错误会显示原因，`:refresh` 可重试。

Memory 可选择地址范围、字节数及实际通道；运行中默认保留暂停快照。Memory / Peripherals 的 Edit value 使用共同预览和写入流程，写权限需独立配置，详见下方写入说明。

### 开发分支的访问入口（尚未发布）

以下说明适用于 `codex/register-debugging` 开发分支，已安装的 v0.9.3 仍遵循上面的操作说明。

- Setup 的 **Memory channels** 可以编辑通道 ID、名称、TCL endpoint、target、运行时访问声明和核心限制。保存写入当前项目覆盖，取消丢弃草稿；继承的工具 profile 不被改写。
- Watch、Peripherals 的 **Memory access** 按钮打开逐项通道与刷新设置，展示实际 target、endpoint 和配置来源。Watch 在暂停时解析可取地址成员；重连、换停止点或栈帧后重新解析。
- Memory 的 **Memory access** 设置地址、字节数及通道，**Apply and read** 保存并读取，**Cancel** 放弃草稿。键盘用 Tab／上下键选字段，左右键切换通道，Ctrl+U 清空输入；鼠标可选字段和按钮。
- Memory 每次读取 1–4096 字节。GDB 通道在暂停时接受 `$sp` 等地址表达式；总线通道只接受十六进制或十进制数值地址。`:memory ADDRESS [COUNT]` 使用该面板已经选择的通道。
- 暂停后，打开 Memory 面板会按当前停止点读取一次；**Read** 可以再次采样。运行时仅允许手动使用明确声明 `while_running = true` 的通道，不对任意地址范围自动轮询。范围读取不保证原子性。
- 地址范围和通道按芯片与核心保存。读取途中换核、重连、切栈帧或修改范围，迟到响应被丢弃。失败显示原因，保留旧样本时标为 stale；失败不会隐式暂停或切换通道。

Headless 的 `memory_channels` 返回通道、核心适用性与配置来源；`memory_dump` 接受 `address`（字符串）、`count`、`channel` 和可选 `context`。context 与 `registers_list` 返回的当前上下文一致。结果返回地址字符串、按地址排列的字节数组、实际 target／endpoint／source、上下文和 `atomic = false`。未知通道、超出范围、地址溢出和不完整响应均报错。旧 `memory` 接口继续保留。

当前分支的能力与未完成项见 [开发进度](docs/registers-development-status.md)。这些说明及软件夹具验证不代表新接口已通过上板验收。

## 运行时刷新

1. 在 **Watch / Peripherals** 的数值上右键（或选中后按 `f`），点击 **Memory access / Live refresh**（快捷键 `r`）。位域使用所属寄存器的读取策略，显示进制仍独立。
2. **Access** 选择通道；默认 **GDB · selected core · stopped only**。环境声明的 OpenOCD 通道显示是否允许运行时读取。
3. **Live refresh** 选择 On，**Interval** 输入毫秒数（50–60000；100 ms = 10 Hz），点击 **Apply & save**。**Read once & save** 可单次读取，不要求开启循环。
4. 成功采样标记 **LIVE**；错误直接显示在该项，失败后至少间隔 1 秒再试。设置保存在工程 `[ui.refresh]`，按核心和观察项分别记忆。

只轮询当前可见的 Watch 标量/指针、展开树中的可见成员及外设寄存器；隐藏窗口、折叠分支不继续后台读取。结构体根节点的策略可由成员继承，成员也可单独覆盖。读取串行执行，频率为尽力而为：多个值、探针延迟和调试命令会降低实际刷新率。不会隐式暂停、恢复、复位核心，也不自动写 AP/CTI 配置；副作用/只写寄存器不能自动轮询。

Watch 的地址和类型由 GDB 在暂停时解析，支持可取地址的 8/16/32/64 位标量、成员与强制类型转换，正确解释整数符号和 float/double。运行前至少暂停解析一次；运行中按已解析地址采样，下次暂停重新解析。指针在运行中改变时不会自动追踪新的地址，临时值、位域、CPU 寄存器及无地址表达式不能用总线直接轮询。64 位读取是两次 32 位访问，不保证原子性。目标物理地址映射、权限、缓存一致性必须由板级配置保证；AHB 数值不保证等于有缓存/MMU 的核内视图。

`tools` 环境提供通道，例如：

```toml
[[memory_access]]
id = "bus"
label = "AHB memory"
tcl_endpoint = "127.0.0.1:6666"
target = "soc.ahb"
while_running = true
# cores = ["core0", "core1"]  # 可选：仅对这些核心开放
```

TUI 只发送指定 target 的 `read_memory`，不执行全局 `targets` 切换。`soc.ahb` 对应哪个 DAP/AP、借哪个核、如何创建多个 CTI，均放在板级 OpenOCD 配置；AP1/AP3 不具备通用固定含义。多核示例见 [多核内存通道示例](https://github.com/bei-li16/DebugTUI/blob/main/tools/examples/multicore-access.md)。已有 `[live_watch]` 配置会直接更新 Watch；显式设置的逐项 Memory access / refresh 优先，包括手动或关闭设置。

**STM32F429 参考环境**：[OpenOCD profile](https://github.com/bei-li16/DebugTUI/blob/main/tools/debug-env-openocd.toml) 使用 J-Link 探针 + OpenOCD，M4 与独立 `mem_ap` target 均使用该芯片的 AP0。选择该 profile 可获得 AHB 和 stopped-only Core 通道；J-Link GDB Server 的普通 profile 不提供 OpenOCD TCL 运行时通道。THA6206 的通道应以其板级 OpenOCD 配置为准，不要照搬 STM32 的 AP 编号。已有 STM32、THA6206 等验证记录及限制见 [测试记录](TESTING.md)，参考环境通过不代表所有通道和芯片组合都已验收。

### 兼容的全局 Live Watch 配置

可选 `[live_watch]` 配置 `tcl_endpoint`、`bus_target`、`elf`（相对工程路径）、`interval_ms`。解析 ELF32/64 小端文件中的唯一全局 8/16/32 位对象，在运行时直接更新 Watch 值及 LIVE 标记；不再把每次数值变化刷到 Console。数值标为 `<raw N-bit>`，支持显示进制切换，不推断浮点/有符号类型。结构体、数组大对象、局部表达式和类型解释仍由停止后的 GDB Watch 或逐项 Memory access 负责。随切核、停止代次及 Watch 列表变化更新，暂停时恢复 GDB 的值，断线显示错误并重试，断开时取消。读取使用 OpenOCD 的 [target-specific read_memory](https://openocd.org/doc-release/html/CPU-Configuration.html)，不改变全局选中 target；运行时能否读到一致数据取决于芯片、AP 和缓存配置。Headless 输出结构化 `live_watch` 事件，包含表达式、核、generation、地址、位宽、原始值/错误与时间；GDB 停止快照不被实时值改写。

## 多核工作区

优先通过 Setup 的 Chip / Debug cores 选择核心；该模式按 `[debug].cores` 生成会话。Legacy 模式使用显式 `[[cores]]`；未选芯片且未配置 `[[cores]]` 才沿用普通单核会话及 JSON 快照格式。以下是 Legacy 双核配置片段，需配合有效的 GDB、ELF 和工具环境：

```toml
[[cores]]
name = "cpu0"
endpoint = "localhost:3333"
startup_order = 1

[[cores]]
name = "cpu1"
endpoint = "localhost:3334"
startup_order = 0
```

- 每核独立 GDB/MI 会话；序号固定按配置顺序。点击 **Cores** 栏按钮、`Ctrl+T`、`:core 0`、`:core cpu1` 切核，`:cores` 查询所有核的状态。多核按钮带运行状态，左右箭头可浏览更多核心。切换后 Source、Asm、System Regs、Stack、Memory、Watch 和外设缓存归属当前核，Asm 等暂停后按需刷新；源码及检查窗口的底色、边线随核心颜色变化，保留 PC/错误本身的语义色。单核工程不显示多余的 Cores 栏。
- Connect、Reconnect、Run、Disconnect、Exit 作用于整个工作区；Connect/Run 按 `startup_order` 逐核等待命令结果。它保证调试命令执行顺序，不保证前一个核的固件已完成启动。
- Continue/F5、Pause/F6 可选择全部核或当前核。工具栏 **Scope: All / Core** 或 `:scope all` / `:scope core` 切换当前会话范围；旧配置默认 Core。Run All 始终按启动顺序控制全部核：每次连接/复位后首次执行各核 `run`，之后只继续，已运行核跳过。
- All 模式下，断点/观察点/异常停止会暂停正在运行的其他核，并切换到触发停止的核。Step/Next/Finish 先暂停其他核，再只执行选中核；跨核锁或同步等待代码可能需要继续全部核才能推进。Core 模式保持独立调试。Watch、新建断点和任意原始 GDB 命令仍属于选中核；显式关联的多核断点编辑按其成员范围分发；Console 中的 `continue`/`c` 等执行别名遵循上述范围。
- 组模式不会重复执行每核的复位脚本。整片 Reset 需要下述 `multicore.restart`；它先暂停全组，只经指定核执行一次，再清除每个 GDB 的寄存器缓存并刷新状态。即使切到 Core 模式，**Reset Chip** 也作用于整片。没有配置共享复位时，All 模式拒绝 Reset。部分继续/复位失败会报告逐核结果，并尝试停住已运行的核。
- 任一核连接失败会断开所有核并清理本工作区启动的服务。仅一个 `[[cores]]` 时也会正确管理服务。外部启动的服务不归 TUI 所有。
- 已有会话时重复 Connect 直接报错并保留连接；需要重建时使用 Reconnect。多核更换共享 ELF 请在 F2 Setup 修改 Program / ELF 后整体启动；`:elf` 保留用于单核。
- Legacy 模式的 Watch、断点保存到各自 `[[cores]]`；Chip 模式保存到 `core_preferences.<chip>."core.N"`；多核断点记录通过可选 `group` 标识关联；未配置时继承顶层列表，`watch = []` / `breakpoints = []` 表示显式空列表。
- 外部 Build / Download 命令执行前释放所有会话及自有服务，成功后恢复先前已连接的工作区；GDB download action 仍只对当前核执行。
- 当前各核共用 GDB 程序、ELF、Source root 和 SVD；异构核使用不同 ELF/SVD 尚不支持。

例如 THA6206 的组控制（`chipreset` 必须由板级 OpenOCD 配置提供）：

```toml
[multicore]
scope = "all"
halt_peers = true
restart_core = "core.0"
restart = ["monitor chipreset"]
```

组控制是软件顺序协调，不承诺同时启动、同时暂停或周期级锁步。需要精确同步时，仍须板级环境配置并验证 CTI 等硬件链路。`halt_peers = false` 可关闭断点联停；运行时范围切换不写回配置，下次启动采用 TOML 的 `scope`。

可选的 `[sync]` 使用 `tcl_endpoint = "localhost:6666"` 与 `open = ["..."]`，在共享服务就绪后、GDB 连接前执行。`method = "tcl"` 或兼容旧配置的 `"cti"` 都执行显式配置的 TCL 命令，不自动识别/配置 CTI。任何命令失败都会中止连接。

## 构建与下载

启动页在 Source root 后提供 **Build command** 和 **Download command**。填入完整命令行，例如 `build.bat`、`cmake --build Debug`、`"tools/flash app.bat" "Debug/app.elf"`。命令保留原始引号；Windows 使用 `cmd.exe /D /V:OFF /S /C`，可调用 BAT / CMD / EXE，PowerShell 脚本需显式写 `powershell -File ...`。

```toml
[program]
elf = "Debug/app.elf"
source_root = "."

[tasks]
build = 'build.bat'
download = 'flash.bat'
timeout_ms = 300000
```

- 两个脚本都以 **Source root** 为工作目录，相对脚本路径和参数也相对于此目录；Source root 留空时使用项目配置文件所在目录。命令保存在项目 `debug.toml`，不会改写 tools 配置。
- 主界面顶部 **Project** 操作区独立显示 **Build / Download**，与下方 Run / Continue / 单步控制分开；窄窗口同样保留这两个入口。输出和失败信息显示在 Console。
- 外部命令执行前释放已连接的 GDB / 自有服务，避免 ELF 或探针占用；命令成功后恢复原有连接并重新加载符号。失败后保持断开，方便修改或重试。运行中先 Pause，再点击按钮；执行中禁用重复任务，Ctrl+Q 可取消并清理本程序启动的子进程。
- 尚未生成 ELF、但已配置 Build 时，可以进入工作台先构建，然后点击 Reconnect 开始调试。
- Download 执行前显示命令与工作目录供确认。Download command 留空时继续使用 tools 的 `[actions].download` GDB 动作（需要目标暂停）；显式配置的脚本优先，二者不会同时执行。
- 兼容旧 `[build] command / args / cwd` 配置；Build command 留空时使用旧配置，否则使用 Source root 下的新脚本。外部命令默认限时 300 秒，可用 `tasks.timeout_ms` 调整。



### 修改源码后的工作流

1. 在 VS Code 或其他外部编辑器打开对应源码，修改并保存。PowerShell 独立启动 DebugTUI 同样可用，无需编辑器插件；TUI 不负责打开/定位外部编辑器。
2. 在 DebugTUI 暂停目标，点击 **Build**，确认命令成功退出，并确认输出的 ELF/HEX 路径。
3. 点击 **Download**，核对命令与工作目录后下载这次构建的产物。Build 失败时先处理编译错误，不继续下载旧产物。
4. 外部任务成功会恢复之前已连接的会话，并重新加载项目配置的 ELF；如果原本未连接，使用 **Reconnect / Start debugging**。验证源码、符号和实际固件一致后继续调试。

Build 和 Download 是分别触发的任务，不会自动串成流水线，也不会自动核验下载脚本的 HEX 与 Program / ELF 是同一次构建。改变 ELF 路径或使用流水线产物时，还要同步修改下载脚本/参数；代码目录不同则增加 [源码路径重映射](#源码路径重映射)。

## Console 与会话日志

- Console 右侧支持滚轮、点击轨道和拖动滑块。点击输出区后可用 ↑↓、PgUp/PgDn、Home/End 浏览；Shift+PgUp 可直接从输入框进入历史，保留未发送的草稿。
- 默认跟随最新输出，向上翻阅后保持当前位置；新消息显示在 **Latest (+N)** 计数中。点击 **Latest** 或在输出区按 **End** 恢复跟随。Enter 返回输入框，`/` 开始输入命令。
- Console 保留最近 2,000 行；Log 页保留最近 1,000 行，均为有界内存缓存。更早的记录从磁盘日志查看。
- 在启动页 **Log directory**、项目 `[session] log_dir` 或 `--log-dir` 设置日志目录后，每次连接（包括重连）新建 `session-YYYYMMDD-HHMMSS-mmm.log`，如 `session-20260917-171530-042.log`。同毫秒发生重名时追加序号，使用排他创建，旧文件不会覆盖；目录配置不变。默认不落盘，每次连接最多 8 MiB，达到上限时记录提示。
- 文件中每个物理行包含记录时间和会话已运行时长，例如 `[2026-09-17 17:15:31.276] [+1.234s] [gdb] Breakpoint 1, main ...`。Windows 使用本机时间（毫秒），其他平台标记 UTC `Z`；运行时长使用单调时钟，不受系统校时影响。Console / Log 页显示简短时间 `HH:mm:ss.mmm`。
- headless `log` 事件包含 `timestamp`、`elapsed_ms`、`channel` 和 `text` 字段，结构化进度数据可直接解析。

### 输入与补全

- 点击底部 `gdb>` 输入 GDB 命令，或输入 `:` 使用 DebugTUI 命令。输入时自动显示候选，支持 GDB 子命令及表达式参数。
- Watch 面板底部输入框直接接收全局变量名或表达式，点击 **+ Add** 或回车添加监视；也可以选中标量行后按 Enter 聚焦输入框。结构体行 Enter 展开/折叠，每个顶层表达式右侧的 **×** 直接移除该项。
- `↑` / `↓` 选择候选，`Tab` 或鼠标点击填入，`Enter` 提交。没有候选时，Console 的 `↑` / `↓` 浏览历史命令；`Esc` 退出输入。
- 变量名来自当前 ELF 的全局/静态符号；结构体成员（如 `object.field` / `pointer->field`）由 GDB 补全。符号补全在连接且未运行时可用，运行中不查询符号。
- 补全查询异步执行，输入停顿 150 ms 后查询；最多显示 64 个候选，可继续输入缩小范围。补全不执行命令，旧输入的延迟回复不会覆盖新输入。

## 自动化接口

人工界面和 AI/脚本共享同一个调试核心：

~~~powershell
debugtui --project ./debug.toml --headless --stdio
debugtui --project ./debug.toml --script ./commands.jsonl
~~~

~~~jsonl
{"id":1,"method":"connect"}
{"id":2,"method":"break","params":{"location":"main"}}
{"id":3,"method":"run"}
{"id":4,"method":"wait_stopped","params":{"timeout_ms":10000}}
{"id":5,"method":"quit"}
~~~

continue/run/step 返回表示请求已提交；wait_stopped 等待暂停或程序退出。程序未运行/已退出时状态为 READY；断点停下时为 STOPPED。脚本命令失败后停止并清理会话。当前不支持人工与 AI 同时加入同一运行会话。

Headless 不弹出 Setup，也不会因为指定了项目自动连接；先发 `connect`。一行一个 JSON 请求，标准输出包含响应及异步事件，调用方需按 `id` 关联响应。`--script` 隐含 headless，按顺序等待请求完成，失败后停止并清理会话。

Headless 支持 `{"method":"control_scope","params":{"scope":"all"}}`；单次 `continue`/`pause` 可带 `{"scope":"core"}` 或 `{"scope":"all"}`，不改变会话范围。快照包含 `control_scope`，组请求返回每核 `results` 和 `cores`；异步断点联停不伪造用户请求响应。`run` 带 `scope:"core"` 可显式只启动当前核。

## 外观与终端

界面采用深蓝灰面板与蓝色焦点，绿色用于运行/成功，红色用于断点/错误。按钮仅在圆角边框内填色；各核使用独立身份色，在工作区施加轻微色调。宽屏横向排列工程操作、核心选择和 Symbols 搜索；窄屏保留换行和紧凑布局。

### 动画与反馈

在 Help 中选 **appearance**，或输入 `:appearance` 打开设置：

- **Off**：静态反馈；**Subtle**：默认，短暂高亮；**Full**：加入断点扫描、真实单步落点残影和少量粒子。
- 也可输入 `:animations off` / `:animations subtle` / `:animations full`；外观面板的 `u` 切换 Unicode / ASCII 动效字符。
- 连接阶段来自真实环境、GDB、目标连接事件；成功后短暂点亮标题。RUNNING 有低频状态提示，运行期间旧 PC 和数值缓存变暗。
- 真正停止后才强调 PC；断点短扫描、文件标签提示，单步最多保留 3 个实际执行落点的淡影。数值变化立即显示最终值，变化高亮淡出，变化圆点保持到下次刷新；可对齐的十六进制值强调变化数字，外设位域独立判断变化。
- 输入聚焦、自动补全、提交、Watch 新增、错误、鼠标悬停/点击和滚动都有短反馈。粒子只落在空白单元格，避开输入文字。
- Build / Download 在独立 Project 栏显示实际阶段和耗时，完成/失败状态保留；只有 GDB 报告真实下载计数时才显示百分比。
- 短动画上限 25 FPS，忙碌/运行提示约 2.5 Hz；静止且无过渡时不产生装饰重绘。终端报告失焦时暂停装饰动画，任务耗时仅在聚焦时更新。动效不增加 GDB 读取，不延迟输入或调试事件；无新增运行时依赖、字体、图片或常驻服务。



使用 `debugtui --demo` 预览布局，不连接调试器。实际连接、断点、任务反馈仍由真实事件触发。

## 常见问题

| 现象 | 检查方法 |
| --- | --- |
| 能连板但没有 C 源码，或打开服务器旧路径 | 确认 ELF 有调试信息；Source root 对应本地源码；按 ELF 目录层级设置 Source remap，并重新启动会话。 |
| 行号不对、变量被优化掉或断点位置跳动 | 使用同一次构建的固件/ELF 与对应源码提交；检查优化选项。路径映射不能消除编译优化或版本差异。 |
| Watch 运行中不刷新 | 默认 GDB 只在暂停时读取；先暂停解析地址，再配置支持 `while_running` 的通道并打开 Live refresh，保持该项可见。 |
| SVD 有定义但值为空/报错 | 确认芯片/SVD 一致、目标暂停、外设时钟/访问权限及寄存器副作用；手动刷新查看错误。 |
| 多核只控制当前核，或某核停住后其他核仍运行 | 核对当前 Scope、`halt_peers` 和实际连接核心；Core 模式允许各核独立运行。软件联停并非硬件同步。 |
| core1 单核附加后卡在初始化/屏障 | 确认 core0 已执行必要初始化、辅助核按固件启动策略运行；检查项目 after_connect/run，勿套用不匹配的 MCAL/Bao 流程。 |
| cannot read IDR / AP 初始化失败、端口连接失败 | 检查板卡供电、探针、复位状态、驱动、OpenOCD 的 DAP/AP/target 配置及已有进程占用；不要仅凭此错误判定是 TUI 问题。 |
| GDB 读取某类寄存器报错 | 核对芯片初始化和服务器支持；用 `gdb.registers` 限制自动读取范围，不会由 SVD 自动补齐 CPU 寄存器。 |
| Build / Download 失败后未连接 | 查看 Console 和日志中的退出码、工作目录、工具链/许可证及产物；修正后重试，或手动重连。失败任务不会伪装成已恢复调试。 |
| 终端截获快捷键/无法拖选 | 先试可见按钮、右键菜单或键盘选择；VS Code 检查终端按键转发，PowerShell 可在 Windows Terminal 中运行。 |

需要定位连接问题时，在 Setup 设置 Log directory，保留当次会话日志及工程/profile 配置、程序版本和产物信息。只连接外部服务时 `--connect` 不会停止已有服务器，也不会去掉该 profile 的连接初始化动作。

## 开发与验证

### 寄存器状态与读取取消（开发分支）

寄存器配置先继承 Tools/profile 根设置及选定 backend，再由项目同名字段覆盖；芯片 CPU 关联只在没有显式 CPU/目录选择时填默认值。非空 catalogue 文件优先于 CPU preset，CPU preset 按用户同名文件、内置目录顺序选择。相对目录路径以声明它的项目/profile 为准；已有用户 override 损坏、不可读或为目录时报错，不回退。要改用 CPU preset，清除继承的 `catalogue`；要回到 GDB 列表，同时清空 `cpu` 与 `catalogue`。完整优先级与例证见 [配置自检](docs/register-configuration.md)。

Setup 的 CPU registers 可选 M4、R52、R52+、用户 preset、Automatic 或 GDB；Register catalogue 可输入或 F2 浏览文件。CPU picker 内的 F2 同样浏览目录。F1 查看候选目录的名称、架构、来源、说明、支持条件及各核身份，方向键／翻页／Home／End／滚轮滚动，Esc／Close 返回草稿。选择与预览不启动读取，Ctrl+S 保存项目引用，不改写客户 profile 或目录。使用文件且未选 CPU preset 时显示 Catalogue file，不会误显示成原 GDB 列表。

配置 CPU、目录 CPU、Chip 关联和 Observed CPU 分别显示；差异提示不自动改配置。芯片关联只是配置；当前停止核心没有有效 Probe 时显示 Unknown，其他核及旧 session／stop／frame 的身份不复用。Setup 改目标、工具或 TCL endpoint／target 等访问路由后，旧身份也不用于新草稿。选择 R52+ 目录不证明实际 R52+ 身份、可选扩展或 reader/writer 支持；完整操作及自检见 [Setup 目录选择](docs/register-setup-catalogues.md)。

System Regs 的 Total／Shown／Valid 分别计目录定义、展开筛选后的寄存器行及当前暂停上下文的有效值。Status 按钮、`t` 或 `:register-status` 打开分类计数和目录来源；窄窗口可用方向键／滚轮，Esc 或 Close 关闭。失败可保留旧值，但显示当前原因，不计成功。

Status 的条件详情分别保存当前判定、最新尝试和保留原值当时的依据，显示 min/max、配置声明与当前观察、目录来源、物理核和停止代次。成功读取不会丢失条件；失败后不会把新依据当成旧值依据，旧生产者没有记录时显示 Unknown。别名继承父链的条件及 WO；Unknown 可选项不自动读取，WO 即使手动也不发送读取。PMCR.N 在 Guest/SVC 下可能受 HPMN 限制，不能据此认定物理计数器不存在。当前 R52 物理数量只接受 Hyp N=4，物理 ICC 优先级只接受五位，其他原始值保留但适配容量未知。查看详情不触发读取；规则及限制见 [条件依据自检](docs/register-eligibility.md)。

R52/R52+ Timer 目录含十五项寄存器及字段；`timer.present` 未确定时不自动读取。TVAL 是三十二位有符号差值，可在格式菜单选有符号十进制；Timer 关闭时 TVAL 与 CTL.ISTATUS 的原始位没有有效计时语义。计数器、CVAL 与 CNTVOFF 保留完整六十四位，但分别采样不保证同时性。访问说明区分 Hyp、EL1/Guest 和 EL0 门控，说明本身不证明当前权限已核验；完整范围及未完成项见 [Timer 自检](docs/register-timer.md)。

使用专用后端时，在 `[registers]` 中显式设置 `timer_command="aarch64 timer"`；实际 OpenOCD 必须符合 [源码锁](tools/openocd-adapter/source.lock.json) 的独立 Timer 协议，版本号本身不能证明支持。当前适配实际 R52：Debug EL2 允许十五项，EL1 允许频率、CNTKCTL 和虚拟 Timer；EL1 物理 Timer 及 EL0 中未证明的上层使能保持权限未知。详情与 headless JSON 保存实际 MIDR、当前 Debug EDSCR 和停止 DSPSR/DLR；停止前 CPSR 不代替当前 EL。选用后端后，协议不符、访问拒绝或截断不会回退其他命令。未设置该字段的工程保留原读取路径。

Timer 的详情和 headless 来源会记录单项 `mrc32`/`mrrc64` 及主机请求起止时间。一次 MRRC 保留同一项的完整高低位；不同项分别采样。请求区间属于当前核的主机会话时钟，包含传输和检查耗时，不是硬件时间戳，也不能用来证明跨核同步或由先后两个计数值推导精确 CNTVOFF。旧值保留原来的时间来源；缺少旧证据时保持未知。

目录的 EL 条件描述正常执行权限；Debug state 另受 Hyp debug 授权、EDSCR.HDD 及后端执行规则影响，停止前 CPSR 不能独自证明允许或拒绝。延后 Timer 驱动使用专用 Hyp 固件基线，先检查模式及 ready，再核对四个稳定六十四位参考符号；这种验证流程的模式要求不等于应用已有通用 Debug state 权限适配。

Status 的 Sampling view 区分选中栈帧与物理核心状态。GDB 值及其别名随栈帧失效；返回原帧后也需要重新读取，同一停止代次的直接后端值可保留。Reset、Reconnect、更换 ELF 和 Console 会使相关旧值失效；共享 Reset 与原始 Console 在命令发送前使全部核心缓存失效，命令报错也不会恢复旧值为有效。详细规则与验证见 [缓存生命周期](docs/register-cache-lifecycle.md)。

多核项目可用 `[registers.topology]` 的 `chip` 及 `[registers.topology.clusters]` 的核心名称到 cluster 名称映射声明共享归属。目录的 `scope` 使用 `core`、`cluster` 或 `chip`；缺少 cluster 映射时不会按核心编号猜测，条目显示未知归属且不发送读取。chip 优先使用 topology.chip，否则沿用 debug.chip；两者都未声明时保留未知。

Status 显示 Scope／Owner、实际 **Sample core**，共享条目还显示 **Owner lifetime** 的采样及当前代次。各采样核心的缓存独立保留；同 cluster 成员的运行、新停止点、会话变化或退出会使该 cluster 与 chip 的共享值过期，其他 cluster 及其他核私有值保留。未知 cluster 核心活动时，全部已声明共享域保守过期。读取期间归属代次改变会丢弃对应共享新值，保留此前原值及时间；详细规则见 [多核归属自检](docs/register-shared-owners.md)。

Headless 的 `registers_list` 返回配置 `topology` 及 `topology_source = "configuration"`；多核 `registers_list`／`registers_read` 还返回 `owner_generations`，Snapshot 返回 `register_owner_generations`。缓存共享值时须同时核对样本的 worker context、owner 和 `owner_generation`；配置拓扑不证明实际硬件身份。

Status 还分别显示 Catalogue CPU／架构、Configured CPU choice 和有当前停止上下文证据的 Observed CPU。Latest read attempt 显示实际 GDB 名称／索引或 TCL endpoint／请求 target；MMIO 同时显示地址、通道来源、字节序与 bus width/count。64 位 AP 读由两个 32 位读组成，不保证原子性。请求 target 名称不能证明物理 CPU 身份。

路由状态区分未开始发送、发送后结果未确认、收到完整响应；收到错误响应仍不计 Valid。失败刷新保留旧值时，Retained raw value origin 独立显示旧值来源。别名和 VFP 重叠视图保留父值的实际请求与时间；任意 Console 后的新 GDB 请求不猜测已连接 endpoint，配置地址另列。Headless 在 Sample 的 `provenance`／`last_value_provenance` 中提供对应元数据及原始命令，旧 JSON 兼容且来源缺失时保持 Unknown。打开来源详情不会访问目标。完整规则见 [读取来源自检](docs/register-read-provenance.md)。

宽窗口固定显示 Name、Value、Size、Access 四列；长名称或过长值以省略号提示。窄窗口保留名称和值，选中说明显示位宽及访问属性。选中字段时使用字段自身的位宽与访问覆盖；按 `t` 可滚动查看父描述、字段说明、全部枚举、bit segments、访问条件、读取原因和完整原始值，包括被主行省略的 128 位高低位。

Cancel read、`:register-cancel` 或非搜索模式的 Esc 取消当前寄存器读取／Probe。MPU 总览关闭或切 bank 也取消未完成读取。当前事务完整恢复后停止后续项、丢弃新结果；会话和其他核心继续可用，需手工重读。恢复结果未知仍进入 FAULT。详见 [计数、取消及限制](docs/register-read-status-and-cancel.md)。尚未进入 v0.9.3 release。

### 寄存器显示格式与视图偏好（开发分支）

在 System Regs 选择已经读取的寄存器或字段，按 `f` 或右键打开格式菜单。整数支持十六进制、无符号十进制、二进制、八进制和有符号十进制，精确保留最多 128 位。32／64 位整寄存器还可按 IEEE 浮点显示；负零、无穷和带原始位载荷的 NaN 单独显示，极小或极大的有限值使用科学计数法。宽寄存器可选择 8／16／32／64 位向量分量，浮点分量仅有 32／64 位；lane 0 始终对应最低有效位。字段只提供整数格式，枚举同时显示数值与名称。

方向键或 Tab／Shift+Tab 选择格式，Enter 应用，Esc 取消；菜单支持鼠标及窄窗口滚动。格式只解释现有原始位，不读取目标、不使能 FPU，也不修改样本。可选格式不表示 GDB／OpenOCD 已支持读取相应浮点或向量寄存器。

展开的组和字段、筛选、目标／全部定义选择、已提交的搜索和格式保存到项目 `ui.register_views`。偏好按芯片、实际核心、CPU、架构和目录来源／版本隔离；未知芯片另绑定连接地址。搜索输入按 Enter 才保存，Esc 取消；旧进制偏好仅在首次建立视图时迁移。没有项目路径时只保留本次会话，保存失败显示错误。

同一 owner、会话、核心及栈帧内，当前有效值与前次有效样本不同的行显示琥珀色；字段只比较自身位值。首次读取、未变化、切核／重连／换帧后的首次比较和过期值不作为当前变化高亮。格式切换只改变显示，不改变原始位或变化判断。

Headless 使用 `register_preferences`，提交 `scope` 和单个 `preferences`：`open`／`fields` 为 ID 列表，`filter` 为 0–3，另有 `all_definitions`、`query` 和 `formats`。格式对象使用 `kind`：`unsigned` 加 `radix`，`signed`，`float` 加 `bits`，或 `vector` 加 `lane_bits`／`interpretation`；格式键为 JSON 编码的 `[register_id, field_name_or_null]`。`scope` 为 JSON 编码的 `[chip, core, cpu, architecture, catalogue_source, catalogue_version]`。普通 `ui_preferences` 保留已保存的寄存器视图并拒绝整张非空 `register_views` 替换；Windows 客户端通过独占文件句柄协调保存，重新合并最新配置后原子替换文件。

### 读取当前核心能力（开发分支）

配置 R52 寄存器目录后，在暂停核心的物理 frame 0 点击 System Regs 的 **Probe caps**，或执行 `:register-probe`。这会显式采样 CPSR、MIDR、ID_PFR1、ID_DFR0、MPUIR、HMPUIR、CPACR、PMCR、ICC_CTLR 、ICH_VTR，以及 FPSID、MVFR0/1/2、FPEXC 共十五项中当前身份及权限允许的项目。连接、切核和下次暂停不会自动执行这项探测。实际 MIDR 目前只识别 Arm Cortex-R52 的 D13 编码；其他型号、尚未适配的 R52+ 身份或无法读取的身份保留 Unknown，并停止扩展探测。

Log 保留逐项原始值、读取来源、错误和能力解码依据。EL1 MPU 数量来自 MPUIR[15:8]，EL2 来自 HMPUIR[7:0]；目录补齐 EL1 的 16–23 号区域，是否显示依实际实现数量判断。只有已确认的 Hyp 模式与 GIC 系统接口才探测物理 ICC 和 ICH；虚拟 ICH_VTR 的优先级信息不会用于物理 ICC AP 寄存器过滤。CPACR 权限值不能证明 FPU 已实现或 FPEXC.EN 已打开。

观测结果只覆盖当前会话／核心／停止代次／帧的运行时实现条件，不改写工程中的 `registers.facts`。运行、换帧、重连、写入或停止代次改变后失效；失败保留 Unknown 与原因。Scope All 仍只探测当前物理核心。当前采样没有验收完整可选寄存器、目标描述位宽或物理后端的执行行为；MVFR0/1 证实 D16/D32 与 NEON，FPEXC 独立记录启用状态。

Headless 先调用 `registers_list` 获取 `context`，再用 `registers_probe` 传入相同的 `context`。响应含 `probe.samples`、带寄存器及通道来源的 `probe.facts`、实际 MIDR 解码、GDB 可见名称、说明及有效 worker 的工具路径声明。路径声明不等于真实服务器版本／哈希证明。多核 Snapshot 的 `register_generation` 用于寄存器上下文，`generation` 仍表示协调器刷新版本。

### 读取 R52 模式银行（开发分支）

内置 R52／R52+ 目录的 23 个模式银行条目使用独立的 `banked` reader。工程需显式配置 `registers.banked_command = "aarch64 banked"`、TCL endpoint 和各核实际 target，并选用 [固定源码适配后端](tools/openocd-adapter/README.md)。默认空配置或协议不符返回 Reader unsupported。普通 **Read**／`registers_read` 即可读取；**Read bank** 是 MPU／PMU 选择器动作。

读取只接受当前暂停核心的物理 frame 0。后端从调试态 DSPSR 读取完整停止 CPSR，以实际 MIDR 确认身份；普通 MRS CPSR 屏蔽执行状态位，不能用于这个检查。当前银行使用普通 MOV／MRS，其他银行使用架构允许的 banked MRS。它不切换模式；在 R52 的非 Hyp 模式下，Hyp 的 SP／ELR／SPSR 返回 Access restricted；User 模式在读取 DSPSR 后拒绝专用银行访问，不尝试 MIDR／banked MRS，当前用户寄存器仍在 Core 中读取。System 模式可读取其他银行。当前仅适配 Arm D13 Cortex-R52，R52+ 专用身份仍待确认。

同一服务租约涵盖前后线程／帧检查和单次 target 事务；Scope All 仍只读选中核。后端保存、恢复并物理回读 R0，再核对 DSPSR 的完整停止状态一致，成功结果固定 8 位十六进制。线程／帧变化丢弃样本；恢复不确定时进入 FAULT、停用共享通道，显式重连前不重试。生产事务和实际二进制软件用例已验证；物理探针和板卡尚未验证，延后驱动见 [测试说明](tests/README.md)。银行 reader 不开放 writer 或使能 FPU。

### 读取 R52 浮点与向量视图（开发分支）

R52 目录将 Single、Double、Quad 分组，并以独立 `vfp` reader 读取 D/Q 和 FPSID、FPSCR、MVFR0/1/2、FPEXC；S 是 D 的原始位别名。工程显式配置 `registers.vfp_command = "aarch64 vfp"`、TCL endpoint 和各核 target，使用固定适配后端。配置为空或协议不匹配时返回 Reader unsupported，不回退 GDB 或旧 get_reg。

先 **Probe caps** 获取实际 MVFR/FPEXC 证据，再按需 **Read**。未知实现条件不会自动尝试数据；手动读取会由后端重新核验权限。当前专用读取要求暂停物理 frame 0、实际 R52 D13 身份、Hyp 模式和 HCPTR.TCP10=0。EL1/Guest/User 的合法读取路径仍待适配，当前显示 Access restricted，不等同于未实现。

MVFR0/1 确认 SP-only D16 或 DP/NEON D32；D16 仍有 64 位 D 存储，但不支持双精度运算或 Q 视图。FPEXC.EN=0 时标识和 FPEXC 可读，数据与 FPSCR 显示 Feature disabled。程序不修改模式、CPACR/HCPTR/FPEXC/FPSCR 或 FP 数据。未知／矛盾 MVFR 保留原始证据，不授权高 D 或 Q 读取。

同一请求内 S/D/Q 共享精确 128 位物理 pair，保留 NaN 载荷、负零及高位；缓存不跨请求／核心／停止点／帧。Scope All 只读选中核。后端恢复回读 R0/R1，并复核 DSPSR、HCPTR、FPEXC；异常未知进入 FAULT，重连前不重试。软件用例和 Windows/Linux 构建通过，物理执行按本任务要求未测试。REG-H03 驱动和独立固件钩子见 [测试说明](tests/README.md)。

### 编辑 R52 浮点与向量原始位（开发分支）

S/D/Q 的 **Edit value** 使用独立 writer。专用 reader 可用或目录标有 RW，都不足以开放写入；使用包含写协议的固定源码适配后端，并在工程中显式配置：

```toml
[registers]
cpu = "cortex-r52"
tcl_endpoint = "127.0.0.1:6666"
vfp_command = "aarch64 vfp"
vfp_write_command = "aarch64 vfp_write"
[registers.targets]
core0 = "board.cpu0"
core1 = "board.cpu1"
```

上例 target 名称须替换为实际映射；现有 stock xPack 不提供此协议。当前写入只适配已确认的 R52 D13、Hyp、HCPTR.TCP10=0、FPEXC.EN=1、暂停的物理 frame 0，且遵守 GDB `may-write-registers`。实际 MVFR 确认 D16/D32；D16 的 64 位 D 存储可写，高 D 和 Q 不开放。R52+ 实际身份、合法 EL1/Guest/User 路径以及 FPSCR/FPEXC 等控制 writer 仍待适配。

在 Single、Double 或 Quad 中选中条目，按 `e` 或点 **Edit value**，输入后 **Preview**，核对物理核、target、通道和位宽，再显式 **Apply** 或 **Cancel**。S/D/Q 分别接受精确 32/64/128 位字符串；浮点输入仅适用于 S/D，NaN payload 应使用原始十六进制，字节输入需明确 LE/BE。Scope All 始终只写选中核，不广播。

Headless 使用同一 `write_preview` / `write_apply` / `write_cancel` 接口。先调用 `registers_list`，用其返回的当前 `context` 替换以下示例：

```json
{"id":10,"method":"write_preview","params":{"context":{"session":17,"generation":3,"core":"core0","frame":0},"target":{"kind":"register","id":"q15"},"selection":{"kind":"register"},"input":{"kind":"unsigned","text":"0x8123456789abcdef7ff0123456789abc"}}}
```

Preview 不写 FP 数据，返回完整原始 pair 及单次草稿令牌。Apply 重新核对上下文、权限、协议和物理容量；后端以发送时的新鲜 pair 保留 S/D 相邻位。Q 使用两次 D 写入，`atomic=false`，部分执行可能发生；不通过 GDB LONGEST 写 128 位数据，不修改模式或使能 FPU。

结果 `verified` 表示完整 pair 回读一致，`mismatch` 包括邻接位变化，`not_sent` 表示已知未写，`unknown` 表示可能已改变存储。写后上下文变化时，已验证结果显示 `accepted`，已有 mismatch 保留。未知结果使共享通道进入 FAULT；不重试或自动恢复旧值。成功或未知写入使旧样本、别名、其他草稿及关联视图失效；重连不会重放草稿。

软件验证与延后上板步骤见 [VFP 写入验收](tests/cases/register-vfp-writes.md)。实际 DebugTUI 驱动默认报告 skipped；本任务没有执行板卡测试。

### 读取 MPU／PMU 选择器组（开发分支）

普通 **Read** 继续使用 PRBARn／PRLARn、PMEVCNTRn／PMEVTYPERn 的直接索引通道。需要选择器通道时，在暂停物理核心的 frame 0 先 **Probe caps**，选中对应区域／事件计数器，再点击 **Read bank** 或执行 `:register-bank-read`。Scope All 仍只操作当前核心。区域索引必须小于实际 MPUIR／HMPUIR 数量；当前适配 R52 的 16／20／24 区域和最多 4 个 32 位 PMU 事件计数器，EL2 要求 Hyp。选择 PMU index 31 读取计数器不受支持，但保存的 PMSELR=31 可以原样恢复。

工程需显式配置经过实际 OpenOCD 构建核对的命令对，例如：

```toml
[registers]
cpu = "cortex-r52"
tcl_endpoint = "127.0.0.1:6666"
cp15_command = "arm mrc"
selector_command = "arm mcr"
[registers.targets]
core0 = "core.0"
core1 = "core.1"
```

`selector_command` 默认空，`arm`／`aarch64` 命令对不能混用；target 必须是实际物理核心名称。官方 [OpenOCD 命令文档](https://openocd.org/doc/html/Architecture-and-Core-Commands.html) 定义 `arm mcr` 的值为最后一个参数，命令存在本身不证明当前后端已支持 R52。不要直接把示例中的 endpoint／target 当作板级映射。

一个 TCL 请求内保存 target 和原选择器，核对实际 MIDR／数量及暂停状态，选择索引、同步、读取成对值，再恢复并回读原选择器及 target。同步只使用 R52 实现的旧 CP15ISB，并要求预先观测到当前 SCTLR／HSCTLR.CP15BEN 已设置；未设置时在首次 MCR 前拒绝，程序不会打开该位。该状态下仍可尝试普通直接 **Read**；完整支持需要后端提供经过验证的 ISB 路径。读取不修改区域配置、事件类型或计数值，也不使能／清空 PMU。

Headless 使用 `registers_select`，参数为当前 `context`、`kind`（`mpu_el1`／`mpu_el2`／`pmu`）及 `index`。响应保留 saved／restored、同步说明、成对原始样本、实际 target；MPU 另返回基址、包含末地址的限址、使能、AP／XN／SH 和 AttrIndex。该单个选择器事务只返回 AttrIndex；完整 MAIR 解释见 MPU regions 总览。SH 的含义注明 Normal memory。普通错误只使本次成对值不可用，旧值保留时间说明；身份／数量变化废弃能力缓存。选择器／target 恢复失败或结果未知使共享通道进入 FAULT，显式重连前不继续访问。该路径经过实际 Tcl 软件夹具验证，尚未上板。

### MPU 区域与内存属性总览（开发分支）

在 System Regs 点击 **MPU regions**，或使用 `:mpu el1`／`:mpu el2` 打开当前核心的区域总览。打开窗口、滚动和切换 EL1／EL2 只显示已有采样。方向键、PageUp／PageDown、Home／End 或滚轮浏览，Tab／Shift+Tab 选择底部按钮；`b` 切换组、`p` 显式 Probe、`r` 显式 Read、Esc 关闭。

先在暂停核心的物理 frame 0 执行 Probe，再选择 **Read**。批次重新检查实际 GDB 线程／帧、CPSR 模式、MIDR 和 MPUIR／HMPUIR 数量，然后读取全部已实现的直接 PRBARn／PRLARn 或 HPRBARn／HPRLARn，以及相应 MAIR、SCTLR／HSCTLR；EL2 另显示 HCR 和 HPRENR。数量、身份、模式或上下文变化时丢弃结果并要求重新 Probe。读前验证整组目录编码，整个批次持有服务锁，不修改选择器或控制寄存器；Scope All 仍只读取当前物理核心。未实现的 EL2 MPU 不访问区域或 MAIR。

每个区域显示 64 字节对齐的基址、包含末地址的限址、区域使能、AP、XN、SH 字段和 MAIR AttrIndex。MAIR 解码区分 Device 四种属性、Normal 的内外缓存策略及读／写分配提示，并保留 UNPREDICTABLE 编码。R52 忽略 transient 提示；SH 字段的解码适用于 Normal memory，Device 与 Normal non-cacheable 的实现行为另有说明。全局 MPU 开关与背景区开关分别来自对应 SCTLR／HSCTLR，不把区域 EN 当作全局使能。

失败项目逐项显示原因及原始证据；缺失、过期或其他核心的数据不会参与地址／属性推导。一个 MAIR 寄存器不可读不影响使用另一个 MAIR 的区域。总览解释配置，读取为暂停状态下的顺序采样，不保证架构原子性，也不替代给定地址经过 EL1／EL2 组合后实际权限的判定。

Headless 使用 `registers_mpu`，参数为当前 `context`、`bank`（`el1`／`el2`）和可选 `read`。默认 `read=false` 仅解释当前缓存，不发送调试器请求；`read=true` 执行上述直接读取并返回 `samples`、`view` 与实际 owner。接口不支持通过用户目录把 MPU 固定动作重定向到有副作用的条目或共享 owner。

### 编辑 Core 寄存器（开发分支）

开发分支新增的 Core 寄存器编辑尚未进入 v0.9.3 release。选择暂停核心的物理 frame 0，在 System Regs 选中 r0–r12、SP、LR 或 PC，点击 **Edit value**（或按 `e`、输入 `:edit-value`）。填写数值后先 **Preview**，核对对象、owner、位宽、掩码、实际 GDB endpoint 和影响，再明确 **Apply**；**Cancel** 丢弃未发送草稿。Tab／Shift+Tab 切换输入和按钮，Ctrl+U 清空数值。Bytes 格式明确显示 LE／BE，可用左右键改变字节序。

修改输入必须重新预览；切核、帧、运行、重连或换 ELF 后旧草稿不可应用。Core writer 的 Scope All 仍只写当前核心；共享区域只执行一次所属 owner 的写入。`verified` 表示按有效掩码回读一致，`accepted` 表示后端已受理但没有完成安全验证，`mismatch` 表示回读不符，`unknown` 表示可能已写入而无法确定结果；不自动重试、回滚或重放。发送后关闭编辑窗口不会撤回操作。PC/SP 改动会使源码、栈、Locals、反汇编等视图失效并重新读取；这不是通用的目标恢复操作。

未声明独立 writer 的对象可查看原因，当前 CPSR、系统／银行／浮点寄存器 writer 仍在开发。Watch／Locals 的标量及成员已接入 GDB 类型赋值。RAM 和 8/16/32 位 SVD MMIO writer 的声明方式与限制见下方。数值规划器支持 128 位，并不表示 GDB 整数 writer 能写 128 位向量；完整范围与验证限制见 [开发进度](docs/registers-development-status.md)。

### 编辑 Watch／Locals 变量（开发分支）

在 Watch 或 Locals 选中标量／展开成员，点击 **Edit value**、按 **e** 或执行 `:edit-value`。Locals 的左右箭头、Enter 和成员旁的箭头可展开／收起；编辑绑定当前线程和栈帧，普通 RAM 局部变量可在调用者帧中修改。成员路径会在 Preview 和 Apply 各自重新创建、解析和检查，GDB 变量对象名称不会作为持久标识。

Unsigned、Signed、Float、Bytes 输入会按 GDB 的实际类型与宽度校验；Bytes 必须选择字节序。普通存储要求声明覆盖根对象及所选成员的 GDB RAM 区域；所选标量的宽度和对齐必须允许。寄存器驻留变量只有 GDB 明确确认且处于物理 frame 0 才接受。GDB 的 editable 属性不能覆盖 const、volatile 或项目区域权限，显式 AP Watch 不会自动改走 GDB。指针值与其指向对象分别检查，修改指针值不会写入其指向对象。

赋值通过 `-var-assign` 保留类型语义；检查和赋值期间禁用 GDB 目标函数调用，并恢复原设置。表达式限定为变量、成员、解引用和常量数组下标；不接受函数、赋值、算术、转换或动态下标。不可赋值、优化掉、作用域／类型／地址变化会拒绝；未知结果不重试。当前已用真实本机 GDB 验证 32/64 位整数、指针、结构体／数组成员及 float/double，包括负零。128 位变量表达式仅有纯软件验证；引用、位域、long double、NaN/Infinity 尚需适配，不能绕过类型 writer 改写原始内存。[GDB 变量对象接口](https://sourceware.org/gdb/current/onlinedocs/gdb.html/GDB_002fMI-Variable-Objects.html)

### 声明 RAM 与外设写入区域（开发分支）

参考 [区域模板](profiles/write-regions.toml.example) 配置 `[[writes.regions]]`。必须声明地址区间（`end` 不包含）、`kind`（ram/flash/mmio）、`channel`（空字符串为当前核心 GDB）、`scope`、允许的 `widths` 及必要的 `little_endian`。RAM 字节 writer 还要求 `byte_writable=true` 与允许 8 位访问；读取通道配置不会授予写权限。按芯片手册核对模板地址和 target 后再使用。

在 Memory 或 Peripherals 选择 **Edit value**，或按 **e**。Memory 可修改字面地址和 1–4096 字节范围；Bytes 输入按地址递增顺序，例如 `12 34 56 78`。标量采用区域声明的字节序，并检查宽度和对齐；不接受会执行函数、赋值或解引用的地址表达式。Flash 提示使用 Download，未知区域或从 RAM 入口访问 MMIO 均拒绝。

Peripherals 使用加载的 SVD 的实际寄存器地址、字段权限和特殊写语义。先在 Memory access 选择已声明的 TCL 通道；当前 writer 只执行单个对齐 8/16/32 位 `target write_memory`，64 位 MMIO 及 GDB MMIO writer 尚未适配。TCL 区域需列出 owner 对应的物理 CPU `halted_targets`；AP 的状态不能替代 CPU 暂停证明。布局字节序来自区域声明或 SVD，实际总线 target 字节序通过 `cget -endian` 查询并转换。[OpenOCD target 命令](https://openocd.org/doc/html/CPU-Configuration.html)

RO 权限不会被 override 开放。只有目标手册确认保留位、RO 写效果或稳定验证掩码时，才添加 `[[writes.svd_overrides]]`。普通字段修改保留事务内新鲜读取的邻接 RW 位；无关 W1C/W0C 位使用不动作值。WO 或读副作用寄存器可执行不需要读取的写入，但不会自动回读；后端受理后显示 accepted。缺少一次写／解锁规则时拒绝。

所有写入要求暂停，RAM/MMIO 仍绑定当前帧但无需物理 frame 0。共享 chip/cluster owner 要求协调器确认相关核心暂停；Scope All 仅执行一次该 owner 写入，共享写入会使其他核心的缓存与草稿失效。服务锁只防止 DebugTUI 命令交错，不保证外部调试器、多主设备或多字节访问的原子性。错误后不自动重试或恢复旧值。

源码使用 Rust 2024 edition；构建需安装能编译当前锁定依赖的 Rust 工具链及 Windows 原生链接器。GNU 构建可用 `DEBUGTUI_GCC_DIR` 指定工具目录。Node 用于分发和部分测试，完整功能验收还需要 PowerShell 7、本机 GCC/GDB；这些不是最终用户启动 EXE 的依赖。

在仓库根目录按需执行：

```powershell
./scripts/build.ps1 -Test
./scripts/build.ps1 -Release
node ./scripts/test-functional.cjs --binary ./target/debug/debugtui.exe --gdb C:/MinGW/bin/gdb.exe --cc C:/MinGW/bin/gcc.exe
```

`build.ps1` 使用锁定依赖；首次运行功能套件前确保对应 debug EXE 已生成。统一入口目前默认执行 20 个软件套件；覆盖范围、运行条件和硬件专项以 [测试说明](https://github.com/bei-li16/DebugTUI/blob/main/tests/README.md)、仓库 `tests/functional-coverage.json` 和实际报告为准，不把未执行项计为通过。

已有 STM32F429、THA6206 MCAL 单核/双核及 Bao 等调试记录，具体板卡、固件、探针、项目和限制见 [TESTING.md](TESTING.md)。模拟 GDB/TCL、终端渲染与配置测试不能替代实板验证；当前发行与宿主验收范围为 Windows，不代表其他平台或任意架构组合均通过。

打包与发布见 [PUBLISHING.md](PUBLISHING.md)，设计见 [ARCHITECTURE.md](ARCHITECTURE.md)。详细排障历史和验收数据留在测试记录，使用指南描述当前行为；待开发计划不等于已有功能。

PMU 查看：配置 registers.pmu_command="aarch64 pmu" 可显式选择当前开发后端的独立只读协议。该适配覆盖 R52 四项 32 位事件与完整 64 位周期及相关状态/控制视图；观察不会启动或清空计数。当前 Debug EL2 与停止前 CPSR 分开证明，低 EL 无法证明 Hyp 陷阱时显示 Unknown。计数数量 Probe 需要新鲜原生证据；同名 GDB PMCR 不替代该证明。直接 PMEVCNTRn/PMEVTYPERn 保持 PMSELR，SEL=31 的 PMXEVCNTR 不可用。后端、配置、字段、未上板限制见 [PMU 说明](docs/register-pmu.md)。

GIC 查看：开发版本显式配置 `registers.gic_command="aarch64 gic"` 后使用独立观测协议。物理 ICC、Hyp ICH 与虚拟 ICV AP backing 别名分别展示；R52 只实现每组 AP0R0/AP1R0，Probe 以当前 Debug EL2 证据分别确认物理/虚拟五位容量并过滤其他 AP 定义。停止前 Hyp 或同名 GDB CTLR/VTR 只提供原始字段，不能授权物理容量。IAR 有 acknowledge 副作用，观测后端即使手工也拒绝；EOIR/DIR/SGI 为 Write only，不安排读取。低 EL 保持 Unknown，不关闭陷阱或使能接口。LR/LRC 各为 32 位独立样本，详情保留接口、target、owner 与时间区间。配置和边界见 [GIC 说明](docs/register-gic.md)；当前发行 0.9.3 尚未包含新字段。

### R52 MMIO 的逐 owner 路线（开发分支，尚未发布）

GICD 按 processor cluster 共享，GICR 与外部 Debug 按物理 core 私有。使用 [配置片段](profiles/tha6-mmio.toml.example) 的 registers.component_owners，层次为组件名 → 完整 owner → base/channel/little_endian；例如 gicr/core:core.2 与 gicd/cluster:A。示例地址均为虚构，present=0 默认禁用，必须先按板级资料核对再使用。GICR base 指控制页，SGI/PPI 页在其后 0x10000；不从 core 编号或 Aff0 推导地址。

channel 为空使用当前 GDB 内存连接，非空引用既有 memory_access ID；继续检查实际 endpoint、target、core filter 和状态。组件声明 owner map 后，缺失 owner 或 cluster 不回退静态 components 或其他核心。新内置 MMIO reader 要求 owner map；旧用户静态 MMIO 定义兼容。Scope All 仍只读选中 owner，64 位 AP 读取是两个 32 位字，不保证原子快照。

默认兼容模式的 present、gicd.interrupts、Debug comparator 数量来自配置，来源显示 configuration；普通 TYPER/Debug ID 数据读取不会自动授予能力。确认板级映射后，可设置 registers.mmio_probe=true，再显式执行 Probe，以42个只读身份/容量请求形成当前 owner/context 的物理证明；启用后无有效证明的数据读取拒绝，配置与观察仍分别显示。步骤及界限见 [新鲜 MMIO Probe](docs/register-mmio-probe.md)。Unknown 不自动读取，No/WO 不读取。EDPRSR、PC sample 与 TX 使用手工策略；外部 RX read 不清 RXfull，不等同于 CPU 接收操作。RW 标签不开放 MMIO 编辑。精确地址、副作用和未完成范围见 [MMIO 说明](docs/register-mmio.md)，[延后环境用例](tests/cases/register-mmio.md) 默认不访问目标。
