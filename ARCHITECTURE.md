# DebugTUI 架构

## 寄存器与内存访问开发分支（尚未发布）

本次只读版本按 [冻结 Goal](docs/registers-readonly-goal.md) 验收 M3/M4/M7 与 R52 的明确范围。每核有效配置、继承目录及条件复用公共模型；CorePrivate 的路由、归属与缓存不能按地址合并。R52 普通 CP15/MPU/selector 使用匹配的 `aarch64 r52_read` / `aarch64 r52_select`，在 OpenOCD 内取得当前 Debug EL2 证明并保存/恢复/回读；停止前 CPSR/DSPSR 不授权注入，恢复未知隔离服务。详细 [范围与模板](docs/registers-readonly-guide.md)、[软件/硬件用例入口](tests/cases/registers-readonly-release.md) 区分软件验收、待上板和后续功能。下列历史子系统记录不扩大本次范围。

`ui/registers/status.rs` shares row classification with expanded-row counts, excludes fields and groups, and requires current stopped context/owner for Valid. Its scrollable popup keeps complete selected-object reasons, raw provenance and sample time accessible. A local skipped-serde `Request` cancellation token passes through the coordinator for register, Watch resolution and explicit memory reads. Workers check between complete operations and before publication; MI/Tcl restoration is never interrupted by this token. Cancelled batches preserve old samples and cannot mask unknown restoration or broadcast to peer cores. See [status and cancellation](docs/register-read-status-and-cancel.md).

`registers.rs` 定义版本化目录、读取路由、位域、精确原始值、实现条件及显式 core/cluster/chip 归属；`session/registers.rs` 按请求读取，错误按条目隔离。`ui/registers.rs` 展示分组树并只请求当前可见条目。旧 `Snapshot.registers` 保留，新增样本包含来源、原因、状态和上下文；目录或读取失败不证明硬件缺失。

`session/register_matrix.rs` 提供纯缓存 `registers_matrix`，由 `registers/matrix.rs` 汇总类别、全依赖位宽/条件、配置路线及完整观察索引。协调器在响应边界重新核对共享 owner epoch、过滤 Probe 并重算矩阵；运行、旧上下文或未接受的共享值不成为当前观察。配置计划和读取结果分开保留，12 个 AArch32 计划类别的硬件支持保持 unverified，STM 目录和未观察类别均显式列出；[能力矩阵](docs/register-capability-matrix.md) 导出没有目标 I/O。

`launch/channels.rs` 编辑项目的 `memory_access` 草稿，不改写继承的工具 profile。配置加载记录实际来源。`ui/monitor.rs` 的 Watch／外设策略按芯片、核心和条目保存，绑定及迟到响应检查会话、停止代次和栈帧；Watch 地址仍由 GDB 在暂停时解析。

`Project::refresh_policy` 统一芯片/核心/条目、Watch 根继承及旧键优先级。外设默认 GDB 和命名 AP 均走统一 memory_read；`ui/memory_access.rs` 核对成功 receipt 的上下文、选中线程代次、实际命令/route、状态、时间和精确 raw。Watch 的 worker 绑定句柄缓存真实类型地址和线程/帧/PC，停止读取前重新证明，稳定运行 AP 不查询 MI；同帧线程通知通过 Snapshot.memory_selection_epoch 使 UI 失效。普通失败保留原值来源与时间；owner、路线变化清除或隐藏旧样本。见 [绑定与读取来源](docs/bus-read-provenance.md)。

`ui/memory.rs` 保存每芯片／核心的范围和通道，使用 `memory_dump` 请求。新接口限制 1–4096 字节；GDB 响应必须连续、完整并包含合法字节，总线响应按地址序解释为字节，不进行字节序重排。结果包含路由和上下文，由发起请求的视图验证后缓存；不通过无上下文的 `Snapshot.memory` 发布。切核、重连、切帧、范围或通道改变后丢弃迟到结果。暂停时可见面板每个停止点只读取一次，运行时仅手动读取配置声明允许的通道。旧 `memory` JSON 接口保持兼容。标量/范围及 GDB 外设读取共用 [响应边界](docs/bus-read-boundary.md)，消费已到达的 MI 通知并核对上下文、状态、通知代次和取消；成功结果提供实际命令 Access，GDB 配置 endpoint 与已确认连接分开。

