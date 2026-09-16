# DebugTUI 验证记录

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

真实会话日志位于 artifacts/launch ui 工程 20260916-121245/A/logs 和 B/logs。其他回归批次为 environment test 工程 20260916-121306、native-gdb-20260916-121310、hardware-20260916-121502、lifecycle-20260916-121510。配置页操作说明见 README。

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
- 变量树、SVD、FreeRTOS 专项面板、源码编辑器、多人共享会话尚未实现。
- npm Registry 发布、其他 npm 版本和其他操作系统未验证；本地安装、替换、卸载及程序调试已验证。
- `session.on_exit=resume` 是当前后端支持的退出策略。强制终止后目标状态需要重新连接确认。
