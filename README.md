# DebugTUI

GitHub：[bei-li16/debugtui](https://github.com/bei-li16/debugtui)。DebugTUI 源码使用仓库的 Apache-2.0 许可证，附带第三方程序保留各自许可证，见 `NOTICE`。

在终端中直接运行的 STM32 调试工作台。Rust 原生 EXE + GDB/MI，附带最小 GDB/J-Link 工具集。没有 Python、Node 常驻服务或浏览器界面。

界面采用 Ratatui + Crossterm：在 Windows Terminal / PowerShell 所在终端内绘制彩色面板，支持键盘、鼠标和窗口尺寸变化。`--demo` 展示的就是实际 TUI 渲染器。推荐 120×36 字符，最低 45×12；80×24 会折叠侧边变量区，可切换 Watch 面板查看。

## 运行

从 [GitHub Releases](https://github.com/bei-li16/debugtui/releases) 下载对应版本的 `debugtui-VERSION-win-x64.zip`，解压后可直接运行，不需要 npm：

```powershell
.\debugtui.exe --demo
.\debugtui.exe --elf path\to\firmware.elf
```

保留 EXE 旁的 `tools` 目录。Release 同时提供 npm 本地安装包 `.tgz` 和 `SHA256SUMS.txt`。通过 npm 安装后可使用下面的 `debugtui` 命令。

```powershell
debugtui --project .\debug.toml
debugtui --elf .\Debug\app.elf --tools-dir .\tools
debugtui --elf .\Debug\app.elf --connect localhost:3333
```

托管模式自动启动 J-Link Server、加载 ELF 并暂停目标。`--connect` 使用已经运行的服务；可以先运行 `tools/start_server.bat`。普通连接不自动下载固件。Windows x64，默认 STM32F429IG / SWD 4000 kHz / 3333。

直接打开工作台时，可以用 `:elf PATH` 选择固件，再执行 `:connect`。`--demo` 可在无探针时体验真实终端界面。

## 键盘与界面

| 操作 | 按键 / 命令 |
|---|---|
| 继续 / 暂停 | F5 / F6，Ctrl+C 暂停目标 |
| 源码行断点 | F9，或点击源码左侧行号 |
| 单步越过 / 进入 / 跳出 | F10 / F11 / Shift+F11 |
| 切换面板 | Tab / Shift+Tab，或点击顶部标签 |
| 命令面板 | Ctrl+P |
| 当前源码搜索 | Ctrl+F，或 `:find TEXT` |
| 工作台命令 | `:` |
| GDB 命令输入 | `/` |
| 帮助 | `?` |
| 结束会话并退出 | Ctrl+Q，或命令栏 `:quit` |

支持源码高亮、当前执行行、变量监视、局部变量、调用栈、寄存器、内存、反汇编、断点列表、源码文件列表和调试控制台。目标运行时显示上次暂停的数据快照。

```text
:watch xTickCount
:unwatch xTickCount
:data-break Log_Tx_En
:break main
:delete 2
:memory 0x20000000 256
:disasm $pc
:files
:open path/to/main.c
:frame 1
:restart
:download
:disconnect
```

Watch 列表只在暂停时读取表达式；`:data-break` 才设置硬件观察点。下载操作确认后写入所选 ELF，复位并暂停。`:restart` 复位并暂停，F5 继续运行。

普通 GDB 命令直接输入，例如 `p/x g_w25q_jedec_id`、`x/8wx 0x20000000`、`set variable flag=1`。GDB 控制台与面板使用同一个持续的 GDB 会话。

底部 Console 保留命令和 GDB 输出，完整服务输出在 Log 面板。TUI 内的 `run` / `r` 映射为 MCU 复位后继续运行；`:restart` 复位后暂停。

## 项目配置

复制 `debug.toml.example` 为工程自己的 `debug.toml`。路径相对于配置文件解析，不依赖启动目录。支持 `tools.root`、`program.elf`、`program.source_root`、`server`、`session`、`watch`、`breakpoints`。

退出时把监视列表和普通断点保存到已有的项目配置；临时断点触发后不再保存。未指定项目文件时不自动创建配置文件。

源码迁移映射：

```toml
[[source_map]]
from = "C:/old/project"
to = "../firmware"
```

可选构建命令（先断开调试，再 `:build`）：

```toml
[build]
command = "cmd.exe"
args = ["/d", "/c", "Build.bat"]
cwd = "AutoCMakeTool"
```

构建期间 Ctrl+Q 可取消构建并退出，相关子进程会清理。

构建工具和 ELF/源码由工程提供。工具 EXE/DLL 固定使用所选 tools 副本。Windows 系统组件和 J-Link USB 驱动由电脑提供。

TUI 的连接参数来自 `debug.toml` / CLI；BAT 的参数来自 `tools/config/paths.bat`。TUI 直接启动对应 EXE，不执行 BAT 配置。使用外部模式时，确保 `--connect` 的端口与 BAT 参数一致。

当前 J-Link 后端退出时恢复 MCU 运行，`session.on_exit` 只接受 `resume`。J-Link Server 正常退出会恢复目标运行，无法通过先发送 `halt` 再退出保证目标继续暂停；需要保留暂停状态时保持会话打开。参见 [SEGGER 退出行为](https://kb.segger.com/J-Link_GDB_Server#Program_termination)。强制终止程序会清理其子进程，但目标状态需重新连接确认。

日志默认不写磁盘；指定 `--log-dir` / `session.log_dir` 后，每次连接最多记录 8 MiB。不同连接保留独立日志，目录可自行归档清理。

## 自动化与 AI

同一个 EXE 支持 JSON Lines 接口；不需要额外守护进程。

```powershell
debugtui --project debug.toml --headless --stdio
debugtui --project debug.toml --script commands.jsonl
```

请求示例：

```jsonl
{"id":1,"method":"connect"}
{"id":2,"method":"evaluate","params":{"expression":"xTickCount"}}
{"id":3,"method":"break","params":{"location":"DEBUG_PRINTF","temporary":true}}
{"id":4,"method":"continue"}
{"id":5,"method":"wait_stopped","params":{"timeout_ms":10000}}
{"id":6,"method":"quit"}
```

输出类型：`snapshot`、`log`、`response`、`exit`。`response.id` 对应请求编号；`ok:false` 包含错误。`continue/step/next` 返回表示请求已提交，使用停止事件或 `wait_stopped` 确认实际停止。

脚本模式遇错停止后执行断开清理，进程退出非零。交互 JSON 模式允许调用方处理单条错误继续操作。一次会话只使用一个 GDB；不要同时让另一个 GDB 占用相同探针。

## npm 安装与升级

维护者首次发布、登录、确定包名和版本升级步骤见 [PUBLISHING.md](PUBLISHING.md)。

包名暂为 `@debugtui/cli`，发布到 registry 前使用本地构建的 `.tgz` 安装：

```powershell
npm install -g .\artifacts\debugtui-cli-0.1.0.tgz
debugtui --version
debugtui --demo
```

正式发布后可使用 `npm install -g @debugtui/cli@latest` 升级、指定版本回退，使用 `npm uninstall -g @debugtui/cli` 卸载。先退出调试再升级。项目配置和日志应放在 npm 安装目录之外。

npm 注册的命令直接运行 `bin/debugtui.exe`，运行调试不需要常驻 Node 进程。软件附带最小 tools，安装时不运行下载脚本或本地编译。原厂许可证随工具分发。

首次使用 npm 需要 Node.js/npm。安装完成后，Windows 的 CMD/PowerShell 命令入口直接调用原生 EXE；也可以直接运行 EXE。发布只支持 Windows x64，不代表已支持 Linux/macOS。npm 的 `bin` 映射机制见 [官方文档](https://docs.npmjs.com/cli/v11/configuring-npm/package-json/#bin)。

## 开发与验证

需要 Rust 1.88+ 和可用的 Windows 原生链接器。开发缓存不进入 npm 包。

```powershell
.\scripts\build.ps1 -Test
.\scripts\build.ps1 -Release
.\scripts\package.ps1 -SkipBuild
.\scripts\test-npm.ps1
.\scripts\test-build-cancel.ps1
.\scripts\test-hardware.ps1 -Elf path\to\FreeRTOS_Project.elf
.\scripts\test-lifecycle.ps1 -Elf path\to\FreeRTOS_Project.elf
```

如使用 GNU Rust 工具链，设置 `DEBUGTUI_GCC_DIR` 为 MinGW `bin` 目录。发布 EXE 的 DLL 依赖必须仅为 Windows 系统组件。

实板测试脚本针对本次 STM32F429IG FreeRTOS 固件中的 `DEBUG_PRINTF`、`Log_Tx_En` 和 W25Q JEDEC ID 编写；测试其他固件需调整这些断言。`-Download` 额外执行下载和只读段校验。测试输出保存在 `artifacts/`，不进入发布包。验证记录见 [TESTING.md](TESTING.md)，架构见 [ARCHITECTURE.md](ARCHITECTURE.md)。

范围：当前版本集中在源码级调试；FreeRTOS 专项任务列表、SVD 外设位域、内置源码编辑器和多人共享会话接口属于后续扩展。现有 FreeRTOS 应用可通过通用源码、变量和调用栈功能调试。