`debug_access.rs` 在同一OpenOCD服务的GDB/Tcl请求间按endpoint串行化；不约束外部客户端。`session/banked.rs` 在同一租约内检查物理线程/frame0，并在单次target事务内检查独立v2协议。外部MIDR/EDSCR提供当前身份/EL，EL0/User与EL2/Hyp分别确定当前银行规则；EL1具体模式未知，不根据停止前DSPSR猜测，也不执行受约束不可预测的MRS CPSR。读取前后身份/执行状态和完整DSPSR/DLR一致，R0保存/恢复/物理回读；不使用旧mode-switch DPM。严格typed证明绑定owner/context/实际route/请求时间和传输方法，失败原值保留旧证明。Scope All只访问选中核，迟到上下文变化丢弃结果，不确定状态隔离通道。当前后端经Windows/Linux构建及本机离线验证，未连接板卡；完整EL1银行仍待完成，见[银行自检](docs/register-banked-proof.md)。

`session/register_adapter.rs` 为银行／VFP 共用物理线程、frame 0、服务租约和迟到上下文检查。`session/vfp.rs` 验证独立VFP v2协议及严格元数据／位宽，单个请求内按物理pair缓存D/S/Q，并保留pair的MVFR证据防止D16缓存授权Q。`registers/vfp.rs` 只识别手册规定的R52两种配置；控制原始值可保留未知字段，数据不依据未知字段授权。十五项Probe增加真实MVFR/FPEXC事实，未使能与未实现、工具不支持、访问受限分别保留原因。C事务先经外部Debug AP读取MIDR/EDSCR，每次核对HALT，以当前EL2/AArch32证明Hyp；保存DSPSR不独立授权。VMRS／两个GP与D的VMOV后，R0/R1保存恢复并物理回读，前后身份/执行状态、完整DSPSR/DLR/HCPTR/FPEXC一致才发布结果；无模式或FPU写入。当前Hyp路径有软件证据，EL1/Guest/User合法读取仍待适配，见[VFP证明](docs/register-vfp-proof.md)。

`session/timer.rs` 接入按 CP15 编码选择的独立 Timer 协议，复用 `register_adapter.rs` 的服务租约/实际线程/物理 frame 0 前后检查。生产后端从所选 core 的 debug AP 获取当前 MIDR/EDSCR，再在同一状态内读取单项 MRC/MRRC；前后完整 DSPSR/DLR、暂存恢复回读与状态一致才发布。低 EL 不可证明的上层使能保持 Unknown；未知恢复隔离服务并进入 FAULT。`registers/timer.rs` 严格检查原生名称/宽度及新鲜身份/权限，证据写入该次 `provenance.access.timer`，不成为可跨 stop 复用的 implementation 事实。

一次 MRRC 将同一值的两字放入 R0/R1，后续传输取得这个已复制的 pair；不同 Timer 条目分别请求，没有跨条目/跨核原子快照。`live_watch::TransactionProgress` 在开始写入和收到完整帧时记录 Instant；TCL 来源将终点写到 `Access.completed_ms`，MI 来源只在匹配请求 token 的结果记录后填充。起止都换算到当前 worker 的 session_started 单调时钟，记录传输区间，不声明精确核心时间；不同 worker 的数字不能作同步证据。未响应、未发送和旧 JSON 没有终点，不制造完成时间；失败与旧原值保留各自来源。Timer 新证据的 read_method 指明一次 MRC32/MRRC64，旧证据缺失该字段时保持 Unknown。详细一致性与环境限制见 [Timer 自检](docs/register-timer.md)。

## 0.8.3-local.3 多核断点

`ui/breakpoints/cores.rs` 提供选中断点的核心复选列表；新建源码断点不带核心范围。`coordinator/breakpoints.rs` 为显式关联的断点分配稳定 group 标识，各核仍拥有自己的 GDB 编号。`BreakpointOptions.group` 保存在各自 `[[cores]].breakpoints` 中；Session 维护本次连接的编号到 group 的映射，重连通过保存的定义重建。快照中的 `cores` 来自真实每核断点列表，不根据地址相同推测关联。

