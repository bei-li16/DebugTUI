# DebugTUI

基于 GDB/MI 的原生终端调试工作台。当前版本 0.8.3，发布构建支持 Windows x64；TUI 不绑定芯片、探针或 GDB Server，不需要 Python 或 Node 常驻进程。

GitHub：[bei-li16/DebugTUI](https://github.com/bei-li16/DebugTUI)。源码使用 Apache-2.0；依赖声明见 NOTICE。

## 断点管理

- **Breaks** 行首 `[x]` / `[ ]` 是启用开关，点击或选中后按 **Space / Enter** 切换；禁用保留记录和配置，**Delete** 才删除。源码用 `●` / `○` 区分启用与禁用；对禁用断点按 F9 会重新启用。
- **+ Code**（Insert / n）添加 `file:line`、函数或 `*address` 断点，可选择硬件断点和命中后删除的临时断点。
- **+ Data**（d）添加变量或指针表达式，例如 `xTickCount`、`*(uint32_t *)0x20000000`，可选择 **Write / Read / Read/write**。Write 使用 GDB 的值变化语义；读、读写需要目标支持硬件 watchpoint。数量、宽度、对齐限制由 GDB/调试服务器决定，失败时显示其错误。
- **Edit**（e / 右键）设置启用状态、条件和忽略次数；忽略 N 次表示跳过接下来的 N 次命中。行下方显示当前核 GDB 的实际命中总数与剩余忽略次数。**Enable all / Disable all** 处理当前核列表，并包含列表中多核断点关联的其他核。
- 点击源码或 **+ Code** 默认只在当前核创建。选中已有代码断点后，点击 **Cores** 或按 **c**，勾选目标核心，**Ctrl+Enter / Apply** 应用；**a** 全选，**s** 只保留当前核。多核断点显示 `[N cores]`，底部列出核心名称。启停、条件/忽略次数修改和删除会应用到该断点关联的所有核；回到单核会移除其他核上的关联副本。当前核必须保留，可先切核再更改所属范围。
- 核心选择支持持久代码/硬件断点；临时断点和数据观察点仍按核管理。操作前受影响核心都必须连接并暂停；不会为了修改断点隐式暂停目标。跨核失败会尝试回滚此前的修改，回滚失败同样明确报告。相同位置的独立断点不会自动合并。
- 断点修改立即保存至工程；禁用状态、数据类型、条件与剩余忽略次数跨重连及进程重启恢复。临时断点不持久化。恢复失败的记录标为 `[!]`，仍保留，可重试启用或明确删除。
- 编辑器支持鼠标、Tab / ↑ ↓、Ctrl+U 清空、Ctrl+Enter 应用、Esc 取消。修改断点前先暂停目标；窄终端仍可用快捷键打开编辑器。

控制台兼容 `:break main`、`:data-break counter`，新增 `:data-break read counter`、`:data-break access *(uint32_t *)0x20000000`、`:enable 2`、`:disable 2`、`:disable all`。这些操作通过标准 [GDB/MI 断点命令](https://sourceware.org/gdb/current/onlinedocs/gdb.html/GDB_002fMI-Breakpoint-Commands.html) 执行。

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

## 多核工作区

不配置 `[[cores]]` 时，沿用原单核会话、命令与 JSON 快照格式。多核配置示例（端口和 GDB 命令由工程的调试环境决定）：

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
- Watch、断点分别保存到各自 `[[cores]]`；多核断点记录通过可选 `group` 标识关联；未配置时继承顶层列表，`watch = []` / `breakpoints = []` 表示显式空列表。
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

可用 `gdb.registers` 指定自动读取的寄存器名称，默认空列表读取所有命名寄存器。THA6206 在复位初态自动读取 VFP 的 d/s 寄存器会引发 OpenOCD `DSCR.ERR`；工程可配置 `registers = ["r0", "r1", "r2", "r3", "r4", "r5", "r6", "r7", "r8", "r9", "r10", "r11", "r12", "sp", "lr", "pc", "cpsr"]`。这样保留通用寄存器和程序状态的自动刷新；浮点寄存器可在固件完成初始化后通过 GDB Console 按需读取，或调整列表。程序不会伪造被排除寄存器的值；不存在的寄存器名称会报错。

Headless 支持 `{"method":"control_scope","params":{"scope":"all"}}`；单次 `continue`/`pause` 可带 `{"scope":"core"}` 或 `{"scope":"all"}`，不改变会话范围。快照增加 `control_scope`，组请求返回每核 `results` 和 `cores`；异步断点联停不伪造用户请求响应。`run` 带 `scope:"core"` 可显式只启动当前核。

可选的 `[sync]` 使用 `tcl_endpoint = "localhost:6666"` 与 `open = ["..."]`，在共享服务就绪后、GDB 连接前执行。`method = "tcl"` 或兼容旧配置的 `"cti"` 都执行显式配置的 TCL 命令，不自动识别/配置 CTI。任何命令失败都会中止连接。

可选 `[live_watch]` 配置 `tcl_endpoint`、`bus_target`、`elf`（相对工程路径）、`interval_ms`。解析 ELF32/64 小端文件中的唯一全局 8/16/32 位对象，在运行时直接更新 Watch 值及 LIVE 标记；不再把每次数值变化刷到 Console。数值标为 `<raw N-bit>`，支持显示进制切换，不推断浮点/有符号类型。结构体、数组大对象、局部表达式和类型解释仍由停止后的 GDB Watch 或逐项 Memory access 负责。随切核、停止代次及 Watch 列表变化更新，暂停时恢复 GDB 的值，断线显示错误并重试，断开时取消。读取使用 OpenOCD 的 [target-specific read_memory](https://openocd.org/doc-release/html/CPU-Configuration.html)，不改变全局选中 target；运行时能否读到一致数据取决于芯片、AP 和缓存配置。Headless 输出结构化 `live_watch` 事件，包含表达式、核、generation、地址、位宽、原始值/错误与时间；GDB 停止快照不被实时值改写。

### 0.8：按项选择内存通道与实时刷新

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

TUI 只发送指定 target 的 `read_memory`，不执行全局 `targets` 切换。`soc.ahb` 对应哪个 DAP/AP、借哪个核、如何创建多个 CTI，均放在板级 OpenOCD 配置；AP1/AP3 不具备通用固定含义。多核示例见 [tools/examples/multicore-access.md](tools/examples/multicore-access.md)。已有 `[live_watch]` 配置会直接更新 Watch；显式设置的逐项 Memory access / refresh 优先，包括手动或关闭设置。

**STM32F429 本地实测环境**：[tools/debug-env-openocd.toml](tools/debug-env-openocd.toml)，使用 J-Link 探针 + OpenOCD，M4 与独立 `mem_ap` target 均通过这颗芯片实际的 AP0。在 F2 的 Tools / profile 选择该文件即可获得 AHB 和 stopped-only Core 通道；原 J-Link Server 环境仍可用于普通暂停调试，但不能提供这个 TCL 运行时通道。单核已做硬件验证，多核/多 CTI/多 AP 当前为真实双 GDB + 模拟 TCL 路由验证，需要相应多核板卡再做验收。

## 0.6 铜橙 / 暖石墨工作台

暖石墨背景、米白正文、铜橙焦点；绿色用于运行/成功，红色用于断点/错误。保留原有布局、源码标签、Console/Watch 输入和自动补全。

### 动画与反馈

在 Help 中选 **appearance**，或输入 `:appearance` 打开设置：

- **Off**：静态反馈；**Subtle**：默认，短暂高亮；**Full**：加入断点扫描、真实单步落点残影和少量粒子。
- 也可输入 `:animations off` / `:animations subtle` / `:animations full`；外观面板的 `u` 切换 Unicode / ASCII 动效字符。
- 连接阶段来自真实环境、GDB、目标连接事件；成功后短暂点亮标题。RUNNING 有低频状态提示，运行期间旧 PC 和数值缓存变暗。
- 真正停止后才强调 PC；断点短扫描、文件标签提示，单步最多保留 3 个实际执行落点的淡影。数值变化立即显示最终值，变化高亮淡出，变化圆点保持到下次刷新；可对齐的十六进制值强调变化数字，外设位域独立判断变化。
- 输入聚焦、自动补全、提交、Watch 新增、错误、鼠标悬停/点击和滚动都有短反馈。粒子只落在空白单元格，避开输入文字。
- Build / Download 在独立 Project 栏显示实际阶段和耗时，完成/失败状态保留；只有 GDB 报告真实下载计数时才显示百分比。
- 短动画上限 25 FPS，忙碌/运行提示约 2.5 Hz；静止且无过渡时不产生装饰重绘。终端报告失焦时暂停装饰动画，任务耗时仅在聚焦时更新。动效不增加 GDB 读取，不延迟输入或调试事件；无新增运行时依赖、字体、图片或常驻服务。

### Watch 面板增删

直接在 Watch 底部输入变量名或表达式，支持自动补全，点击 **+ Add** 或按 Enter 添加。输入为空时点击 **+ Add** 会聚焦输入框；添加成功后清空已提交内容，失败时保留输入供修改重试。

点击变量行右侧的 **×** 即可移除该变量，无需先选中。也可选中变量后按 **Delete**，或点击标题右侧的 **Del Remove**。删除后自动选择相邻项，可连续删除到空列表。输入框内 Delete 不删除观察项，草稿会保留；Console 焦点也不会触发 Watch 删除。运行中同样可以增删观察表达式，新加的值在下一次暂停时读取；移除观察项不修改目标变量或硬件观察点。

### Watch 结构体、数组和指针

- 输入结构体变量（例如 `object`），点击名称前的 **▸** 或选中后按 **Enter / →** 展开，**←** 折叠；支持嵌套结构体、数组和指针成员。Enter 在普通标量行仍聚焦 Watch 输入框。
- 只在暂停时读取展开的成员。折叠、删除不发送目标读取命令；运行期间显示上次暂停的快照。大数组每次显示 32 项，通过 **Load more** 继续加载；单个表达式最多 256 个节点、8 层，循环指针不会自动无限展开。
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

使用 `debugtui --demo` 预览布局（不连接调试器）；实际连接、断点、任务动画由相应事件触发。

## 运行方式

### SVD 外设寄存器

启动页的 **SVD file** 支持输入路径或 F2 选文件，也可用 `--svd FILE` 指定。配置保存为 `[program] svd = "./chip.svd"`，相对路径以工程配置目录为基准；留空不启用外设描述。

右侧标签顺序为 **System Regs → Peripherals → Stack → Memory → Breaks**。System Regs 显示 GDB 提供的 CPU/系统寄存器，Peripherals 根据 SVD 显示外设、寄存器及位域。

- 点击或 Enter 展开/收起，左右键展开/收起；支持滚轮和拖动滚动条。
- 仅在目标暂停、外设页可见时，读取已展开分组中当前可见的寄存器；每个停止代次读取一次，无定时轮询。隐藏、收起或目标运行时不发起自动读取，缓存值标为 `cached`。
- 选中寄存器或其位域，点击 **Refresh selected**、按 `r` 或执行 `:peripheral-refresh`，只读取对应寄存器。读取失败显示行内错误，可手动重试。
- 跳过只写寄存器；SVD 标记的寄存器/位域读取副作用会禁止自动读取，允许显式手动刷新。未在 SVD 声明的硬件副作用无法自动判断。
- 支持 `derivedFrom`、数组、cluster、属性继承和位域；按文件声明的大小端解码对齐的 8/16/32/64 位寄存器。未知字节序和不支持的宽度只展示定义，不猜测值。

SVD 解析器静态编译进 EXE，不增加运行时环境。SVD 文件按工程配置加载，不打入 EXE/npm 包；工程移交时请同时提供所引用的 SVD。

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
| 选择字段 | 鼠标点击 / Tab / Shift+Tab / ↑ ↓ |
| 编辑路径或参数 | Enter；Ctrl+U 清空；Enter 应用，Esc 取消 |
| 浏览文件/目录 | F2；Enter 进入目录或选择文件；Space 选择当前目录；Backspace 返回上层 |
| 切换枚举/开关 | ← → / Enter |
| 保存配置 | 顶部 Save / Ctrl+S |
| 开始调试 | 顶部 Start debugging 默认选中，Enter 即可启动；也支持鼠标、Ctrl+R、Ctrl+Enter、F5。默认保存到工程的 debug.toml，可关闭 Save to project 仅连接一次 |
| 从调试页面返回配置 | 主界面 Project 栏的 ← Setup / F2 / :setup |
| 回到原调试页面 | 配置页顶部 ← Workspace / Esc；编辑字段或浏览文件时 Esc 先取消当前操作 |

无参数启动始终先显示配置页，即使当前目录已有 debug.toml；不会直接占用探针。选择工程后优先读取其配置；没有显式 tools/GDB 配置时，会发现该工程内的 tools/debug-env.toml。保存路径尽量相对于工程，保留构建、源码映射、监视与断点等原有配置，不展开并复制整份 tools 配置。

配置页顶部固定显示启动、保存和返回工作区按钮，不随字段滚动。打开配置页时当前调试会话仍然有效；返回工作区会保留未保存的配置草稿。点击 Start debugging 时先校验并应用正在编辑的字段，再清理原会话、启动新配置；旧会话清理失败会在界面报错并停止切换。连接失败可以通过 ← Setup 修正配置并重试。Exit / Ctrl+Q 仍用于退出整个应用。

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

左侧为 Source / Asm / Files / Log，右上为 Regs / Stack / Memory / Breaks，右下为 Watch / Locals；三组标签独立切换。标题显示当前版本，Console 位于底部。

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

寄存器名称和编号全部从 GDB 动态获取，没有 ARM 白名单。内存默认地址为 $sp。run/r 默认保留 GDB 启动语义；只有显式加载环境中的 actions.run 时才执行自定义流程。下载操作执行前需要界面确认。

支持源码、监视、局部变量、栈、寄存器、内存、反汇编、断点和控制台。目标运行时显示上次暂停快照。FreeRTOS、SVD、内置源码编辑器尚未实现。源码由项目提供；路径映射使用 [[source_map]] 的 from/to。

### 构建与下载

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

### Console 历史与会话日志

- Console 右侧支持滚轮、点击轨道和拖动滑块。点击输出区后可用 ↑↓、PgUp/PgDn、Home/End 浏览；Shift+PgUp 可直接从输入框进入历史，保留未发送的草稿。
- 默认跟随最新输出，向上翻阅后保持当前位置；新消息显示在 **Latest (+N)** 计数中。点击 **Latest** 或在输出区按 **End** 恢复跟随。Enter 返回输入框，`/` 开始输入命令。
- Console 保留最近 2,000 行；Log 页保留最近 1,000 行，均为有界内存缓存。更早的记录从磁盘日志查看。
- 在启动页 **Log directory**、项目 `[session] log_dir` 或 `--log-dir` 设置日志目录后，每次连接（包括重连）新建 `session-YYYYMMDD-HHMMSS-mmm.log`，如 `session-20260917-171530-042.log`。同毫秒发生重名时追加序号，使用排他创建，旧文件不会覆盖；目录配置不变。默认不落盘，每次连接最多 8 MiB，达到上限时记录提示。
- 文件中每个物理行包含记录时间和会话已运行时长，例如 `[2026-09-17 17:15:31.276] [+1.234s] [gdb] Breakpoint 1, main ...`。Windows 使用本机时间（毫秒），其他平台标记 UTC `Z`；运行时长使用单调时钟，不受系统校时影响。Console / Log 页显示简短时间 `HH:mm:ss.mmm`。
- headless `log` 事件新增 `timestamp`、`elapsed_ms` 字段，`channel` / `text` 保持原有内容，结构化进度数据仍可直接解析。

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

## Console 与 Watch 补全

- 点击底部 `gdb>` 输入 GDB 命令，或输入 `:` 使用 DebugTUI 命令。输入时自动显示候选，支持 GDB 子命令及表达式参数。
- Watch 面板底部输入框直接接收全局变量名或表达式，点击 **+ Add** 或回车添加监视；也可以选中标量行后按 Enter 聚焦输入框。结构体行 Enter 展开/折叠，每个顶层表达式右侧的 **×** 直接移除该项。
- `↑` / `↓` 选择候选，`Tab` 或鼠标点击填入，`Enter` 提交。没有候选时，Console 的 `↑` / `↓` 浏览历史命令；`Esc` 退出输入。
- 变量名来自当前 ELF 的全局/静态符号；结构体成员（如 `object.field` / `pointer->field`）由 GDB 补全。符号补全在连接且未运行时可用，运行中不查询符号。
- 补全查询异步执行，输入停顿 150 ms 后查询；最多显示 64 个候选，可继续输入缩小范围。补全不执行命令，旧输入的延迟回复不会覆盖新输入。

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
