# DebugTUI 验证记录

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