Headless `break_cores {number, cores:[0,1]}` 修改选中核某个已有代码断点的成员；数组必须包含当前核。关联记录的 `enable_break`、`update_break`、`delete_break` 分发到全部成员，用各核真实编号，批量启停包含当前列表涉及的关联成员。先验证所有受影响核处于 READY/STOPPED，再依序执行内部 `break_apply`；每步附有原定义的 undo，包括失败步，以覆盖 GDB 已修改但配置保存失败的情况。失败时反序恢复，保留原错误和回滚错误；始终以重新查询的快照显示结果。跨进程/配置文件写入不是崩溃原子事务。

多核断点成员与运行控制 Scope 独立：成员决定在哪些核安装断点；Scope / halt_peers 决定命中后是否联停。硬件资源由每核调试器报告，不硬编码数量。共享可写内存上的软件断点是指令补丁，不能保证核隔离，需要使用硬件断点。原始 GDB Console 命令仍由当前 GDB 处理。

## 0.8.1 断点管理

`ui/breakpoints.rs` 提供勾选开关、代码/数据断点编辑器、条件、忽略次数及当前核批量启停。UI 不预先修改快照，等待工作线程重新查询 GDB；一次只发送一个面板修改请求，输入框中的 Delete 不会删除断点。核心切换关闭旧编辑器，列表按 GDB 编号保留选择位置。

`session/breakpoints.rs` 使用标准 MI：`-break-insert`（含 `-h/-t/-d/-c/-i`）、`-break-watch`（写）、`-break-watch -r`（读）、`-break-watch -a`（读写）、`-break-enable/-disable/-condition/-after/-delete`。数据断点附加设置失败时删除刚创建的断点；编辑已有断点失败时尝试恢复原设置。失败后也查询实际状态，避免 UI 假成功。

Headless API 新增 `enable_break {number, enabled}` 或 `{all:true, enabled}`，`update_break {number, enabled?, condition?, ignore_count?}`。原 `break` 增加 `hardware/enabled/condition/ignore_count`；`data_break` 增加 `access: write|read|access` 及 `enabled/condition/ignore_count`。断点快照增加 condition、ignore_count、hit_count、address、pending、restore_error；旧字段保持兼容。

配置 `BreakpointSpec` 同时接受旧 location 字符串与详细对象。修改后立即通过原有偏好写锁合并至单核根列表或当前 `[[cores]]`，不写 tools。跨重连保存定义而非 GDB 编号；临时断点不恢复。无法恢复的数据断点保留为可见失败记录，允许重试或删除，不静默丢弃。硬件数量、读写范围和对齐由调试环境判断，TUI 没有芯片断点表。

等待暂停时，少数远程 GDB 会在运行中拒绝 `-thread-info`。对这一明确错误继续等待异步停止事件，沿用原有超时；不隐式重新连接或把运行目标标成已停止。

## 0.8 核心导航与显式内存通道

```mermaid
flowchart LR
    UI["Cores 按钮 / 每项刷新设置"] --> SELECT["当前核快照 + 同色工作区域"]
    UI --> POLL["可见项调度：间隔、单个在途请求、失败退避"]
    POLL --> SESSION["当前核 Session"]
    SESSION -->|"暂停时：表达式 → 地址/位宽/类型"| GDB["该核 GDB/MI"]
    SESSION -->|"默认 GDB 内存读取，暂停限定"| GDB
    SESSION -->|"显式 target read_memory"| TCL["配置的 OpenOCD TCL 通道"]
    TOOLS["tools：DAP/AP/CTI、可运行读取能力"] --> TCL
    TCL --> CPU["CPU target / 借核访问"]
    TCL --> BUS["mem_ap target / 总线访问"]
```

