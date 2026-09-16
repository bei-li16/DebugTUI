# DebugTUI

基于 GDB/MI 的原生终端调试工作台。当前版本 0.2.0，发布构建支持 Windows x64；TUI 不绑定芯片、探针或 GDB Server，不需要 Python 或 Node 常驻进程。

GitHub：[bei-li16/DebugTUI](https://github.com/bei-li16/DebugTUI)。源码使用 Apache-2.0；依赖声明见 NOTICE。

## 运行方式

安装或升级到 GitHub Release 最新正式版（Windows x64）：

~~~powershell
npm install -g --prefer-online "https://github.com/bei-li16/DebugTUI/releases/latest/download/debugtui-cli.tgz"
~~~

安装后直接输入名称即可打开终端内的启动配置页：

~~~powershell
debugtui
~~~

无需记忆启动参数。在页面中选择工程目录或 debug.toml，选择 tools 目录或环境 TOML、GDB、ELF/本机程序、源码目录，并设置目标模式、地址、服务启动、超时和退出策略。

| 配置页操作 | 按键 |
|---|---|
| 选择字段 | Tab / Shift+Tab / ↑ ↓ |
| 编辑路径或参数 | Enter；Ctrl+U 清空；Enter 应用，Esc 取消 |
| 浏览文件/目录 | F2；Enter 进入目录或选择文件；Space 选择当前目录；Backspace 返回上层 |
| 切换枚举/开关 | ← → / Enter |
| 保存配置 | Ctrl+S |
| 开始调试 | F5；默认保存到工程的 debug.toml，可关闭 Save to project 仅连接一次 |
| 从调试页面重新选工程/环境 | F2 或 :setup |

无参数启动始终先显示配置页，即使当前目录已有 debug.toml；不会直接占用探针。选择工程后优先读取其配置；没有显式 tools/GDB 配置时，会发现该工程内的 tools/debug-env.toml。保存路径尽量相对于工程，保留构建、源码映射、监视与断点等原有配置，不展开并复制整份 tools 配置。

配置页打开时当前调试会话仍然有效。按 F5 切换时，先清理原会话，再启动新工程；旧会话清理失败会在界面报错并停止切换。连接失败可以按 F2 修正配置并重试。

原有参数仍可使用：配置完整时直接准备调试环境；参数不足时进入已填好参数的配置页。添加 --setup 可强制先查看配置。--project 同时接受工程目录和配置文件。

~~~powershell
debugtui --project ./工程目录
debugtui --project ./工程目录 --setup
~~~

TUI 包只包含程序、说明和许可证。需要用户提供与目标架构匹配、支持 GDB/MI2 的 GDB。GDB 自身需要的 DLL、资源和服务由对应环境维护。

连接已经启动的调试服务：

~~~powershell
debugtui --gdb C:/toolchain/bin/arm-none-eabi-gdb.exe --connect localhost:3333 --elf ./build/app.elf
~~~

调试本机程序：

~~~powershell
debugtui --gdb C:/MinGW/bin/gdb.exe --local --elf ./build/app.exe
~~~

使用 tools 中的环境配置，保留 STM32/J-Link 的一键连接：

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

--target-mode 支持 remote、extended-remote、local。--gdb-arg 可以重复使用。符号文件可省略；缺少符号时，源码和变量功能受 GDB 提供的信息限制。未配置连接目标时，在配置页选择环境，或使用 F2 / :setup 补齐参数。GDB 参数数组使用 JSON 格式；环境变量、Server 参数和特殊动作等高级配置继续放在环境 TOML 中。

~~~powershell
debugtui --demo
debugtui --snapshot ./preview.txt
~~~

## 环境与项目分层

TUI 只处理通用的 GDB/MI 请求、事件、界面和进程生命周期。可选环境文件描述如何准备 GDB；芯片、探针、下载和复位策略全部位于 tools。

| 配置 | 职责 |
|---|---|
| 项目 debug.toml | 程序/源码路径、监视、断点、环境引用、构建命令 |
| tools/debug-env.toml | GDB 路径/参数、服务启动、连接、芯片相关动作 |
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
~~~

也可以完全不用 tools，使用 debug.toml.example 的独立 GDB 配置。

环境中允许 gdb、target、service、actions、session 五个表。先加载环境，项目配置按字段覆盖环境，最后应用 CLI 参数；数组整体替换，空数组可以清除环境动作。相对可执行路径（含 / 或 \）和 cwd 相对定义它们的 TOML 文件，裸命令名通过 PATH 查找。环境文件中的 ${profile_dir} 展开为该环境文件所在目录，可用于资源路径参数。

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
on_exit = "detach"
~~~

GDB 和 service 均可配置 args、cwd、env、unset_env。TUI 默认继承环境，仅设置 LC_ALL=C；具体工具所需的环境变量调整由配置声明。

可选 service 表支持 command、args、cwd、env、unset_env、ready、timeout_ms、enabled。ready 是就绪日志片段列表，匹配 stdout 或 stderr；为空表示启动后直接尝试 GDB 连接。仅管理本程序启动的进程。项目可以用 [service] enabled=false 使用外部服务。

actions 支持 restart、run、download、before_disconnect；target.after_connect 和 gdb.init 也是命令数组。以 - 开头的命令按 MI 发送，其余按 GDB 控制台命令发送。不执行 shell 拼接。restart/download 未配置时不可用；run 未配置时使用标准 -exec-run。任一动作失败立即停止后续动作。

退出策略：detach 使用 -target-detach；resume 先继续再 detach；disconnect 使用 -target-disconnect。关闭 GDB、停止自有服务后目标是否继续运行取决于服务端，TUI 不承诺硬件停机状态。STM32/J-Link 配置声明 monitor go 后 detach；原有服务退出恢复运行的特性由该环境负责说明。

## 操作

| 操作 | 按键/命令 |
|---|---|
| 选择工程、tools 和启动参数 | F2 / :setup |
| 继续；本地 READY 状态启动程序 | F5 |
| 暂停 | F6 / Ctrl+C |
| 切换源码断点 | F9 |
| 单步越过 / 进入 / 跳出 | F10 / F11 / Shift+F11 |
| 切换面板 | Tab / Shift+Tab |
| 命令面板 | Ctrl+P |
| 搜索源码 | Ctrl+F |
| 工作台命令 / GDB 控制台 | : / / |
| 退出 | Ctrl+Q / :quit |
| 帮助 | ? |

~~~text
:connect
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

寄存器名称和编号全部从 GDB 动态获取，没有 ARM 白名单。内存默认地址为 $sp。run/r 默认保留 GDB 启动语义；只有显式加载环境中的 actions.run 时才执行自定义流程。下载操作执行前需要界面确认。

支持源码、监视、局部变量、栈、寄存器、内存、反汇编、断点和控制台。目标运行时显示上次暂停快照。FreeRTOS、SVD、内置源码编辑器尚未实现。源码由项目提供；路径映射使用 [[source_map]] 的 from/to。

可选 [build] 包含 command、args、cwd；构建前需断开会话，Ctrl+Q 可以取消构建并清理子进程。日志默认不落盘，--log-dir 开启后每次连接最多 8 MiB。

## 人工和自动化

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

## 安装与分发

从 0.2.0 起，npm tgz 和便携 ZIP 都不包含底层工具。可选 tools ZIP 单独构建、单独更新。当前已有的 v0.1.0 Release 保留原来的集成包；本次源码变更不会覆盖旧附件。

每个正式 Release 都上传固定名称 debugtui-cli.tgz；包内版本号随版本递增。上方 latest 命令用于首次安装及后续升级，每次运行都会查询在线版本。它使用 GitHub 的最新正式 Release；尚未发布到 npm Registry，因此不使用 npm install @debugtui/cli@latest 或 npm update 来追踪 GitHub 版本。

便携 ZIP、版本化 npm 包和可选工具包见 [最新 Release](https://github.com/bei-li16/DebugTUI/releases/latest)。将所需工具包解压到代码工程中，使工程包含 tools/debug-env.toml。

~~~powershell
npm install -g ./artifacts/debugtui-cli-0.2.0.tgz
debugtui --version
npm uninstall -g @debugtui/cli
~~~

原生程序直接运行，不依赖 Node 常驻进程。npm Registry 尚未发布，包名 @debugtui/cli 需确认作用域权限；发布流程见 PUBLISHING.md。Git 不跟踪 DebugTUI 编译产物，bin/、target/、artifacts/ 已忽略。

## 从 0.1.0 迁移

- 将旧项目的 [server] 配置移到 tools/debug-env.toml 的 service/target/actions。旧 [server] 会明确报迁移错误。
- --device/--port 已移除：修改环境中的服务参数及连接地址；可复制环境配置来维护多个板卡。
- on_exit 不再强制 resume；J-Link 的恢复策略在环境的 before_disconnect 中。
- 使用 --tools-dir 或 [tools] root 引用现有 tools 时，目录内必须有 debug-env.toml。
- npm 升级后底层工具需要在安装目录之外独立保存，并显式引用。

## 开发与验证

需要 Rust 1.88+、Windows 原生链接器；GNU 构建可设置 DEBUGTUI_GCC_DIR。Node 仅用于 npm 和开发时的严格 MI 模拟测试，不进入调试运行链路。

~~~powershell
./scripts/build.ps1 -Test
./scripts/build.ps1 -Release
./scripts/test-gdb-environment.ps1
./scripts/test-native-gdb.ps1 -Gdb C:/MinGW/bin/gdb.exe -Compiler C:/MinGW/bin/gcc.exe
./scripts/release-assets.ps1 -SkipBuild
./scripts/test-npm.ps1
./tools/package.ps1
./scripts/test-hardware.ps1 -Elf path/to/FreeRTOS_Project.elf
./scripts/test-lifecycle.ps1 -Elf path/to/FreeRTOS_Project.elf
~~~

硬件测试针对 tools 中的 STM32F429IG/J-Link 环境和既有 FreeRTOS 固件；RISC-V 模拟验证不代表已经通过 RISC-V 实板测试。现阶段只验证 Windows 宿主；目标架构独立不等于已验证所有宿主操作系统。详见 TESTING.md。