- `ui/cores.rs`：稳定核序号选色、核心状态按钮和溢出切换；Source/Asm 与右侧检查区加同色底色和边线。核心变化令按需视图缓存、外设值及实时采样失效，停止快照与运行采样不会混写。
- `ui/monitor.rs`：策略按核心 + 数据项保存到 `Ui.refresh`；Watch 树根策略可继承，子项可覆盖。采样只针对可见标量/展开成员，单个请求在途、候选公平轮转，用户动作和补全优先。响应携带本地核心身份与 generation 校验，切核/重连后迟到结果丢弃。错误至少 1 秒退避，频率非硬实时保证。
- `session/memory.rs`：`memory/watch.rs` 在有界只读表达式和 may-call-functions 保护下创建短寿命变量对象，取得成员路径、地址、类型及 ELF/PE 字节序，严格清理并复核线程/帧/PC；见 [Watch 地址解析](docs/watch-address-resolution.md)。`memory_read` 默认复用停止时 GDB 内存读取；命名通道使用显式目标 TCL 读取并校验当前核、运行能力、宽度、对齐、响应和超时。每通道按需复用一个 TCP，故障/断开关闭。64 位采用两次 32 位读取；不保证原子性。Session/Coordinator 传递局部读取取消，清理/恢复不会被中断。
- `memory_access` 和 `sync` 可放环境文件；工程优先覆盖。AP 号/CTI 映射仅存在于板级脚本，TUI 不配置隐式硬件触发或推断缓存一致性。运行态直读不发任何 GDB 指令，不改变全局 OpenOCD selected target，不隐式 halt/resume。
- 单核只配置 `memory_access` 时仍直接走原 Session，不增加协调线程。配置 `live_watch` 时，独立 TCL 采样器发送结构化 LiveWatchSample，Coordinator 绑定核和停止代次，UI 接入独立的实时显示缓存；普通未配置面板仍默认关闭轮询。

## 多核调度（0.7.2）

```mermaid
flowchart LR
    UI["TUI / JSONL"] <--> CO["Coordinator：命令队列、各核快照、共享服务生命周期"]
    CO <--> A["Session 0 → GDB 0 → endpoint 0"]
    CO <--> B["Session 1 → GDB 1 → endpoint 1"]
    CO --> SV["共享 GDB Server（可选）"]
    CO --> LW["Live Watch（可选，独立 TCL 连接）"]
    LW --> SV
```

没有 `cores/live_watch/sync` 时直接创建原 Session，不引入 Coordinator 线程。显式 cores 的数组位置是稳定的核序号；startup_order 仅决定全局启动命令的执行顺序。

Coordinator 将用户请求 ID 与内部逐核请求 ID 分离，每次只等待一个子请求，收齐结果后回复原请求。当前核的命令保持原 result 结构，全局命令带 `results[]`（含核索引、名字、成功与错误），后台核出错不会被忽略。子通道退出或响应逾时转为有界错误，退出取消标记向全部 Session 传递。

各核 Snapshot 分别缓存；输出当前核快照附带 `core` / `cores` 元数据，切核更新 generation、标题 endpoint 和按需视图缓存。后台核停止时更新状态，不覆盖当前核的寄存器或源码。每核 MI 日志在 `core-N/`，共享服务日志在 `coordinator/`。

配置文件写入串行合并，Watch/断点写回指定 core，空数组与继承顶层配置含义不同。启动页保存允许运行时这些列表变化，仍检测真正的配置冲突。

全局连接失败执行 disconnect 回滚再清理自有服务；Run 的部分成功显式返回，不声称硬件原子同步。外部 Build/Download 释放整个工作区的 GDB 和自有服务，成功后恢复连接。芯片 reset、CTI 和共享内存/缓存策略仍由环境配置决定。

## 边界

~~~mermaid
flowchart LR
    U["人工 TUI / AI JSONL"] <--> C["通用会话核心"]
    C <-->|"GDB/MI2"| G["用户选择的 GDB"]
    G <-->|"远程协议"| S["外部调试服务"]
    S <--> T["目标芯片"]
    G <--> L["本机程序"]
    E["tools 环境配置"] -->|"启动参数与动作"| C
    C -.->|"可选通用进程启动"| S
~~~

基础连接与服务启动流程不硬编码芯片、探针品牌或预设 monitor 命令。扩展寄存器由目录和经过验证的架构 reader 决定支持范围；专用 Timer/VFP 等路径有各自编码与实际身份检查。GDB 路径、资源目录、服务程序、就绪标记、连接方式和特殊动作由配置决定。tools 是可选环境层，可以来自另一个目录或仓库。

环境文件是数据驱动的通用协议：核心只知道如何启动命令、等待配置声明的就绪标记、发送配置声明的 GDB 命令。STM32 的 GDB/OpenOCD/探针具体内容集中在 tools/debug-env*.toml。不配置服务时只启动 GDB。

## 启动与状态

无参数启动先进入 TUI 配置页，不启动 GDB 或占用探针。CLI 参数和界面编辑使用同一份项目文档模型；参数完整时可以直接连接，--setup 强制先进入配置页。工程目录中的 tools/debug-env.toml 可以自动发现，TUI 安装目录不作为工具查找路径。

配置页顶部固定放置 Start debugging / Save / Workspace，默认聚焦启动；鼠标、Enter、Ctrl+R、Ctrl+Enter、F5 共用启动校验。主界面的 Project 栏提供 ← Setup，F2 仍可使用。返回配置保留活动会话，Workspace / Esc 保留编辑草稿并回到原工作区。启动动作提交正在编辑的字段并校验输入后，向旧会话发送 quit，等待清理响应和 Exit，再保存项目、清空旧核及监视面板状态、创建新会话并连接。清理失败时停止切换。配置浏览和输入不执行硬件命令。

1. 加载可选 tools 环境，合并项目字段，再应用 CLI 参数。
2. 校验通用配置及可选符号文件，不校验某个 tools 二进制目录布局。
3. 按需启动配置中的服务，读取 stdout/stderr 就绪标记；--connect/--local 跳过服务。
4. 启动指定 GDB，使用 MI2，通过管道交换命令与异步事件。
5. 加载可选符号和源码映射，执行 gdb.init。
6. 选择 remote/extended-remote 或保持 local，执行显式 after_connect。
7. 查询实际线程和栈状态；无运行目标为 READY，暂停为 STOPPED，运行中为 RUNNING。
8. 动态获取寄存器和面板数据。每个动作的实际停止由异步事件确认。

默认 run 使用 -exec-run；restart 与 GDB download 动作来自环境。跨架构差异由匹配目标的 GDB 处理。默认连接和退出不发送 monitor 命令。

项目 `[tasks]` 可配置 build / download 外部命令，启动页直接编辑；工作目录固定为 `program.source_root`。命令由系统 shell 在会话工作线程中执行，stdout/stderr 通过现有事件队列传到 Console。Windows 子进程归属独立 Job，退出/超时时清理。外部任务先释放已连接的 GDB / 自有服务，成功后重连以重新加载 ELF；失败保持断开。脚本下载优先于环境 GDB 下载动作；旧 `[build]` 作为兼容回退。主界面以独立 Project 工具栏呈现，不与调试执行控制混排。

Pause 先发送标准 MI interrupt，等待异步停止通知。超过 250 ms 未收到通知时，通过 -thread-info 核对线程状态；全部线程确认为 stopped 后才同步状态并读取栈、变量和寄存器。如果仍在运行，最多补发一次 interrupt，保留有界超时。GDB 报告“目标已停止”的中断竞争也走同一核对流程；不通过 reset、monitor 或重连伪造暂停成功。断开会话复用此暂停逻辑。开启 log_dir 时，日志同时记录 MI 异步事件以供诊断。

## 模块

| 文件 | 职责 |
|---|---|
| cli.rs | 启动参数解析、界面预填和直接启动选项 |
| launch.rs | 工程文档、启动表单、文件浏览器、相对路径保存 |
| config.rs | 项目/环境合并、相对路径、通用启动与动作配置 |
| session.rs | MI 请求队列、异步状态、通用进程管理、动态数据 |
| logging.rs | 日志捕获时间、单调会话计时、排他创建文件和逐行时间戳 |
| mi.rs | MI 记录和转义解析 |
| ui.rs | 输入、面板焦点、鼠标命中、视图按需请求和下载确认 |
| ui/render.rs | 双栏工作台、工具栏、滚动条、命令列表及窄终端布局 |
| theme.rs | 原生终端配色、分层面板、全行高亮、圆角弹窗及背景遮罩 |
| ui/highlight.rs | 轻量 C 类语法着色；缓存当前文件每行的注释起始状态，仅对可见行生成 Span |
| ui/source_tabs.rs | 源码文件标签、路径去重、独立浏览位置、溢出导航及可搜索的已打开文件列表 |
| main.rs | CLI 和 JSONL |
| process.rs | 宿主进程生命周期；Windows Job Object |
| tools/ | 可选工具、厂商配置、启动脚本和许可证 |

环境覆盖按字段合并，数组整体替换。相对启动路径基于定义该路径的文件，避免配置迁移后依赖启动目录。会话退出只保存项目的监视和断点。启动表单保存项目显式设置和环境引用，保留其余配置，不将整份已展开的环境配置写回项目；检测到外部修改启动设置时要求重新加载。

## 工作台交互

视觉层继续使用 Ratatui/Crossterm 的单元格差分输出，没有浏览器、GPU 画布、图片或字体依赖。调试等待状态最多每 250 ms 更新一次本地状态符号；无请求且无输入/会话事件时不绘制。悬停只更新 UI，不发送 GDB 请求。所有点击区域以实际绘制的终端字符宽度计算。弹窗先遮罩和清空区域，再绘制不透明面板，避免底层文字残留；PC、变化值和选择行背景明确铺满视口宽度。

主视图区保留 Source / Asm / Files / Log，右上检查区为 Regs / Stack / Memory / Breaks，右下变量区为 Watch / Locals。三组分别记录活动标签，每个视图记录浏览位置、内容区域和滚动条坐标。滚轮、拖动和键盘共享视口边界计算；Files / Stack 点击使用视口起始行定位，Log 浏览历史时暂停自动跟随，End 恢复跟随。Console 内固定输入框接收 GDB 原生命令及冒号前缀的工作台命令，复用同一 Request 分发与历史记录。版本标题来自 Cargo 包元数据。

Source 的打开文件集合与当前 GDB 栈帧分别维护。所有打开入口按映射后的规范文件路径去重；停止事件定位对应文件，手动切换只变更浏览视图。标签只保存路径和光标/滚动位置，源码文本只保留当前文件，切换时按需读取，单文件继续限制为 2 MiB / 30000 行。标签栏按终端字符宽度截断长名称，保留关闭按钮命中区域；新激活文件和窗口缩放会自动确保活动标签可见，翻页浏览不会改变活动文件。无源码位置的停止保留已有标签，但清空活动源码视图，避免显示旧位置。

Asm、Memory、Files 按可见性加载。Asm / Memory 使用停止代次、栈帧地址及层级作为刷新标识；等待在途命令完成后提交请求，避免复位尚未完成时查询旧 PC。GDB 异步停止或栈帧变化会清除旧内存/汇编快照。查询失败保留错误提示，显式 refresh 或下一次停止再重试，不进行高频轮询。

Reconnect 在会话核心中顺序执行 disconnect 和 connect；清理失败则停止，不隐式另开服务。工具栏的 Reset 使用现有环境 restart 配置，所有硬件差异继续由环境层决定。

## 退出与所有权

Console 和 Watch 使用独立输入缓冲区、共享补全弹窗。补全等待 150 ms 输入停顿后，经现有 Engine 通道发送 `complete` 请求；每次最多一个在途查询，回复按输入、焦点、会话状态、停止代次及前台命令失效标识匹配。工作台冒号命令本地匹配，GDB 命令和成员表达式使用 `-complete`，裸变量前缀使用 `-symbol-info-variables`。查询不会提交用户输入为执行命令；补全响应不进入普通 Console 日志，运行状态不查询符号。

TUI 只终止自己启动的 GDB/服务/构建进程。默认 detach；resume/disconnect 可配置。before_disconnect 在停止状态下执行。目标最终运行状态还受远端 detach、服务退出和探针策略影响，不由 UI 猜测。

Windows 使用 Job Object 清理意外终止后的自有进程。非 Windows 分支尚未提供同等级进程树回收，当前发行和实测范围为 Windows x64。

## 分发

- npm tgz / 便携 ZIP：TUI + 文档 + 本程序依赖许可证。
- tools ZIP：独立的 GDB、服务程序、配置和原厂许可证。
- Git：DebugTUI 源码和配置；应用 EXE 始终在 ignore 中。现有可选 tools 依赖继续单独维护。

人工和 AI 使用同一核心、同一协议。当前两者是不同启动模式，没有多客户端会话服务；运行时没有 Node/Python 常驻依赖。

## SVD peripheral view

`program.svd` belongs to the project and resolves relative to its configuration file. The startup UI and optional `--svd` argument select it. `svd.rs` parses CMSIS-SVD, resolves inheritance/arrays, and retains a compact device/register/field model; it has no probe or STM32-specific runtime logic.

The Peripherals UI owns expansion state and a cache keyed by register and stopped generation. Only visible rows under expanded groups request data, one register at a time through the existing engine channel. The generic `peripheral_read` method accepts a numeric address, bit width and byte order, checks STOPPED state, and issues one GDB/MI memory read. Results do not modify the separate Memory view. Manual refresh uses the same path; write-only registers are never read, and SVD readAction registers/fields skip automatic refresh.

## UI effects and numeric presentation (0.6)

`ui/effects.rs` consumes existing session snapshots, command results, input, focus and scroll events. Bounded pulses (256), displayed-value history (2048) and real stop traces (3) paint cell backgrounds/foregrounds after layout. The scheduler runs at 25 FPS only during transitions, 2.5 Hz for activity and stops decorative redraw when idle/unfocused. No GDB access is possible from the effects module.

Console 输出使用 `ui/console.rs` 独立视口，包含历史起点、实时跟随开关、新消息数和鼠标拖动状态；与命令输入框、各数据面板的选择位置分离。新输出不会移动历史视口，缓存淘汰时调整起点保留相同记录。历史操作不向 GDB 发送请求。

`logging::Stamp` 在产生日志时捕获时间；子进程 stdout/stderr 在读取线程入队时捕获，Engine 刷队列时保留这个时间。每次连接先刷旧队列再新建 `session-日期-时间-毫秒.log`，`create_new` 避免覆盖。原始 Log.text 不加前缀，文件 writer 为多行输出逐行加绝对时间和会话时长；UI 单独渲染简短时间。Windows 时间由系统 API 提供，未增加运行时库。

`ui/formats.rs` converts scalar integer strings using u128 arithmetic. Preferences are keyed per Watch expression, local file/function/name, system register, SVD device/peripheral/register/field, or memory byte address. Unsupported natural values are preserved. UI saves use a serialized `ui_preferences` worker request; it merges only [ui] into the latest project document and sends no MI. Setup saves preserve concurrent runtime UI preferences alongside watches and breakpoints.

GDB download status records can emit a progress log from actual total-sent / total-size counters. Shell tasks use real stage and elapsed time when no structured progress exists. Animation never infers a successful stop, advances a numeric value, or runs a debug request.

## Multicore control

`coordinator/control.rs` separates control scope from selected inspection core. The default preserves independent Continue/Pause; `multicore.scope = "all"` broadcasts them in startup order. Run initializes each worker once per connection/shared reset and thereafter continues it. Running workers are skipped. Shared reset has one owner and refreshes all register caches after the board command. Remote workspace connect also refreshes every worker after per-core connect hooks, since one hook may reset the whole chip.

A new stopped generation schedules an internal peer-halt batch ahead of queued user requests and focuses its cause. Pending resumes are discarded once a group stop is observed. Internal pause replies are consumed by the coordinator and never reuse user response IDs. Explicit stepping/pausing and connect/reset refreshes do not recursively trigger another group halt. Pause attempts remaining cores after one fails; a failed group resume stops peers already running. This is software coordination with transport latency, not CTI synchronization.

`coordinator/server_log.rs` frames service stdout/stderr bytes into complete UTF-8 lines while recognizing readiness markers without requiring a trailing newline. Normal stderr uses `server-stderr`; actual OpenOCD error lines retain `server-error`. Logging remains bounded and preserves line capture timestamps.

## Native Watch trees

`session/watch.rs` uses GDB/MI `-var-create`, `-var-list-children --all-values`, and `-var-delete`. Every refresh creates temporary objects in the currently selected frame, reads only expanded branches, then deletes the root and all its children. No Python pretty printers are enabled. Scalar expression evaluation remains the fallback for GDBs without variable-object support. Empty scalar values are evaluated again through the object to retrieve unreadable-memory diagnostics.

`Snapshot.watches` remains the list of root expressions. Optional `Variable.tree` metadata contains type, relative child-index path, child count, expanded state, children and pagination/limit flags. `watch_expand` takes `{expression, path: [], expanded: true, more: false}`; expanding requires STOPPED, while collapsing and unwatching are local operations with no MI. Pages contain 32 children; each root is bounded to 256 nodes and 8 expansion levels. Session-owned expansion maps isolate cores and are cleared when removing the expression.

`ui/watch.rs` flattens visible nodes for scrolling, selection and hit testing. Root identity plus child path gives each member a separate display-format key. Only root rows have removal controls; collapsing a subtree moves a hidden selection to its nearest visible ancestor. The existing single-core worker path and root Watch persistence format remain unchanged.

PMU 的独立 pmu_command 根据原始编码选择固定原生读者；它从当前 Debug AP 与系统寄存器获取新鲜身份、EL2、PMCR.N、HDCR 和选择器证据，再以直接 MRC 索引或单条 MRRC64 读取。数据后完整校验，只有 scratch 恢复并回读成功才发布。Scope All 不广播寄存器读取；样本/Probe/旧值分别保留各自 core、context、实际 route 和 Access.pmu。计数控制、过滤器、选择器及模式不会被写。物理 PMU 容量不再由停止前 CPSR.M=Hyp 推断；详见 [PMU 读取证据](docs/register-pmu.md)。

GIC 的独立 `gic_command` 同样按精确 reader 编码分发，禁止任意编码/位宽伪装。OpenOCD 先从外部 AP 建立当前 Debug EL2/身份，再以生产保存/恢复/回读 R0 的 MRC helper 读取十项状态/权限/容量证明，数据后完整核对。物理 ICC 与虚拟 ICV/ICH 的容量分别由 CTLR/VTR 解码，Probe 只接受当前 context、PhysicalCore、完整响应时间及匹配 MIDR/值/接口的 Access.gic；旧 CPSR 不能替代当前权限。ICV AP 在目录上是 ICH backing 的显式 alias，复用同一值和 provenance，不执行 EL2 ICV 指令或广播 Scope All。LR/LRC 分开 MRC32，不提供跨项原子快照。IAR 及 WO 控制不属于观测数据；低 EL、错协议、未知身份/容量、故障均无回退。详见 [GIC 自检](docs/register-gic.md)。

R52 MMIO 目录通过统一 session/registers → read_memory_channel 进入 GDB/AP 路线，owner 先由显式拓扑决定，再查 registers.component_owners 的完整 core:/cluster:/chip: 键；缺失即拒绝，不按数值核心标签推导 base。新 reader.require_owner_mapping 防止私有窗口落到旧全局静态 base；旧用户定义默认 false。GICD cluster 共享和 GICR/Debug core 私有沿用既有上下文与协调器 owner_generation，实际请求来源与保留原值来源继续分开。内置目录解析由 OnceLock 持有，用户目录仍按明确文件来源加载；没有地址自动发现。[MMIO 自检](docs/register-mmio.md) 与默认无 I/O 的延后驱动记录软件证据和剩余范围。

可选 registers.mmio_probe=true 通过固定三组件有界 RO 请求形成当前物理身份证明；数据路径要求同 context 的有效组件证明。独立 mmio_probe.ID/.after 样本保留实际 memory route/aperture、前后区间及 owner；GIC组被Debug前后复核包围，按手册精确 R52字段解码，不使用逻辑core名称推断地址或affinity。coordinator对 Probe 和普通样本同样绑定/验证共享owner epoch；peer生命周期、请求期间epoch变化和worker FIFO失效均清除相关共享事实。原配置单独保存、未知不变成No、64位两字非原子；完整范围见 [MMIO Probe](docs/register-mmio-probe.md)。

VFP数据响应在 `provenance.access.vfp_pair` 记录首个D编号、完整128位pair和同次MVFR0/MVFR1/FPEXC。`PairEvidence`验证存储容量/位宽/视图范围；session请求级ReadCache共享完整原始来源，Alias继续记录独立的source/offset/bits。S/D/Q的派生及显示使用逻辑位序，lane0是低位，不依赖内存端序。该元数据描述已取得的存储，不授予执行权限；v2另以 `provenance.access.vfp` 保存外部MIDR/EDSCR、停止DSPSR/DLR及HCPTR，控制值也保留相同访问证明。旧JSON缺字段不制造证明；完整低EL后端适配继续在REG-304跟踪。[存储视图](docs/register-storage-views.md)、[当前状态自检](docs/register-vfp-proof.md)。

`registers/stm.rs` 复用 MMIO 原始请求证据，独立于 CPU MIDR 验证 STM v1.1 核心及可选控制类；`session/stm.rs` 仅由显式 chip owner 映射启用，错误 identity 或后验变化立即停止后续地址。核心前后各15项包围可选HWE/DMA前后请求；所有路线、owner/epoch和请求顺序都验证。`session/registers.rs` 对内置和自定义 STM reader 均执行当前 Proof、定义地址、位宽和功能过滤，不能靠配置事实或手工读取绕过。复用共享chip生命周期、服务锁和请求取消；读选择器本身但不修改它，未实现新writer/Trace数据路径。[STM范围](docs/register-stm.md)。
